//! Personal learning: context vocabulary (external file, Contacts, Calendar), the
//! recently-used / recently-learned term lists, and pattern suggestions. All
//! state is local and in memory, except `suggestions.json` (text pairs only).
//! Terms from here are never written to `dictionary.json`.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use whimpr_core::suggestions::{self, SuggestionStore};
use whimpr_core::{CalTerm, ContextVocab, DictionaryStore, PromptInputs};

static CTX: Mutex<ContextVocab> = Mutex::new(ContextVocab { external: Vec::new(), contacts: Vec::new(), calendar: Vec::new() });
static RECENT: Mutex<Vec<String>> = Mutex::new(Vec::new());
static LEARNED: Mutex<Vec<String>> = Mutex::new(Vec::new());
static SUGG: OnceLock<Mutex<SuggestionStore>> = OnceLock::new();
static STATUS: Mutex<(String, String)> = Mutex::new((String::new(), String::new()));

const RECENT_CAP: usize = 40;

fn support_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join("Library/Application Support/WhimprFlow")
}
fn suggestions_path() -> PathBuf {
    support_dir().join("suggestions.json")
}
fn external_path() -> PathBuf {
    support_dir().join("vocab-extern.json")
}

fn sugg() -> &'static Mutex<SuggestionStore> {
    SUGG.get_or_init(|| Mutex::new(SuggestionStore::load(&suggestions_path())))
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Unix time of the end of tomorrow in local time (so "today + tomorrow" follows
/// the user's clock).
fn end_of_tomorrow(now: i64) -> i64 {
    #[cfg(unix)]
    unsafe {
        let t: libc::time_t = now as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if !libc::localtime_r(&t, &mut tm).is_null() {
            let secs_today = tm.tm_hour as i64 * 3600 + tm.tm_min as i64 * 60 + tm.tm_sec as i64;
            return now - secs_today + 2 * 86_400;
        }
    }
    now + 48 * 3600
}

/// Rank all sources into the Whisper prompt for the next dictation. Cheap: only
/// in-memory data, no I/O.
pub fn build_prompt(store: &DictionaryStore, max_tokens: usize) -> Option<(String, usize)> {
    let ctx = CTX.lock().unwrap().clone();
    let recent = RECENT.lock().unwrap().clone();
    let learned = LEARNED.lock().unwrap().clone();
    let now = unix_now();
    store.whisper_prompt_ranked(
        &PromptInputs { vocab: &ctx, learned: &learned, recent: &recent, now, soon_until: end_of_tomorrow(now) },
        max_tokens,
    )
}

/// Remember a freshly learned / accepted term (ranks high in the next prompts).
pub fn note_learned(term: &str) {
    let mut l = LEARNED.lock().unwrap();
    l.retain(|t| !t.eq_ignore_ascii_case(term));
    l.insert(0, term.to_string());
    l.truncate(RECENT_CAP);
}

// --- After each dictation: LRU of used terms + pattern counting --------------

/// Called with the final text of a dictation. Runs on its own thread.
pub fn observe_dictation(text: &str) {
    let text = text.to_string();
    std::thread::spawn(move || {
        let dict = crate::hotkey::dictionary_entries();
        let ctx = CTX.lock().unwrap().clone();
        let mut all: Vec<&str> = dict.iter().map(|e| e.correct.as_str()).collect();
        all.extend(ctx.all_terms());

        // LRU: vocabulary terms that were actually dictated.
        let lc = text.to_lowercase();
        let used: Vec<String> = all
            .iter()
            .filter(|t| t.chars().count() >= 4 && lc.contains(&t.to_lowercase()))
            .map(|t| t.to_string())
            .collect();
        if !used.is_empty() {
            let mut r = RECENT.lock().unwrap();
            for u in used {
                r.retain(|t| !t.eq_ignore_ascii_case(&u));
                r.insert(0, u);
            }
            r.truncate(RECENT_CAP);
        }

        // Pattern suggestions.
        let index = suggestions::index_terms(all.iter().copied());
        let known: Vec<String> = dict.iter().flat_map(|e| e.mishears.iter().cloned()).collect();
        let found = suggestions::find_candidates(&text, &index, &known);
        if !found.is_empty() {
            let mut s = sugg().lock().unwrap();
            for (heard, correct) in &found {
                s.record(heard, correct);
            }
            let _ = s.save(&suggestions_path());
        }
    });
}

#[derive(Serialize, Clone)]
pub struct SuggestionDto {
    pub heard: String,
    pub correct: String,
    pub count: u32,
}

pub fn pending_suggestions() -> Vec<SuggestionDto> {
    sugg()
        .lock()
        .unwrap()
        .pending()
        .into_iter()
        .map(|c| SuggestionDto { heard: c.heard, correct: c.correct, count: c.count })
        .collect()
}

/// Accept: the misheard spelling becomes a mishear of the correct term.
pub fn accept_suggestion(heard: &str, correct: &str) {
    crate::hotkey::dictionary_add(correct.to_string(), vec![heard.to_string()]);
    note_learned(correct);
    let mut s = sugg().lock().unwrap();
    s.resolve(heard, correct);
    let _ = s.save(&suggestions_path());
}

/// Reject: never suggest this pair again.
pub fn reject_suggestion(heard: &str, correct: &str) {
    let mut s = sugg().lock().unwrap();
    s.resolve(heard, correct);
    let _ = s.save(&suggestions_path());
}

// --- Context sources ----------------------------------------------------------

#[derive(Serialize, Clone)]
pub struct ContextInfo {
    pub external: usize,
    pub contacts: usize,
    pub calendar: usize,
    pub contacts_status: String,
    pub calendar_status: String,
}

pub fn context_info() -> ContextInfo {
    let c = CTX.lock().unwrap();
    let st = STATUS.lock().unwrap();
    ContextInfo {
        external: c.external.len(),
        contacts: c.contacts.len(),
        calendar: c.calendar.len(),
        contacts_status: st.0.clone(),
        calendar_status: st.1.clone(),
    }
}

fn reload_external() {
    match whimpr_core::context::load_external(&external_path()) {
        Ok(terms) => {
            eprintln!("[whimpr] external vocabulary: {} terms", terms.len());
            CTX.lock().unwrap().external = terms;
        }
        Err(e) => {
            // Missing file is normal; a broken one must not take the app down.
            if external_path().exists() {
                eprintln!("[whimpr] vocab-extern.json ignored: {e}");
            }
            CTX.lock().unwrap().external.clear();
        }
    }
}

fn external_mtime() -> Option<SystemTime> {
    std::fs::metadata(external_path()).and_then(|m| m.modified()).ok()
}

/// Refresh contacts and calendar now (also called when the settings toggles
/// change). `ask` = may show the system permission dialog.
pub fn refresh_sources(ask: bool) {
    let s = crate::hotkey::current_settings();
    let (cs, ccount) = if s.use_contacts { native::contacts(ask) } else { ("off".to_string(), None) };
    if let Some(list) = ccount {
        eprintln!("[whimpr] contacts vocabulary: {} terms", list.len());
        CTX.lock().unwrap().contacts = list;
    } else if !s.use_contacts || cs != "granted" {
        CTX.lock().unwrap().contacts.clear();
    }
    let (ks, events) = if s.use_calendar { native::calendar(ask) } else { ("off".to_string(), None) };
    if let Some(list) = events {
        eprintln!("[whimpr] calendar vocabulary: {} terms", list.len());
        CTX.lock().unwrap().calendar = list;
    } else if !s.use_calendar || ks != "granted" {
        CTX.lock().unwrap().calendar.clear();
    }
    *STATUS.lock().unwrap() = (cs, ks);
}

/// Start the background refresher. Safe to call once at startup.
pub fn start() {
    let _ = sugg();
    reload_external();
    std::thread::spawn(|| {
        let mut ext_seen = external_mtime();
        let mut tick: u64 = 0;
        refresh_sources(true);
        loop {
            std::thread::sleep(Duration::from_secs(60));
            tick += 1;
            let m = external_mtime();
            if m != ext_seen || tick % 30 == 0 {
                ext_seen = m;
                reload_external();
            }
            // Calendar changes often; contacts rarely. One pass every 15 minutes.
            if tick % 15 == 0 {
                refresh_sources(false);
            }
        }
    });
}

pub fn add_calendar_terms(out: &mut Vec<CalTerm>, title: &str, names: &[String], start: i64) {
    let title = title.trim();
    if title.chars().count() >= 4 && title.split_whitespace().count() <= 5 {
        out.push(CalTerm { term: title.to_string(), start_unix: start });
    }
    for n in names {
        let n = n.trim();
        if n.chars().count() >= 3 && !n.contains('@') {
            out.push(CalTerm { term: n.to_string(), start_unix: start });
        }
    }
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::AnyThread;
    use objc2::runtime::{Bool, ProtocolObject};
    use objc2_contacts::*;
    use objc2_event_kit::*;
    use objc2_foundation::*;

    fn wait_for(mut f: impl FnMut() -> bool) {
        for _ in 0..60 {
            if f() {
                return;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    /// Returns (status, terms). Terms are `Some` only when access is granted.
    pub fn contacts(ask: bool) -> (String, Option<Vec<String>>) {
        unsafe {
            let mut status = CNContactStore::authorizationStatusForEntityType(CNEntityType::Contacts);
            if status == CNAuthorizationStatus::NotDetermined && ask {
                let store = CNContactStore::new();
                let blk = RcBlock::new(|_granted: Bool, _e: *mut NSError| {});
                store.requestAccessForEntityType_completionHandler(CNEntityType::Contacts, &blk);
                wait_for(|| {
                    status = CNContactStore::authorizationStatusForEntityType(CNEntityType::Contacts);
                    status != CNAuthorizationStatus::NotDetermined
                });
            }
            if status != CNAuthorizationStatus::Authorized {
                let s = if status == CNAuthorizationStatus::NotDetermined { "pending" } else { "denied" };
                return (s.to_string(), None);
            }
            let store = CNContactStore::new();
            let keys: Vec<Retained<ProtocolObject<dyn CNKeyDescriptor>>> = vec![
                ProtocolObject::from_retained(CNContactGivenNameKey.copy()),
                ProtocolObject::from_retained(CNContactFamilyNameKey.copy()),
                ProtocolObject::from_retained(CNContactOrganizationNameKey.copy()),
            ];
            let keys = NSArray::from_retained_slice(&keys);
            let req = CNContactFetchRequest::initWithKeysToFetch(CNContactFetchRequest::alloc(), &keys);
            let out = std::cell::RefCell::new(Vec::<String>::new());
            let blk = RcBlock::new(|c: std::ptr::NonNull<CNContact>, _stop: std::ptr::NonNull<Bool>| {
                let c = c.as_ref();
                let mut o = out.borrow_mut();
                for s in [c.givenName().to_string(), c.familyName().to_string(), c.organizationName().to_string()] {
                    let s = s.trim().to_string();
                    if s.chars().count() >= 3 && !s.chars().any(|ch| ch.is_ascii_digit()) {
                        o.push(s);
                    }
                }
            });
            let mut err = None;
            store.enumerateContactsWithFetchRequest_error_usingBlock(&req, Some(&mut err), &blk);
            drop(blk);
            let mut v = out.into_inner();
            v.sort();
            v.dedup();
            ("granted".to_string(), Some(v))
        }
    }

    /// Titles and attendee names of events in the next 14 days.
    pub fn calendar(ask: bool) -> (String, Option<Vec<CalTerm>>) {
        unsafe {
            let mut status = EKEventStore::authorizationStatusForEntityType(EKEntityType::Event);
            let store = EKEventStore::new();
            if status == EKAuthorizationStatus::NotDetermined && ask {
                let blk = RcBlock::new(|_granted: Bool, _e: *mut NSError| {});
                let ptr = &*blk as *const _ as *mut _;
                store.requestFullAccessToEventsWithCompletion(ptr);
                wait_for(|| {
                    status = EKEventStore::authorizationStatusForEntityType(EKEntityType::Event);
                    status != EKAuthorizationStatus::NotDetermined
                });
            }
            if status != EKAuthorizationStatus::FullAccess {
                let s = if status == EKAuthorizationStatus::NotDetermined { "pending" } else { "denied" };
                return (s.to_string(), None);
            }
            let start = NSDate::now();
            let end = NSDate::dateWithTimeIntervalSinceNow(14.0 * 86_400.0);
            let pred = store.predicateForEventsWithStartDate_endDate_calendars(&start, &end, None);
            let events = store.eventsMatchingPredicate(&pred);
            let mut out = Vec::new();
            for ev in events.iter() {
                let title = ev.title().to_string();
                let st = ev.startDate().timeIntervalSince1970() as i64;
                let mut names = Vec::new();
                if let Some(att) = ev.attendees() {
                    for a in att.iter() {
                        if let Some(n) = a.name() {
                            names.push(n.to_string());
                        }
                    }
                }
                add_calendar_terms(&mut out, &title, &names, st);
            }
            ("granted".to_string(), Some(out))
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod native {
    use super::*;
    pub fn contacts(_ask: bool) -> (String, Option<Vec<String>>) {
        ("off".to_string(), None)
    }
    pub fn calendar(_ask: bool) -> (String, Option<Vec<CalTerm>>) {
        ("off".to_string(), None)
    }
}
