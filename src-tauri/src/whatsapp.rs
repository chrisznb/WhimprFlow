//! Reads voice messages out of the WhatsApp desktop app's own storage, so a
//! transcript can be had without downloading each file by hand first.
//!
//! # What this touches
//!
//! WhatsApp keeps everything under
//! `~/Library/Group Containers/group.net.whatsapp.WhatsApp.shared`: the audio
//! as `.opus` files under `Message/Media`, and the metadata in a Core Data
//! database, `ChatStorage.sqlite`. The file names are hashes, so the database
//! is the only way to say which message belongs to which chat.
//!
//! Only voice messages and the columns needed to label them are read, and the
//! database is opened read-only so a half written read can never corrupt the
//! user's chat history. Message text is never touched, with one opt-in
//! exception: the user's chat with themselves (see [`export_self_chat`]).
//!
//! # Files written
//!
//! All under `~/Library/Application Support/WhimprFlow`:
//!
//! - `whatsapp-transcripts.json`: `{media id: transcript}`, the cache. Format
//!   unchanged so older versions keep reading it.
//! - `whatsapp-transcripts-meta.json`: the same transcripts with time
//!   (Europe/Berlin), chat and sender, see [`TranscriptMeta`]. Filled in from
//!   ChatStorage by [`sync_meta`], including for transcripts older than the file.
//! - `whatsapp-selfchat.json`: only when `whatsapp-export.json` names the own
//!   chat; typed notes and voice messages of that one chat, by date.
//!
//! # Full Disk Access
//!
//! macOS protects that directory. Without the permission the app sees an empty
//! folder rather than an error, which is indistinguishable from "no voice
//! messages" - so [`access`] probes it explicitly and the UI can say what is
//! actually wrong.
//!
//! # WhatsApp Business
//!
//! Not supported, and not an oversight. The Business client is normally run as
//! a web app, which keeps its data inside the browser profile rather than as
//! files on disk. Its native container
//! (`group.net.whatsapp.WhatsAppSMB.shared`) exists but stays empty unless the
//! native Business app is installed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Apple's reference date: Core Data counts seconds from 2001-01-01, unix time
/// counts from 1970-01-01.
const APPLE_EPOCH_OFFSET: i64 = 978_307_200;

fn container() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join("Library/Group Containers/group.net.whatsapp.WhatsApp.shared")
}

fn media_dir() -> PathBuf {
    container().join("Message/Media")
}

fn db_path() -> PathBuf {
    container().join("ChatStorage.sqlite")
}

/// The address book lives in its own database, not in ChatStorage.
fn contacts_db_path() -> PathBuf {
    container().join("ContactsV2.sqlite")
}

/// Why the voice message list might be empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// The container can be read.
    Ok,
    /// It exists but macOS denies access: Full Disk Access is not granted.
    Denied,
    /// No WhatsApp desktop app has ever stored anything here.
    Missing,
}

/// Probe the container. Reading the directory is the honest test: the
/// permission is enforced on access, so `exists()` alone would pass while every
/// later read returns nothing.
pub fn access() -> Access {
    let dir = media_dir();
    match std::fs::read_dir(&dir) {
        Ok(_) => Access::Ok,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // The whole container missing means WhatsApp was never installed.
            // The container present but the media folder gone is a permission
            // problem wearing a NotFound coat, which macOS does for protected
            // paths in some releases.
            if container().is_dir() {
                Access::Denied
            } else {
                Access::Missing
            }
        }
        Err(_) => Access::Denied,
    }
}

/// The raw result of the access probe, for the debug log: "ok" or the OS error.
pub fn probe_detail() -> String {
    match std::fs::read_dir(media_dir()) {
        Ok(_) => "ok".to_string(),
        Err(e) => format!("{:?}: {e}", e.kind()),
    }
}

