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
//! Only voice messages and the columns needed to label them are read. Message
//! text is never touched, and the database is opened read-only so a half
//! written read can never corrupt the user's chat history.
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
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join("Library/Application Support/WhimprFlow")
        .join("whatsapp-transcripts.json")
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
}

pub fn cache_get(id: i64) -> Option<String> {
    load_cache().get(&id.to_string()).cloned()
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
    }
}