/// One chat that has at least one voice message.
#[derive(Debug, Clone, Serialize)]
pub struct Chat {
    pub id: i64,
    pub name: String,
    pub is_group: bool,
    pub count: usize,
    /// Unix seconds of the most recent voice message, for sorting.
    pub last_ms: i64,
}

/// A single voice message.
#[derive(Debug, Clone, Serialize)]
pub struct VoiceMessage {
    /// Stable id, used as the cache key. This is the media row's primary key.
    pub id: i64,
    pub chat_id: i64,
    /// Who spoke. Empty in a one to one chat, where the chat name says it.
    pub sender: String,
    pub from_me: bool,
    pub sent_ms: i64,
    pub duration_s: i64,
    /// Absolute path to the .opus file, or None when the audio was never
    /// downloaded to this machine.
    pub path: Option<String>,
    /// Transcript, once it exists.
    pub text: Option<String>,
}

/// Open the database read-only. WhatsApp writes to it while running, so
/// `immutable` is deliberately NOT used: that would ignore the write-ahead log
/// and hide every recent message.
fn open_db() -> Result<rusqlite::Connection, String> {
    let p = db_path();
    if !p.is_file() {
        return Err("ChatStorage.sqlite not found".into());
    }
    rusqlite::Connection::open_with_flags(
        &p,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|e| e.to_string())
}

fn apple_to_unix(secs: f64) -> i64 {
    secs as i64 + APPLE_EPOCH_OFFSET
}

/// Resolve a stored media path to a real file.
///
/// The database holds a path relative to the container, but older rows hold
/// something else entirely, so the file name is used as a fallback search key.
fn resolve_media(stored: &str) -> Option<String> {
    let direct = container().join(stored);
    if direct.is_file() {
        return Some(direct.display().to_string());
    }
    let name = Path::new(stored).file_name()?.to_str()?;
    let found = find_by_name(&media_dir(), name, 0)?;
    Some(found.display().to_string())
}

/// Depth limited search for a file name under the media tree. WhatsApp nests by
/// account and chat, so a plain read_dir of the top level finds nothing.
fn find_by_name(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    if depth > 4 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            subdirs.push(p);
        } else if p.file_name().and_then(|n| n.to_str()) == Some(name) {
            return Some(p);
        }
    }
    subdirs
        .into_iter()
        .find_map(|d| find_by_name(&d, name, depth + 1))
}

/// Every chat that contains at least one voice message, most recent first.
pub fn chats() -> Result<Vec<Chat>, String> {
    let db = open_db()?;
    let mut stmt = db
        .prepare(
            "SELECT cs.Z_PK, cs.ZPARTNERNAME, cs.ZSESSIONTYPE,
                    COUNT(mi.Z_PK), MAX(m.ZMESSAGEDATE)
             FROM ZWAMEDIAITEM mi
             JOIN ZWAMESSAGE m ON mi.ZMESSAGE = m.Z_PK
             JOIN ZWACHATSESSION cs ON m.ZCHATSESSION = cs.Z_PK
             WHERE mi.ZMEDIALOCALPATH LIKE '%.opus'
             GROUP BY cs.Z_PK
             ORDER BY MAX(m.ZMESSAGEDATE) DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |r| {
            let name: Option<String> = r.get(1)?;
            let stype: Option<i64> = r.get(2)?;
            let last: Option<f64> = r.get(4)?;
            Ok(Chat {
                id: r.get(0)?,
                name: name.unwrap_or_else(|| "Unknown".into()),
                // Session type 1 is a group; everything else is one to one.
                is_group: stype == Some(1),
                count: r.get::<_, i64>(3)? as usize,
                last_ms: apple_to_unix(last.unwrap_or(0.0)),
            })
        })
        .map_err(|e| e.to_string())?;

    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// Voice messages of one chat, newest first, with cached transcripts filled in.
pub fn messages(chat_id: i64) -> Result<Vec<VoiceMessage>, String> {
    let db = open_db()?;
    let mut stmt = db
        .prepare(
            "SELECT mi.Z_PK, m.ZISFROMME, m.ZMESSAGEDATE, mi.ZMEDIALOCALPATH,
                    mi.ZMOVIEDURATION, gm.ZMEMBERJID, m.ZFROMJID
             FROM ZWAMEDIAITEM mi
             JOIN ZWAMESSAGE m ON mi.ZMESSAGE = m.Z_PK
             LEFT JOIN ZWAGROUPMEMBER gm ON m.ZGROUPMEMBER = gm.Z_PK
             WHERE m.ZCHATSESSION = ?1 AND mi.ZMEDIALOCALPATH LIKE '%.opus'
             ORDER BY m.ZMESSAGEDATE DESC",
        )
        .map_err(|e| e.to_string())?;

    let raw = stmt
        .query_map([chat_id], |r| {
            let stored: Option<String> = r.get(3)?;
            let dur: Option<f64> = r.get(4)?;
            let date: Option<f64> = r.get(2)?;
            let member: Option<String> = r.get(5)?;
            let from: Option<String> = r.get(6)?;
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<i64>>(1)?.unwrap_or(0) == 1,
                apple_to_unix(date.unwrap_or(0.0)),
                stored,
                dur.unwrap_or(0.0) as i64,
                member.or(from),
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let names = contact_names();
    let cache = load_cache();

    Ok(raw
        .into_iter()
        .map(|(id, from_me, sent_ms, stored, duration_s, jid)| {
            let sender = if from_me {
                String::new()
            } else {
                jid.as_deref()
                    .and_then(|j| names.get(j).cloned())
                    .unwrap_or_default()
            };
            VoiceMessage {
                id,
                chat_id,
                sender,
                from_me,
                sent_ms,
                duration_s,
                path: stored.as_deref().and_then(resolve_media),
                text: cache.get(&id.to_string()).cloned(),
            }
        })
        .collect())
}

/// Map of sender id to display name, so a group message can name its sender.
///
/// Two things make this less obvious than it looks. The address book is in
/// `ContactsV2.sqlite`, a different database from the messages, and modern
/// WhatsApp identifies group members by a "LID" (`12345@lid`) rather than by
/// phone number, so the join goes through `ZLID`. Read in one go: a query per
/// message would be one per row.
fn contact_names() -> HashMap<String, String> {
    let mut out = HashMap::new();
    let p = contacts_db_path();
    if !p.is_file() {
        return out;
    }
    let Ok(db) = rusqlite::Connection::open_with_flags(
        &p,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    ) else {
        return out;
    };
    let Ok(mut stmt) = db.prepare(
        "SELECT ZLID, ZWHATSAPPID, ZFULLNAME, ZGIVENNAME, ZBUSINESSNAME
         FROM ZWAADDRESSBOOKCONTACT",
    ) else {
        return out;
    };
    let rows = stmt.query_map([], |r| {
        let lid: Option<String> = r.get(0)?;
        let wid: Option<String> = r.get(1)?;
        let full: Option<String> = r.get(2)?;
        let given: Option<String> = r.get(3)?;
        let biz: Option<String> = r.get(4)?;
        Ok((lid, wid, full.or(given).or(biz)))
    });
    let Ok(rows) = rows else { return out };
    for (lid, wid, name) in rows.flatten() {
        let Some(name) = name.filter(|n| !n.trim().is_empty()) else {
            continue;
        };
        // Index every spelling a message row might use: the LID with and
        // without its suffix, and the phone number for older rows.
        for key in [lid, wid].into_iter().flatten() {
            let bare = key.split('@').next().unwrap_or(&key).to_string();
            out.insert(key, name.clone());
            out.insert(bare, name.clone());
        }
    }
    out
}

// --- transcript cache ------------------------------------------------------

/// Transcripts keyed by media row id. Kept next to the app's other state rather
/// than inside WhatsApp's container, which is read-only territory.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Cache(HashMap<String, String>);

fn cache_path() -> PathBuf {
    support_dir().join("whatsapp-transcripts.json")
}

fn load_cache() -> HashMap<String, String> {
    std::fs::read_to_string(cache_path())
        .ok()
        .and_then(|s| serde_json::from_str::<Cache>(&s).ok())
        .map(|c| c.0)
        .unwrap_or_default()
}

/// Store one transcript. Read, insert, write: the file is small and written
/// rarely, so the simple approach is the right one.
pub fn cache_put(id: i64, text: &str) {
    let mut c = load_cache();
    c.insert(id.to_string(), text.to_string());
    if let Ok(json) = serde_json::to_string(&Cache(c)) {
        let _ = std::fs::write(cache_path(), json);
    }
    // Date and chat for the new transcript. Best effort: the transcript itself
    // is already safe in the cache, and the next sync picks up a miss.
    if id >= 0 {
        let _ = sync_meta();
    }
}

pub fn cache_get(id: i64) -> Option<String> {
    load_cache().get(&id.to_string()).cloned()
}

// --- time ------------------------------------------------------------------

/// Unix seconds as an ISO 8601 time in Europe/Berlin, with the offset spelled
/// out (`2026-10-18T21:04:05+02:00`), so a reader can filter by local day
/// without knowing anything about time zones.
///
/// Hand rolled rather than pulling in a time zone database: Germany follows the
/// EU rule (summer time from the last Sunday of March to the last Sunday of
/// October, switching at 01:00 UTC both ways), unchanged since 1996.
pub fn berlin_iso(unix: i64) -> String {
    let offset_h = if berlin_is_summer(unix) { 2 } else { 1 };
    let local = unix + offset_h * 3600;
    let (y, mo, d) = civil_from_days(local.div_euclid(86_400));
    let secs = local.rem_euclid(86_400);
    format!(
        "{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}+{offset_h:02}:00",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

fn berlin_is_summer(unix: i64) -> bool {
    let (year, _, _) = civil_from_days(unix.div_euclid(86_400));
    let start = last_sunday_utc(year, 3) + 3600;
    let end = last_sunday_utc(year, 10) + 3600;
    unix >= start && unix < end
}

/// Unix seconds of 00:00 UTC on the last Sunday of a month.
fn last_sunday_utc(year: i64, month: i64) -> i64 {
    // The last day of the month is the day before the 1st of the next one.
    let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    let last = days_from_civil(ny, nm, 1) - 1;
    // 1970-01-01 was a Thursday, so day 3 was the first Sunday.
    let weekday = (last + 4).rem_euclid(7); // 0 = Sunday
    (last - weekday) * 86_400
}

/// Days since 1970-01-01 for a calendar date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The inverse: calendar date for days since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

// --- transcript metadata ---------------------------------------------------

/// One transcribed voice message: the text plus when, in which chat and
/// whether it was sent by the user. Written to its own file so tools outside
/// the app (a weekly report, say) can filter by date and chat without ever
/// needing access to WhatsApp's database themselves.
///
/// Deliberately no sender ids or address book names: that would copy contact
/// data out of WhatsApp's protected container for no use anyone asked for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptMeta {
    pub text: String,
    /// Europe/Berlin, ISO 8601 with offset.
    pub sent_at: String,
    /// The same moment as unix seconds, for sorting and arithmetic.
    pub sent_unix: i64,
    /// The chat's WhatsApp id, e.g. `49151...@s.whatsapp.net` or `...@g.us`.
    pub chat_jid: String,
    pub chat_name: String,
    pub is_group: bool,
    pub from_me: bool,
    pub duration_s: i64,
}

/// The metadata file. A version number so a later format change can be told
/// apart from this one.
#[derive(Debug, Default, Serialize, Deserialize)]
struct MetaFile {
    version: u32,
    /// Keyed by the same media row id as the transcript cache.
    entries: std::collections::BTreeMap<String, TranscriptMeta>,
}

fn support_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join("Library/Application Support/WhimprFlow")
}

fn meta_path() -> PathBuf {
    support_dir().join("whatsapp-transcripts-meta.json")
}

fn load_meta() -> MetaFile {
    std::fs::read_to_string(meta_path())
        .ok()
        .and_then(|s| serde_json::from_str::<MetaFile>(&s).ok())
        .unwrap_or_default()
}

/// Write via a temporary file and rename, so another program reading the file
/// at the same moment sees either the old or the new version, never half of it.
///
/// Readable by the user only (0600): the content comes out of a container
/// macOS protects, and should not become readable to other accounts on the
/// way out.
fn write_atomic(path: &Path, contents: &str) -> Result<(), String> {
    use std::io::Write;
    let tmp = path.with_extension("json.tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp).map_err(|e| e.to_string())?;
    f.write_all(contents.as_bytes()).map_err(|e| e.to_string())?;
    drop(f);
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Columns of one voice message as the database has them, before the
/// transcript is attached.
struct MetaRow {
    sent_unix: i64,
    chat_jid: String,
    chat_name: String,
    is_group: bool,
    from_me: bool,
    duration_s: i64,
}

/// Look up chat, time and sender for a set of media row ids in as few queries
/// as possible. Chunked because SQLite caps the number of `?` placeholders.
fn query_meta(db: &rusqlite::Connection, ids: &[i64]) -> Result<HashMap<i64, MetaRow>, String> {
    let mut out = HashMap::new();
    for chunk in ids.chunks(500) {
        let marks = vec!["?"; chunk.len()].join(",");
        let sql = format!(
            "SELECT mi.Z_PK, m.ZMESSAGEDATE, cs.ZCONTACTJID, cs.ZPARTNERNAME,
                    cs.ZSESSIONTYPE, m.ZISFROMME, mi.ZMOVIEDURATION
             FROM ZWAMEDIAITEM mi
             JOIN ZWAMESSAGE m ON mi.ZMESSAGE = m.Z_PK
             JOIN ZWACHATSESSION cs ON m.ZCHATSESSION = cs.Z_PK
             WHERE mi.Z_PK IN ({marks})"
        );
        let mut stmt = db.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(chunk.iter()), |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    MetaRow {
                        sent_unix: apple_to_unix(r.get::<_, Option<f64>>(1)?.unwrap_or(0.0)),
                        chat_jid: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        chat_name: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                        is_group: r.get::<_, Option<i64>>(4)? == Some(1),
                        from_me: r.get::<_, Option<i64>>(5)?.unwrap_or(0) == 1,
                        duration_s: r.get::<_, Option<f64>>(6)?.unwrap_or(0.0) as i64,
                    },
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (id, meta) = row.map_err(|e| e.to_string())?;
            out.insert(id, meta);
        }
    }
    Ok(out)
}

/// Bring the metadata file in line with the transcript cache: every transcript
/// that has no metadata yet gets it from ChatStorage. This is also the
/// migration for transcripts made before the file existed - they are simply
/// "missing" on the first run. Returns how many entries were added or changed.
///
/// A transcript whose message has since been deleted in WhatsApp cannot be
/// dated any more and stays out of this file (it is still in the cache).
pub fn sync_meta() -> Result<usize, String> {
    let cache = load_cache();
    let mut meta = load_meta();
    let mut changed = 0usize;

    // Text changed under an existing entry: just update the text.
    for (id, text) in &cache {
        if let Some(e) = meta.entries.get_mut(id) {
            if &e.text != text {
                e.text = text.clone();
                changed += 1;
            }
        }
    }

    let missing: Vec<i64> = cache
        .keys()
        .filter(|id| !meta.entries.contains_key(*id))
        .filter_map(|id| id.parse().ok())
        .filter(|id: &i64| *id >= 0) // negative ids are test data
        .collect();

    if !missing.is_empty() {
        let db = open_db()?;
        let rows = query_meta(&db, &missing)?;
        for (id, r) in rows {
            let key = id.to_string();
            let Some(text) = cache.get(&key) else { continue };
            meta.entries.insert(
                key,
                TranscriptMeta {
                    text: text.clone(),
                    sent_at: berlin_iso(r.sent_unix),
                    sent_unix: r.sent_unix,
                    chat_jid: r.chat_jid,
                    chat_name: r.chat_name,
                    is_group: r.is_group,
                    from_me: r.from_me,
                    duration_s: r.duration_s,
                },
            );
            changed += 1;
        }
    }

    if changed > 0 || !meta_path().is_file() {
        meta.version = 1;
        let json = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
        write_atomic(&meta_path(), &json)?;
    }
    Ok(changed)
}

// --- self chat export ------------------------------------------------------

/// Opt-in configuration for the self chat export, read from
/// `whatsapp-export.json`. Without that file nothing is exported: message text
/// of other chats is never written anywhere, and the own chat only when the
/// user has named it.
#[derive(Debug, Default, Deserialize)]
struct ExportConfig {
    /// The user's own WhatsApp id, e.g. `4915112345678@s.whatsapp.net` - the
    /// chat with oneself ("Message yourself") carries it as its chat id.
    #[serde(default)]
    self_chat_jid: String,
}

fn export_config() -> ExportConfig {
    std::fs::read_to_string(support_dir().join("whatsapp-export.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn self_chat_path() -> PathBuf {
    support_dir().join("whatsapp-selfchat.json")
}

/// One message of the self chat: a typed note or a voice message.
#[derive(Debug, Clone, Serialize)]
pub struct SelfChatEntry {
    /// The message row id.
    pub id: i64,
    /// "text" or "voice".
    pub kind: &'static str,
    pub sent_at: String,
    pub sent_unix: i64,
    /// The typed text, or the transcript of a voice message. `None` for a voice
    /// message nobody has transcribed yet, so a reader can see the gap.
    pub text: Option<String>,
    /// Voice messages only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_s: Option<i64>,
}

#[derive(Debug, Serialize)]
struct SelfChatFile {
    version: u32,
    chat_jid: String,
    exported_at: String,
    /// Oldest first.
    messages: Vec<SelfChatEntry>,
}

/// Refuse to export a chat that is not the chat with oneself.
///
/// The config file is plain user-writable JSON, so anything running as the user
/// could put another chat's id there and have this app - which holds Full Disk
/// Access - copy that conversation out of the protected container. The chat
/// with oneself is told apart by its shape: a one to one chat in which no
/// message ever came in from someone else. A real conversation fails that the
/// moment the other side has written once.
fn check_is_self_chat(db: &rusqlite::Connection, jid: &str) -> Result<(), String> {
    let (sessions, groups, incoming): (i64, i64, i64) = db
        .query_row(
            "SELECT COUNT(DISTINCT cs.Z_PK),
                    COALESCE(SUM(cs.ZSESSIONTYPE = 1), 0),
                    (SELECT COUNT(*) FROM ZWAMESSAGE m
                     JOIN ZWACHATSESSION c ON m.ZCHATSESSION = c.Z_PK
                     WHERE c.ZCONTACTJID = ?1 AND m.ZISFROMME = 0
                       AND (m.ZTEXT IS NOT NULL
                            OR EXISTS (SELECT 1 FROM ZWAMEDIAITEM mi
                                       WHERE mi.ZMESSAGE = m.Z_PK)))
             FROM ZWACHATSESSION cs WHERE cs.ZCONTACTJID = ?1",
            [jid],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;
    if sessions == 0 {
        return Err("self_chat_jid: no chat with this id".into());
    }
    if groups > 0 || incoming > 0 {
        return Err(format!(
            "self_chat_jid is not the chat with oneself ({incoming} incoming messages); not exported"
        ));
    }
    Ok(())
}

/// Write every message of the self chat, oldest first, to
/// `whatsapp-selfchat.json`. Returns the number of messages, or `None` when the
/// export is not configured.
pub fn export_self_chat() -> Result<Option<usize>, String> {
    let jid = export_config().self_chat_jid.trim().to_string();
    if jid.is_empty() {
        return Ok(None);
    }
    let db = open_db()?;
    check_is_self_chat(&db, &jid)?;
    let mut stmt = db
        .prepare(
            "SELECT m.Z_PK, m.ZMESSAGEDATE, m.ZTEXT, mi.Z_PK, mi.ZMEDIALOCALPATH,
                    mi.ZMOVIEDURATION
             FROM ZWAMESSAGE m
             JOIN ZWACHATSESSION cs ON m.ZCHATSESSION = cs.Z_PK
             LEFT JOIN ZWAMEDIAITEM mi ON mi.ZMESSAGE = m.Z_PK
             WHERE cs.ZCONTACTJID = ?1
             ORDER BY m.ZMESSAGEDATE ASC",
        )
        .map_err(|e| e.to_string())?;
    let cache = load_cache();
    let rows = stmt
        .query_map([&jid], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                apple_to_unix(r.get::<_, Option<f64>>(1)?.unwrap_or(0.0)),
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<f64>>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut messages = Vec::new();
    for row in rows {
        let (id, sent_unix, text, media_id, media_path, dur) = row.map_err(|e| e.to_string())?;
        let is_voice = media_path.as_deref().is_some_and(|p| p.ends_with(".opus"));
        let entry = if is_voice {
            SelfChatEntry {
                id,
                kind: "voice",
                sent_at: berlin_iso(sent_unix),
                sent_unix,
                // The transcript cache is keyed by the media row, not the
                // message row.
                text: media_id.and_then(|m| cache.get(&m.to_string()).cloned()),
                duration_s: Some(dur.unwrap_or(0.0) as i64),
            }
        } else {
            // Images, stickers, deleted messages and the like carry no text
            // worth exporting; a photo caption does and is kept.
            let Some(text) = text.filter(|t| !t.trim().is_empty()) else {
                continue;
            };
            SelfChatEntry {
                id,
                kind: "text",
                sent_at: berlin_iso(sent_unix),
                sent_unix,
                text: Some(text),
                duration_s: None,
            }
        };
        messages.push(entry);
    }

    let count = messages.len();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let file = SelfChatFile {
        version: 1,
        chat_jid: jid,
        exported_at: berlin_iso(now),
        messages,
    };
    let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    write_atomic(&self_chat_path(), &json)?;
    Ok(Some(count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_dates_convert_to_unix_time() {
        // 0 in Core Data is 2001-01-01, which is 978307200 in unix time.
        assert_eq!(apple_to_unix(0.0), 978_307_200);
        assert_eq!(apple_to_unix(1.0), 978_307_201);
    }

    #[test]
    fn berlin_time_follows_summer_and_winter_time() {
        // 2026-01-15 12:00 UTC: winter, +01:00.
        assert_eq!(berlin_iso(1_768_478_400), "2026-01-15T13:00:00+01:00");
        // 2026-07-01 10:00 UTC: summer, +02:00.
        assert_eq!(berlin_iso(1_782_900_000), "2026-07-01T12:00:00+02:00");
        // Summer time 2026 starts 2026-03-29 01:00 UTC.
        assert_eq!(berlin_iso(1_774_745_999), "2026-03-29T01:59:59+01:00");
        assert_eq!(berlin_iso(1_774_746_000), "2026-03-29T03:00:00+02:00");
        // And ends 2026-10-25 01:00 UTC.
        assert_eq!(berlin_iso(1_792_889_999), "2026-10-25T02:59:59+02:00");
        assert_eq!(berlin_iso(1_792_890_000), "2026-10-25T02:00:00+01:00");
        // Late evening UTC is already the next day in Berlin.
        assert_eq!(berlin_iso(1_767_225_600 - 1800), "2026-01-01T00:30:00+01:00");
    }

    #[test]
    fn civil_dates_round_trip() {
        for days in [-1000, 0, 365, 11_016, 20_746, 30_000] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
    }

    #[test]
    fn meta_entries_serialize_with_readable_keys() {
        let m = TranscriptMeta {
            text: "Haferflocken mit Sojamilch".into(),
            sent_at: berlin_iso(1_782_900_000),
            sent_unix: 1_782_900_000,
            chat_jid: "4915100000000@s.whatsapp.net".into(),
            chat_name: "Ich".into(),
            is_group: false,
            from_me: true,
            duration_s: 12,
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"sent_at\":\"2026-07-01T12:00:00+02:00\""));
        assert!(json.contains("\"chat_jid\":\"4915100000000@s.whatsapp.net\""));
        let back: TranscriptMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn access_reports_missing_when_there_is_no_container() {
        // Nothing to assert about this machine specifically, but the probe must
        // return one of the three states rather than panicking.
        let a = access();
        assert!(matches!(a, Access::Ok | Access::Denied | Access::Missing));
    }

    #[test]
    fn the_cache_round_trips_a_transcript() {
        let id = -424_242; // negative id cannot collide with a real row
        cache_put(id, "hallo welt");
        assert_eq!(cache_get(id).as_deref(), Some("hallo welt"));
        // Leave no test data behind.
        let mut c = load_cache();
        c.remove(&id.to_string());
        if let Ok(json) = serde_json::to_string(&Cache(c)) {
            let _ = std::fs::write(cache_path(), json);
        }
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    /// Runs the real queries against the real database. Ignored by default: it
    /// needs WhatsApp installed and Full Disk Access for the test runner.
    /// Prints counts and shapes only, never message content.
    #[test]
    #[ignore = "needs WhatsApp data and Full Disk Access"]
    fn the_queries_return_usable_rows() {
        if access() != Access::Ok {
            eprintln!("no access: {:?}", access());
            return;
        }
        let chats = chats().expect("chats");
        eprintln!("{} chats with voice messages", chats.len());
        assert!(!chats.is_empty(), "expected at least one chat");

        let mut with_path = 0usize;
        let mut with_sender = 0usize;
        let mut total = 0usize;
        for c in &chats {
            let msgs = messages(c.id).expect("messages");
            assert_eq!(msgs.len(), c.count, "count mismatch for a chat");
            for m in &msgs {
                total += 1;
                if m.path.is_some() {
                    with_path += 1;
                }
                if !m.sender.is_empty() {
                    with_sender += 1;
                }
                assert!(m.sent_ms > 1_000_000_000, "date not converted: {}", m.sent_ms);
            }
        }
        eprintln!("{total} messages, {with_path} resolved to a file, {with_sender} named a sender");
        assert!(with_path > 0, "not a single audio file could be located");

        // The metadata query must find every voice message it is asked about,
        // with a chat id and a plausible date.
        let ids: Vec<i64> = chats
            .iter()
            .flat_map(|c| messages(c.id).unwrap_or_default())
            .map(|m| m.id)
            .take(50)
            .collect();
        let db = open_db().expect("db");
        let meta = query_meta(&db, &ids).expect("meta query");
        assert_eq!(meta.len(), ids.len(), "metadata missing for some ids");
        let with_jid = meta.values().filter(|r| !r.chat_jid.is_empty()).count();
        eprintln!("meta for {} ids, {with_jid} with a chat id", meta.len());
        assert!(meta.values().all(|r| r.sent_unix > 1_000_000_000));

        // Only reports when the export is configured on this machine.
        match export_self_chat() {
            Ok(Some(n)) => eprintln!("self chat export: {n} messages"),
            Ok(None) => eprintln!("self chat export not configured"),
            Err(e) => panic!("self chat export failed: {e}"),
        }
    }
}
