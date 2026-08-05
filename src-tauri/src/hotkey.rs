//! Hold-Fn → pill wiring for the demo shell.
//!
//! This installs an in-process CoreGraphics event tap that feeds Fn key-down /
//! key-up into the real [`whimpr_core`] dictation state machine, and turns the
//! machine's actions into `whimpr://flowbar/state` events the overlay pill
//! renders. There is no audio or ASR yet, so a finalized session is simulated as
//! completing shortly after key release — enough to see the full
//! recording → transcribing → done → idle loop driven by the actual state machine.
//!
//! In the shipping product this hook lives in a separate sidecar process (so heavy
//! inference can't stall it); running it in-process is an acceptable macOS-only
//! path for this demo and the early milestones.

/// Dictionary entry shape sent to the Hub UI (auto-learned entries flagged).
#[derive(Clone, serde::Serialize)]
pub struct DictEntryDto {
    pub correct: String,
    pub mishears: Vec<String>,
    pub auto: bool,
}

/// Overlay window rect (physical pixels) + display scale, published by the
/// hover watcher so the event tap can route pill clicks without touching the
/// window from the tap thread.
#[derive(Clone, Copy)]
pub struct OverlayHit {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub scale: f64,
}

/// A saved text transform: applied to the current selection via its shortcut.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Transform {
    pub name: String,
    /// Accelerator in tauri-global-shortcut syntax, e.g. "Alt+1".
    pub shortcut: String,
    pub prompt: String,
}

pub fn default_transforms() -> Vec<Transform> {
    vec![
        Transform {
            name: "Polish".into(),
            shortcut: "Alt+1".into(),
            prompt: "Improve clarity and conciseness. Fix grammar and flow without changing \
                     the meaning or the tone."
                .into(),
        },
        Transform {
            name: "Prompt Engineer".into(),
            shortcut: "Alt+2".into(),
            prompt: "Rewrite this as a clear, well-structured prompt for an AI assistant: \
                     explicit goal, relevant context, constraints, and the desired output \
                     format."
                .into(),
        },
        Transform {
            name: "Organize".into(),
            shortcut: "Alt+3".into(),
            prompt: "Organize these unstructured thoughts into a clear, polished version \
                     without adding, removing, or changing meaning. Improve flow and \
                     readability while keeping all original ideas intact."
                .into(),
        },
    ]
}

#[cfg(target_os = "macos")]
mod imp {
    use std::os::raw::c_void;
    use std::path::PathBuf;
    use super::DictEntryDto;
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    use serde::Serialize;
    use tauri::{AppHandle, Emitter, Manager};
    use whimpr_core::state::{Action, BarState, DictationState};
    use whimpr_core::{
        AsrEngine, CleanupContext, CleanupMode, CleanupProvider, Input, PipelineEvent, StateMachine,
        TriggerToken,
    };
    use whimpr_ipc::BindingId;

    const OVERLAY_LABEL: &str = "whimpr_bar";

    // --- CoreGraphics / CoreFoundation FFI (listen-only Fn tap) -----------
    type CFMachPortRef = *mut c_void;
    type CFRunLoopSourceRef = *mut c_void;
    type CFRunLoopRef = *mut c_void;
    type CFStringRef = *const c_void;
    type CFAllocatorRef = *const c_void;
    type CGEventRef = *mut c_void;
    type CGEventTapProxy = *mut c_void;
    type CGEventTapCallBack =
        extern "C" fn(CGEventTapProxy, u32, CGEventRef, *mut c_void) -> CGEventRef;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events_of_interest: u64,
            callback: CGEventTapCallBack,
            user_info: *mut c_void,
        ) -> CFMachPortRef;
        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
        fn CGEventGetFlags(event: CGEventRef) -> u64;
        fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
        fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
        fn CGEventSourceButtonState(state_id: u32, button: u32) -> bool;
    }

    /// Left mouse button currently held? Checks both the combined-session and
    /// HID system states — synthetic input only shows up in one of them.
    fn left_button_down() -> bool {
        unsafe { CGEventSourceButtonState(0, 0) || CGEventSourceButtonState(1, 0) }
    }

    #[repr(C)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFMachPortCreateRunLoopSource(
            allocator: CFAllocatorRef,
            port: CFMachPortRef,
            order: isize,
        ) -> CFRunLoopSourceRef;
        fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
        fn CFRunLoopRun();
        static kCFRunLoopDefaultMode: CFStringRef;
    }

    const K_CG_SESSION_EVENT_TAP: u32 = 1;
    const K_CG_HEAD_INSERT: u32 = 0;
    // Default (not listen-only): the tap may swallow pill clicks so they never
    // activate the overlay window or reach the app underneath.
    const K_CG_TAP_OPTION_DEFAULT: u32 = 0;
    const K_CG_EVENT_LEFT_MOUSE_DOWN: u32 = 1;
    const K_CG_EVENT_LEFT_MOUSE_UP: u32 = 2;
    const K_CG_EVENT_MOUSE_MOVED: u32 = 5;
    const K_CG_EVENT_LEFT_MOUSE_DRAGGED: u32 = 6;
    const K_CG_EVENT_KEY_DOWN: u32 = 10;
    const K_CG_EVENT_FLAGS_CHANGED: u32 = 12;
    const KEYCODE_ESC: i64 = 53;
    // mouse-moved is included because synthetic drags (and some devices) emit
    // moved instead of dragged while the button is held; the handler only acts
    // on it when a pill drag is already in flight.
    const EVENTS_OF_INTEREST: u64 = (1 << K_CG_EVENT_FLAGS_CHANGED)
        | (1 << K_CG_EVENT_LEFT_MOUSE_DOWN)
        | (1 << K_CG_EVENT_LEFT_MOUSE_UP)
        | (1 << K_CG_EVENT_MOUSE_MOVED)
        | (1 << K_CG_EVENT_LEFT_MOUSE_DRAGGED)
        | (1 << K_CG_EVENT_KEY_DOWN);
    const FLAG_SECONDARY_FN: u64 = 0x0080_0000;
    const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;
    const KEYCODE_FN: i64 = 63;
    const K_CG_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
    const K_CG_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

    static APP: OnceLock<AppHandle> = OnceLock::new();
    static MACHINE: OnceLock<Mutex<StateMachine>> = OnceLock::new();
    static CLOCK: OnceLock<Instant> = OnceLock::new();
    static FN_IS_DOWN: AtomicBool = AtomicBool::new(false);
    static TAP_PORT: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
    /// Bundle id of the app that was frontmost at record-start = the paste target.
    /// Cleanup uses it to format for the medium (email vs. text vs. chat).
    static TARGET_APP: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    /// Overlay window rect in physical pixels + scale, refreshed by the hover
    /// watcher. The click branch of the event tap tests against this.
    static OVERLAY_HIT: OnceLock<Mutex<Option<super::OverlayHit>>> = OnceLock::new();
    /// Spoken snippets (trigger phrase -> replacement), applied after cleanup.
    static SNIPPETS: OnceLock<Mutex<whimpr_core::SnippetStore>> = OnceLock::new();
    /// Saved text transforms (shortcut -> prompt).
    static TRANSFORMS: OnceLock<Mutex<Vec<super::Transform>>> = OnceLock::new();

    pub fn set_overlay_hit(hit: super::OverlayHit) {
        *OVERLAY_HIT.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(hit);
    }

    /// In-flight pill drag: mouse-down origin, window origin, and whether the
    /// pointer moved far enough to count as a drag rather than a click.
    #[derive(Clone, Copy)]
    struct DragState {
        start_mx: f64,
        start_my: f64,
        win_x: f64,
        win_y: f64,
        scale: f64,
        moved: bool,
    }
    static DRAG: OnceLock<Mutex<Option<DragState>>> = OnceLock::new();
    /// AX text around the caret of the paste target, snapshotted at record-start.
    static WINDOW_CTX: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    /// Last pasted dictation + when — for follow-up context and the tray copy item.
    static LAST_DICTATION: OnceLock<Mutex<Option<(String, Instant)>>> = OnceLock::new();
    /// Whether WE paused the music player (so idle only resumes what we paused).
    static MUSIC_PAUSED: AtomicBool = AtomicBool::new(false);

    /// Snapshot the paste target's focused-field text off-thread (AX can take a
    /// few ms — too slow for the event-tap callback).
    fn snapshot_window_ctx() {
        if !current_settings().context_awareness {
            *WINDOW_CTX.get_or_init(|| Mutex::new(None)).lock().unwrap() = None;
            return;
        }
        std::thread::spawn(|| {
            let ctx = crate::appctx::focused_text_context(280);
            *WINDOW_CTX.get_or_init(|| Mutex::new(None)).lock().unwrap() = ctx;
        });
    }

    pub fn last_dictation() -> Option<String> {
        LAST_DICTATION
            .get()
            .and_then(|m| m.lock().unwrap().as_ref().map(|(t, _)| t.clone()))
    }

    fn music_script(cmd: &str) -> String {
        // Pause/resume whichever player is actually running + playing.
        format!(
            r#"set acted to ""
if application "Spotify" is running then
  tell application "Spotify"
    if player state is {playing} then
      {cmd}
      set acted to "spotify"
    end if
  end tell
end if
if application "Music" is running then
  tell application "Music"
    if player state is {playing} then
      {cmd}
      set acted to acted & " music"
    end if
  end tell
end if
return acted"#,
            playing = if cmd == "pause" { "playing" } else { "paused" },
            cmd = cmd
        )
    }

    fn pause_music_if_playing() {
        if !current_settings().mute_music_while_dictating {
            return;
        }
        std::thread::spawn(|| {
            let out = std::process::Command::new("osascript")
                .arg("-e")
                .arg(music_script("pause"))
                .output();
            if let Ok(o) = out {
                let acted = String::from_utf8_lossy(&o.stdout);
                if !acted.trim().is_empty() {
                    eprintln!("[whimpr] music paused ({})", acted.trim());
                    MUSIC_PAUSED.store(true, Ordering::SeqCst);
                }
            }
        });
    }

    fn resume_music_if_we_paused() {
        if !MUSIC_PAUSED.swap(false, Ordering::SeqCst) {
            return;
        }
        std::thread::spawn(|| {
            let _ = std::process::Command::new("osascript")
                .arg("-e")
                .arg(music_script("play"))
                .output();
            eprintln!("[whimpr] music resumed");
        });
    }
    static CAPTURE: OnceLock<Mutex<Option<whimpr_audio::CaptureHandle>>> = OnceLock::new();
    static ASR: OnceLock<Arc<whimpr_asr::WhisperEngine>> = OnceLock::new();
    static OPENAI: OnceLock<Mutex<Option<whimpr_cleanup::OpenAiProvider>>> = OnceLock::new();
    static ANTHROPIC: OnceLock<Mutex<Option<whimpr_cleanup::AnthropicProvider>>> = OnceLock::new();
    static LOCAL: OnceLock<Mutex<Option<crate::local_llm::LocalWorker>>> = OnceLock::new();
    static SETTINGS: OnceLock<Mutex<whimpr_core::Settings>> = OnceLock::new();
    static DICTIONARY: OnceLock<Mutex<whimpr_core::DictionaryStore>> = OnceLock::new();
    static STATS: OnceLock<Mutex<whimpr_core::StatsStore>> = OnceLock::new();

    #[derive(Clone, Serialize)]
    struct BarPayload {
        state: &'static str,
        vertical: bool,
    }

    #[derive(Clone, Serialize)]
    struct WavePayload {
        bars: Vec<f32>,
    }

    #[derive(Clone, Serialize)]
    struct TranscriptPayload {
        text: String,
    }

    /// The whisper ASR model to load: prefer the most accurate one present, in
    /// descending quality order, falling back to the small base model. Bigger
    /// English models mis-hear names/technical terms far less (and better ASR means
    /// less for cleanup and the dictionary to fix downstream).
    fn model_path() -> PathBuf {
        let dir = support_dir().join("models");
        for name in [
            "ggml-large-v3-turbo-q5_0.bin",
            "ggml-large-v3-turbo.bin",
            "ggml-medium.en.bin",
            "ggml-small.en.bin",
            "ggml-base.en.bin",
        ] {
            let p = dir.join(name);
            if p.exists() {
                return p;
            }
        }
        dir.join("ggml-base.en.bin")
    }

    fn support_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join("Library/Application Support/WhimprFlow")
    }
    fn settings_path() -> PathBuf {
        support_dir().join("settings.json")
    }
    fn dict_path() -> PathBuf {
        support_dir().join("dictionary.json")
    }
    fn stats_path() -> PathBuf {
        support_dir().join("stats.json")
    }
    fn snippets_path() -> PathBuf {
        support_dir().join("snippets.json")
    }
    fn scratchpad_path() -> PathBuf {
        support_dir().join("scratchpad.md")
    }
    fn transforms_path() -> PathBuf {
        support_dir().join("transforms.json")
    }

    /// Seconds since the Unix epoch (UTC), or 0 if the clock is before the epoch.
    fn unix_now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Log one completed dictation to the stats store (words, speaking time, text,
    /// target app) and persist it. Powers both the Hub stats and the history list.
    pub fn record_dictation(text: &str, duration_secs: f32) {
        let words = whimpr_core::stats::count_words(text);
        if words == 0 {
            return;
        }
        let app = TARGET_APP.get().and_then(|m| m.lock().unwrap().clone());
        if let Some(m) = STATS.get() {
            let mut store = m.lock().unwrap();
            let duration_ms = (duration_secs.max(0.0) * 1000.0) as u32;
            let chars = text.chars().count() as u32;
            store.record(words, duration_ms, chars, unix_now(), text.to_string(), app);
            let _ = store.save(&stats_path());
        }
    }

    /// The most recent dictations for the Hub Home history list.
    pub fn history(limit: usize) -> Vec<whimpr_core::HistoryItem> {
        STATS
            .get()
            .map(|m| m.lock().unwrap().history(limit))
            .unwrap_or_default()
    }

    /// The dictionary entries for the Hub Dictionary screen (auto-learned flagged).
    pub fn dictionary_entries() -> Vec<DictEntryDto> {
        DICTIONARY
            .get()
            .map(|m| {
                m.lock()
                    .unwrap()
                    .entries
                    .iter()
                    .map(|e| DictEntryDto {
                        correct: e.correct.clone(),
                        mishears: e.mishears.clone(),
                        auto: matches!(e.source, whimpr_core::DictSource::Auto),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Add a manual dictionary entry and persist.
    pub fn dictionary_add(correct: String, mishears: Vec<String>) {
        if let Some(m) = DICTIONARY.get() {
            let mut store = m.lock().unwrap();
            store.add(correct, mishears, whimpr_core::DictSource::Manual);
            let _ = store.save(&dict_path());
        }
    }

    /// Remove a dictionary entry by spelling and persist.
    pub fn dictionary_remove(correct: &str) {
        if let Some(m) = DICTIONARY.get() {
            let mut store = m.lock().unwrap();
            if store.remove(correct) {
                let _ = store.save(&dict_path());
            }
        }
    }

    /// Add an AUTO-learned entry (from the post-paste correction observer) and persist.
    /// Marked ✨ auto in the UI. No-op if it would duplicate an existing entry's data.
    pub fn dictionary_learn(correct: String, mishears: Vec<String>) {
        if let Some(m) = DICTIONARY.get() {
            let mut store = m.lock().unwrap();
            store.add(correct, mishears, whimpr_core::DictSource::Auto);
            let _ = store.save(&dict_path());
        }
    }

    /// Aggregated stats for the Hub. `tz_offset_minutes` is the UI's
    /// `Date.getTimezoneOffset()` so day math matches the user's local clock.
    pub fn stats_summary(tz_offset_minutes: i32) -> whimpr_core::StatsSummary {
        STATS
            .get()
            .map(|m| m.lock().unwrap().summary(tz_offset_minutes, unix_now()))
            .unwrap_or_else(|| {
                whimpr_core::StatsStore::default().summary(tz_offset_minutes, unix_now())
            })
    }

    /// Read an API key from an env var or the OS keychain (never a plaintext file).
    fn read_key(account: &str, env_var: &str) -> Option<String> {
        if let Ok(k) = std::env::var(env_var) {
            let k = k.trim().to_string();
            if !k.is_empty() {
                return Some(k);
            }
        }
        keyring::Entry::new("com.whimpr.whimprflow", account)
            .ok()
            .and_then(|e| e.get_password().ok())
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
    }
    fn read_openai_key() -> Option<String> {
        read_key("openai_api_key", "OPENAI_API_KEY")
    }
    fn read_anthropic_key() -> Option<String> {
        read_key("anthropic_api_key", "ANTHROPIC_API_KEY")
    }

    /// A snapshot of the current settings.
    pub fn current_settings() -> whimpr_core::Settings {
        SETTINGS
            .get()
            .map(|m| m.lock().unwrap().clone())
            .unwrap_or_default()
    }
    /// Apply new settings and rebuild the cloud providers (picks up model changes).
    pub fn update_settings(new: whimpr_core::Settings) {
        if let Some(m) = SETTINGS.get() {
            *m.lock().unwrap() = new.clone();
        }
        let _ = new.save(&settings_path());
        rebuild_providers();
    }

    /// (Re)build the cloud cleanup providers from the current keys + settings. Called
    /// at startup and whenever a key or model changes, so edits take effect live.
    pub fn rebuild_providers() {
        let settings = current_settings();
        let openai = read_openai_key().map(|k| {
            whimpr_cleanup::OpenAiProvider::with_base_url(
                k,
                settings.openai_model.clone(),
                Some(settings.openai_base_url.clone()),
            )
        });
        let anthropic = read_anthropic_key()
            .map(|k| whimpr_cleanup::AnthropicProvider::new(k, settings.anthropic_model.clone()));
        eprintln!(
            "[whimpr] cleanup providers: openai={}, anthropic={}",
            openai.is_some(),
            anthropic.is_some()
        );
        match OPENAI.get() {
            Some(m) => *m.lock().unwrap() = openai,
            None => {
                let _ = OPENAI.set(Mutex::new(openai));
            }
        }
        match ANTHROPIC.get() {
            Some(m) => *m.lock().unwrap() = anthropic,
            None => {
                let _ = ANTHROPIC.set(Mutex::new(anthropic));
            }
        }
    }

    /// Clean a raw transcript per the current settings (mode + level), feeding in the
    /// dictionary vocabulary relevant to this utterance. Falls back to raw whenever
    /// cleanup is off, the provider is unavailable, it errors, or the gates reject it.
    fn clean_transcript(raw: &str) -> String {
        let cleaned = clean_transcript_inner(raw);
        // Spoken snippets expand after cleanup so triggers survive the LLM pass.
        SNIPPETS
            .get()
            .map(|s| s.lock().unwrap().apply(&cleaned))
            .unwrap_or(cleaned)
    }

    /// Map the paste-target bundle id to a style category and its user-chosen level.
    fn style_for_app(settings: &whimpr_core::Settings, bundle: Option<&str>) -> &'static str {
        use whimpr_core::StyleLevel;
        let b = bundle.unwrap_or("").to_lowercase();
        let level: StyleLevel = if b.contains("whatsapp")
            || b.contains("telegram")
            || b.contains("mobilesms")
            || b.contains("discord")
            || b.contains("signal")
        {
            settings.style.personal
        } else if b.contains("slack") || b.contains("teams") {
            settings.style.work
        } else if b.contains("mail") || b.contains("outlook") || b.contains("superhuman") {
            settings.style.email
        } else {
            settings.style.other
        };
        level.modifier()
    }

    fn clean_transcript_inner(raw: &str) -> String {
        let settings = current_settings();
        let level = settings.cleanup_level;
        if matches!(settings.cleanup_mode, CleanupMode::Raw) || level.bypasses_llm() {
            return raw.to_string();
        }
        // Turn explicit spoken layout cues ("new line", "new paragraph") into break
        // markers up front — the model passes an opaque marker through reliably but
        // mangles the literal cue words. The model sees `raw` (with markers); the gate
        // and any raw fallback use `raw_out` (markers restored to real breaks) so we
        // never paste a "[[NL]]" token or lose an explicit break.
        let raw_norm = whimpr_core::cleanup::pre_normalize_layout(raw);
        let raw = raw_norm.as_str();
        let raw_out = whimpr_core::cleanup::post_process(&raw_norm);
        let vocab = DICTIONARY
            .get()
            .map(|d| d.lock().unwrap().prefilter(raw, 15))
            .unwrap_or_default();
        let app_bundle_id = TARGET_APP.get().and_then(|m| m.lock().unwrap().clone());
        if let Some(app) = app_bundle_id.as_deref() {
            eprintln!("[whimpr] cleanup target app: {app}");
        }
        let style = style_for_app(&settings, app_bundle_id.as_deref());
        // Context awareness: text around the caret (AX snapshot at record-start)
        // plus the previous dictation when it was moments ago (follow-ups).
        let mut window_context = WINDOW_CTX
            .get()
            .and_then(|m| m.lock().unwrap().clone())
            .filter(|_| settings.context_awareness);
        if settings.context_awareness {
            if let Some((prev, at)) = LAST_DICTATION.get().and_then(|m| m.lock().unwrap().clone()) {
                if at.elapsed() < Duration::from_secs(90) {
                    let tail: String = prev.chars().rev().take(200).collect::<Vec<_>>().into_iter().rev().collect();
                    let joined = match window_context.take() {
                        Some(w) => format!("{w}\n[Previous dictation, moments ago:] {tail}"),
                        None => format!("[Previous dictation, moments ago:] {tail}"),
                    };
                    window_context = Some(joined);
                }
            }
        }
        let ctx = CleanupContext {
            level,
            vocab,
            app_bundle_id,
            style: Some(style),
            window_context,
            ..Default::default()
        };
        // Run the on-device model with the same prompt + per-app formatting.
        let run_local = || -> Option<anyhow::Result<String>> {
            LOCAL.get().and_then(|m| {
                m.lock().unwrap().as_mut().map(|w| {
                    // System prompt + few-shot demonstration turns + the transcript,
                    // so the on-device model actually produces newlines/lists and
                    // resolves self-corrections instead of just being told to.
                    let messages = whimpr_core::cleanup::build_messages(raw, &ctx);
                    w.cleanup(&messages)
                })
            })
        };
        // Selected provider, falling back to local when a cloud key can't be read
        // (so cleanup still runs) — and Local mode uses the worker directly.
        let result: Option<anyhow::Result<String>> = match settings.cleanup_mode {
            CleanupMode::OpenAi => OPENAI
                .get()
                .and_then(|m| m.lock().unwrap().as_ref().map(|p| p.cleanup(raw, &ctx)))
                .or_else(run_local),
            CleanupMode::Anthropic => ANTHROPIC
                .get()
                .and_then(|m| m.lock().unwrap().as_ref().map(|p| p.cleanup(raw, &ctx)))
                .or_else(run_local),
            CleanupMode::Local => run_local(),
            CleanupMode::Raw => None,
        };
        match result {
            Some(Ok(cleaned)) => {
                // Deterministic safety net: convert any leftover spoken layout cue the
                // model missed into real line breaks, strip stray code fences, cap blank
                // lines. Guarantees no "new line"/"new paragraph" word reaches the cursor.
                let cleaned = whimpr_core::cleanup::post_process(&cleaned);
                if whimpr_core::cleanup::evaluate_gates(&raw_out, &cleaned, level).passed() {
                    cleaned
                } else {
                    eprintln!("[whimpr] cleanup gate rejected the edit — pasting raw");
                    raw_out
                }
            }
            Some(Err(e)) => {
                eprintln!("[whimpr] cleanup failed ({e}) — pasting raw");
                raw_out
            }
            None => {
                if matches!(settings.cleanup_mode, CleanupMode::Local) {
                    eprintln!("[whimpr] local cleanup model not wired yet — pasting raw");
                } else {
                    eprintln!("[whimpr] cleanup provider has no API key — pasting raw");
                }
                raw_out
            }
        }
    }

    /// Whisper non-speech annotations: "*music*", "[Musik]", "(applause)", "♪ …".
    /// Never legitimate dictation output — always dropped, regardless of level.
    fn is_noise_annotation(text: &str) -> bool {
        let t = text.trim();
        if t.is_empty() {
            return true;
        }
        (t.starts_with('*') && t.ends_with('*'))
            || (t.starts_with('[') && t.ends_with(']'))
            || (t.starts_with('(') && t.ends_with(')'))
            || t.chars().all(|c| matches!(c, '♪' | '♫' | ' ' | '.' | '-'))
    }

    /// Classic Whisper silence hallucinations (German + English).
    fn is_silence_hallucination(text: &str) -> bool {
        let t = text.trim().trim_end_matches(['.', '!']).to_lowercase();
        matches!(
            t.as_str(),
            "" | "vielen dank"
                | "danke"
                | "danke schön"
                | "dankeschön"
                | "thank you"
                | "thanks for watching"
                | "bis zum nächsten mal"
                | "untertitelung des zdf für funk, 2017"
                | "untertitel im auftrag des zdf für funk, 2017"
                | "untertitel der amara.org-community"
                | "copyright wdr 2021"
                | "das war's"
        )
    }

    /// Fire-and-forget system sound (subtle Wispr-style audio feedback).
    fn play_sound(name: &'static str) {
        if !current_settings().sound_on_start {
            return;
        }
        std::thread::spawn(move || {
            let _ = std::process::Command::new("afplay")
                .arg("-v")
                .arg("0.35")
                .arg(format!("/System/Library/Sounds/{name}.aiff"))
                .status();
        });
    }

    fn now_ms() -> u64 {
        CLOCK.get().map(|c| c.elapsed().as_millis() as u64).unwrap_or(0)
    }

    fn bar_name(b: BarState) -> &'static str {
        match b {
            BarState::Idle => "idle",
            BarState::Recording => "recording",
            BarState::Locked => "locked",
            BarState::Transcribing => "transcribing",
            BarState::Done => "done",
            BarState::Cancelled => "cancelled",
            BarState::Error => "error",
        }
    }

    fn emit_bar(app: &AppHandle, state: &'static str) {
        eprintln!("[whimpr] pill -> {state}");
        match state {
            "recording" | "locked" => {
                play_sound("Tink");
                pause_music_if_playing();
            }
            "done" => play_sound("Pop"),
            "idle" | "cancelled" | "error" => resume_music_if_we_paused(),
            _ => {}
        }
        let vertical = crate::overlay_vertical();
        let _ = app.emit_to(
            OVERLAY_LABEL,
            "whimpr://flowbar/state",
            BarPayload { state, vertical },
        );
        // Resize the overlay window to hug the pill per state, so its hitbox is
        // only ever the pill itself (idle stays hoverable for the mic button).
        if let Some(w) = app.get_webview_window(OVERLAY_LABEL) {
            let (lw, lh) = match (state, vertical) {
                ("recording" | "locked", false) => (240.0, 56.0),
                ("recording" | "locked", true) => (56.0, 240.0),
                ("idle", false) => (76.0, 44.0),
                ("idle", true) => (44.0, 76.0),
                (_, false) => (200.0, 52.0),
                (_, true) => (52.0, 120.0),
            };
            let _ = w.set_size(tauri::LogicalSize::new(lw, lh));
            crate::position_overlay(&w);
        }
    }

    /// Feed one input into the shared state machine and enact its actions.
    fn handle_input(input: Input) {
        let (Some(app), Some(machine)) = (APP.get(), MACHINE.get()) else {
            return;
        };
        let actions = {
            let mut m = machine.lock().unwrap();
            m.step(input)
        };
        for action in actions {
            apply_action(app, action);
        }
    }

    fn apply_action(app: &AppHandle, action: Action) {
        match action {
            Action::ShowBar(bar) => {
                emit_bar(app, bar_name(bar));
                // Let the "done" tick linger briefly before returning to idle.
                if bar == BarState::Done {
                    let app2 = app.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(500));
                        emit_bar(&app2, "idle");
                    });
                }
            }
            // Start the microphone; stream real RMS bars to the pill waveform.
            // Runs off the tap thread so the mic-permission prompt can't stall keys.
            Action::StartCapture { .. } => {
                let app_thread = app.clone();
                std::thread::spawn(move || {
                    let app_cb = app_thread.clone();
                    match whimpr_audio::start(move |bars| {
                        let _ = app_cb.emit_to(
                            OVERLAY_LABEL,
                            "whimpr://audio/waveform",
                            WavePayload { bars: bars.to_vec() },
                        );
                    }) {
                        Ok(handle) => {
                            *CAPTURE.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(handle);
                        }
                        Err(e) => eprintln!("[whimpr] mic capture failed to start: {e}"),
                    }
                });
            }
            // Stop the mic, transcribe the buffered audio, and advance the machine.
            Action::StopCaptureAndFinalize { session } => {
                let app2 = app.clone();
                let handle = CAPTURE.get().and_then(|slot| slot.lock().unwrap().take());
                std::thread::spawn(move || {
                    // Whatever happens, return the pill to idle (done -> idle).
                    let finish =
                        || handle_input(Input::Pipeline(PipelineEvent::Committed { session }));
                    let Some(res) = handle.and_then(|h| h.stop()) else {
                        eprintln!("[whimpr] no audio captured");
                        finish();
                        return;
                    };
                    let peak = res.samples.iter().fold(0f32, |m, &s| m.max(s.abs()));
                    eprintln!(
                        "[whimpr] captured {} samples @ {} Hz (~{:.2}s), peak {:.4}",
                        res.samples.len(),
                        res.sample_rate,
                        res.duration_secs(),
                        peak
                    );
                    if peak < 0.005 {
                        eprintln!(
                            "[whimpr] ⚠ audio is silent — the mic isn't being captured. Grant \
                             Microphone access to your terminal (System Settings → Privacy & \
                             Security → Microphone), then fully quit + reopen it and rerun."
                        );
                    }
                    let Some(asr) = ASR.get().cloned() else {
                        eprintln!("[whimpr] ASR not ready (model still loading or missing)");
                        finish();
                        return;
                    };
                    let pcm = whimpr_audio::resample_to_16k(&res.samples, res.sample_rate);
                    match asr.transcribe(&pcm) {
                        Ok(t) => {
                            let mut raw = t.text;
                            eprintln!("[whimpr] TRANSCRIPT: \"{}\"", raw);
                            // Whisper hallucinates stock phrases on (near-)silence
                            // ("Vielen Dank.", subtitle credits, …). Only filter when
                            // the audio was actually quiet, so genuinely dictated
                            // thanks still go through.
                            if is_noise_annotation(&raw)
                                || (peak < 0.012 && is_silence_hallucination(&raw))
                            {
                                eprintln!("[whimpr] non-speech transcript dropped");
                                raw = String::new();
                            }
                            // Clean the transcript (cloud LLM if configured), then paste.
                            let mut text = clean_transcript(&raw);
                            if text != raw {
                                eprintln!("[whimpr] CLEANED:   \"{}\"", text);
                            }
                            // Smart spacing: pasting right behind existing text
                            // (caret at a word/punctuation) gets a leading space.
                            if let Some(ctx) = WINDOW_CTX.get().and_then(|m| m.lock().unwrap().clone()) {
                                let needs_space = ctx
                                    .chars()
                                    .last()
                                    .map(|c| !c.is_whitespace())
                                    .unwrap_or(false)
                                    && text.chars().next().map(|c| c.is_alphanumeric()).unwrap_or(false);
                                if needs_space {
                                    text.insert(0, ' ');
                                }
                            }
                            if !text.is_empty() {
                                if let Err(e) = crate::paste::paste_text(&text) {
                                    eprintln!("[whimpr] paste failed: {e}");
                                }
                                *LAST_DICTATION
                                    .get_or_init(|| Mutex::new(None))
                                    .lock()
                                    .unwrap() = Some((text.clone(), Instant::now()));
                                // Log words + speaking time for the Hub stats (WPM, streak…).
                                record_dictation(&text, res.duration_secs());
                                // Watch the field for a post-paste correction to learn (✨).
                                crate::autolearn::watch_correction(&text);
                            }
                            let _ = app2.emit_to(
                                OVERLAY_LABEL,
                                "whimpr://transcript",
                                TranscriptPayload { text },
                            );
                        }
                        Err(e) => eprintln!("[whimpr] ASR error: {e}"),
                    }
                    finish();
                });
            }
            Action::DiscardCapture { .. } => {
                if let Some(slot) = CAPTURE.get() {
                    if let Some(handle) = slot.lock().unwrap().take() {
                        let _ = handle.stop();
                    }
                }
            }
            // The ASR path (StopCaptureAndFinalize) now drives pipeline completion.
            Action::RunPipeline { .. } => {}
            // PlayPing / WarnSessionCap: no-ops for now.
            _ => {}
        }
    }

    extern "C" fn tap_callback(
        _proxy: CGEventTapProxy,
        etype: u32,
        event: CGEventRef,
        _info: *mut c_void,
    ) -> CGEventRef {
        if etype == K_CG_TAP_DISABLED_BY_TIMEOUT || etype == K_CG_TAP_DISABLED_BY_USER_INPUT {
            let port = TAP_PORT.load(Ordering::SeqCst);
            if !port.is_null() {
                unsafe { CGEventTapEnable(port, true) };
            }
            return event;
        }
        if etype == K_CG_EVENT_KEY_DOWN {
            // Esc cancels a running dictation (and is swallowed); everything else
            // passes through untouched.
            let keycode =
                unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) };
            if keycode == KEYCODE_ESC {
                let recording = MACHINE
                    .get()
                    .map(|m| matches!(m.lock().unwrap().state(), DictationState::Recording { .. }))
                    .unwrap_or(false);
                if recording {
                    eprintln!("[whimpr] Esc -> cancel");
                    handle_input(Input::Trigger(TriggerToken::Cancel { at_ms: now_ms() }));
                    return null_mut();
                }
            }
            return event;
        }
        if etype == K_CG_EVENT_LEFT_MOUSE_DOWN {
            let hit = OVERLAY_HIT.get().and_then(|m| *m.lock().unwrap());
            if let Some(r) = hit {
                let loc = unsafe { CGEventGetLocation(event) };
                let (mx, my) = (loc.x * r.scale, loc.y * r.scale);
                if mx >= r.x && mx <= r.x + r.w && my >= r.y && my <= r.y + r.h {
                    // Don't act yet — wait for mouse-up so a drag can move the
                    // pill instead of clicking it. Swallow the event either way.
                    *DRAG.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(DragState {
                        start_mx: mx,
                        start_my: my,
                        win_x: r.x,
                        win_y: r.y,
                        scale: r.scale,
                        moved: false,
                    });
                    return null_mut();
                }
            }
            return event;
        }
        if etype == K_CG_EVENT_LEFT_MOUSE_DRAGGED || etype == K_CG_EVENT_MOUSE_MOVED {
            let drag = DRAG.get().and_then(|m| *m.lock().unwrap());
            if drag.is_none() {
                return event;
            }
            // Stuck-drag guard: if the button is no longer down (missed mouse-up,
            // e.g. from synthetic input), finish the drag now.
            if etype == K_CG_EVENT_MOUSE_MOVED && !left_button_down() {
                *DRAG.get().unwrap().lock().unwrap() = None;
                if let Some(app) = APP.get() {
                    crate::overlay_drag_end(app);
                }
                return event;
            }
            if let Some(mut d) = drag {
                let loc = unsafe { CGEventGetLocation(event) };
                let (mx, my) = (loc.x * d.scale, loc.y * d.scale);
                let (dx, dy) = (mx - d.start_mx, my - d.start_my);
                if !d.moved && (dx * dx + dy * dy).sqrt() < 5.0 * d.scale {
                    return null_mut();
                }
                d.moved = true;
                *DRAG.get().unwrap().lock().unwrap() = Some(d);
                if let Some(app) = APP.get() {
                    crate::overlay_drag_move(app, d.win_x + dx, d.win_y + dy);
                }
                return null_mut();
            }
            return event;
        }
        if etype == K_CG_EVENT_LEFT_MOUSE_UP {
            let drag = DRAG.get().and_then(|m| *m.lock().unwrap());
            if let Some(d) = drag {
                *DRAG.get().unwrap().lock().unwrap() = None;
                if d.moved {
                    // Drag finished: persist the new anchor, no click action.
                    if let Some(app) = APP.get() {
                        crate::overlay_drag_end(app);
                    }
                    return null_mut();
                }
                // Plain click: act based on the current state.
                let loc = unsafe { CGEventGetLocation(event) };
                let mx = loc.x * d.scale;
                let r_w = OVERLAY_HIT
                    .get()
                    .and_then(|m| *m.lock().unwrap())
                    .map(|r| (r.x, r.w))
                    .unwrap_or((d.win_x, 1.0));
                let state = MACHINE.get().map(|m| m.lock().unwrap().state());
                match state {
                    Some(DictationState::Idle) => {
                        eprintln!("[whimpr] pill click -> start");
                        let target = crate::appctx::frontmost_bundle_id();
                        *TARGET_APP.get_or_init(|| Mutex::new(None)).lock().unwrap() = target; snapshot_window_ctx();
                        handle_input(Input::Trigger(TriggerToken::Down {
                            binding: BindingId::HandsFree,
                            at_ms: now_ms(),
                        }));
                    }
                    Some(DictationState::Recording { .. }) => {
                        // Left third of the bar cancels; the rest stops+pastes.
                        let frac = (mx - r_w.0) / r_w.1;
                        if frac < 0.33 {
                            eprintln!("[whimpr] pill click -> cancel");
                            handle_input(Input::Trigger(TriggerToken::Cancel {
                                at_ms: now_ms(),
                            }));
                        } else {
                            eprintln!("[whimpr] pill click -> stop");
                            handle_input(Input::Trigger(TriggerToken::Down {
                                binding: BindingId::PushToTalk,
                                at_ms: now_ms(),
                            }));
                        }
                    }
                    _ => {}
                }
                return null_mut();
            }
            return event;
        }
        if etype == K_CG_EVENT_FLAGS_CHANGED {
            let keycode =
                unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) };
            if keycode == KEYCODE_FN {
                let flags = unsafe { CGEventGetFlags(event) };
                let down = (flags & FLAG_SECONDARY_FN) != 0;
                let was_down = FN_IS_DOWN.swap(down, Ordering::SeqCst);
                let at_ms = now_ms();
                if down && !was_down {
                    eprintln!("[whimpr] Fn DOWN");
                    // Snapshot the paste target now, while the user's app is focused.
                    let target = crate::appctx::frontmost_bundle_id();
                    *TARGET_APP.get_or_init(|| Mutex::new(None)).lock().unwrap() = target; snapshot_window_ctx();
                    handle_input(Input::Trigger(TriggerToken::Down {
                        binding: BindingId::PushToTalk,
                        at_ms,
                    }));
                } else if !down && was_down {
                    eprintln!("[whimpr] Fn UP");
                    handle_input(Input::Trigger(TriggerToken::Up {
                        binding: BindingId::PushToTalk,
                        at_ms,
                    }));
                }
            }
        }
        event
    }

    // --- Snippets / Scratchpad / Transforms API (Hub + shortcuts) ---------

    pub fn snippets_entries() -> Vec<whimpr_core::Snippet> {
        SNIPPETS
            .get()
            .map(|s| s.lock().unwrap().entries.clone())
            .unwrap_or_default()
    }

    pub fn snippets_add(trigger: String, replacement: String) {
        if let Some(s) = SNIPPETS.get() {
            let mut s = s.lock().unwrap();
            s.add(trigger, replacement);
            let _ = s.save(&snippets_path());
        }
    }

    pub fn snippets_remove(trigger: &str) {
        if let Some(s) = SNIPPETS.get() {
            let mut s = s.lock().unwrap();
            s.remove(trigger);
            let _ = s.save(&snippets_path());
        }
    }

    pub fn scratchpad_get() -> String {
        std::fs::read_to_string(scratchpad_path()).unwrap_or_default()
    }

    pub fn scratchpad_set(text: &str) {
        let _ = std::fs::create_dir_all(support_dir());
        let _ = std::fs::write(scratchpad_path(), text);
    }

    pub fn transforms_list() -> Vec<super::Transform> {
        TRANSFORMS
            .get()
            .map(|t| t.lock().unwrap().clone())
            .unwrap_or_else(super::default_transforms)
    }

    pub fn transforms_save(list: Vec<super::Transform>) {
        if let Some(t) = TRANSFORMS.get() {
            *t.lock().unwrap() = list.clone();
        }
        let _ = std::fs::write(
            transforms_path(),
            serde_json::to_string_pretty(&list).unwrap_or_default(),
        );
    }

    /// Run a transform on the current selection: copy it, rewrite it with the
    /// local LLM, paste the result over the selection. Driven by the global
    /// shortcuts registered in the Tauri shell.
    pub fn run_transform(shortcut: &str) {
        let Some(t) = transforms_list()
            .into_iter()
            .find(|t| t.shortcut.eq_ignore_ascii_case(shortcut))
        else {
            return;
        };
        let Some(app) = APP.get().cloned() else { return };
        std::thread::spawn(move || {
            let sel = match crate::paste::copy_selection() {
                Ok(Some(s)) => s,
                Ok(None) => {
                    eprintln!("[whimpr] transform '{}': no selection", t.name);
                    emit_bar(&app, "error");
                    reset_bar_soon(&app);
                    return;
                }
                Err(e) => {
                    eprintln!("[whimpr] transform copy failed: {e}");
                    return;
                }
            };
            eprintln!("[whimpr] transform '{}' on {} chars", t.name, sel.len());
            emit_bar(&app, "transcribing");
            let system = format!(
                "You are a text transformation engine. Apply the instruction to the text you \
                 receive. Return ONLY the transformed text — no preamble, no explanations, no \
                 quotes, no markdown fences. Keep the text's original language.\n\
                 Instruction: {}",
                t.prompt
            );
            let msgs = vec![
                whimpr_core::cleanup::CleanupMsg { role: "system", content: system },
                whimpr_core::cleanup::CleanupMsg { role: "user", content: sel },
            ];
            let result = LOCAL
                .get()
                .and_then(|m| m.lock().unwrap().as_mut().map(|w| w.cleanup(&msgs)));
            match result {
                Some(Ok(out)) if !out.trim().is_empty() => {
                    if let Err(e) = crate::paste::paste_text(out.trim()) {
                        eprintln!("[whimpr] transform paste failed: {e}");
                    }
                    emit_bar(&app, "done");
                }
                other => {
                    eprintln!("[whimpr] transform failed: {other:?}");
                    emit_bar(&app, "error");
                }
            }
            reset_bar_soon(&app);
        });
    }

    fn reset_bar_soon(app: &AppHandle) {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1400));
            emit_bar(&app, "idle");
        });
    }

    /// Start a hands-free (locked) dictation, as if the user tapped Fn — used by
    /// the pill's on-hover mic button. Snapshots the paste target first, exactly
    /// like the Fn-down path.
    pub fn trigger_hands_free() {
        let target = crate::appctx::frontmost_bundle_id();
        *TARGET_APP.get_or_init(|| Mutex::new(None)).lock().unwrap() = target; snapshot_window_ctx();
        handle_input(Input::Trigger(TriggerToken::Down {
            binding: BindingId::HandsFree,
            at_ms: now_ms(),
        }));
    }

    /// Finalize the running session (pill stop button).
    pub fn trigger_stop() {
        handle_input(Input::Trigger(TriggerToken::Down {
            binding: BindingId::PushToTalk,
            at_ms: now_ms(),
        }));
    }

    /// Cancel and discard the running session (pill cancel button).
    pub fn trigger_cancel() {
        handle_input(Input::Trigger(TriggerToken::Cancel { at_ms: now_ms() }));
    }

    pub fn install(app: AppHandle) {
        let _ = APP.set(app);
        let _ = MACHINE.set(Mutex::new(StateMachine::new()));
        let _ = CLOCK.set(Instant::now());

        // Load the speech-to-text model off the main thread (it takes ~1s).
        std::thread::spawn(|| {
            let path = model_path();
            if !path.exists() {
                eprintln!("[whimpr] ASR model not found at {}", path.display());
                return;
            }
            match whimpr_asr::WhisperEngine::load(&path) {
                Ok(engine) => {
                    // Transcribe a second of silence now so Metal compiles its
                    // pipelines at startup, not on the user's first dictation.
                    let t = Instant::now();
                    let _ = engine.transcribe(&vec![0.0f32; 16_000]);
                    eprintln!("[whimpr] ASR warmed up in {:?}", t.elapsed());
                    let _ = ASR.set(Arc::new(engine));
                    eprintln!("[whimpr] ASR model loaded — ready to transcribe");
                }
                Err(e) => eprintln!("[whimpr] ASR model load failed: {e}"),
            }
        });

        // Load settings + dictionary, and build cloud providers from stored keys.
        let settings = whimpr_core::Settings::load(&settings_path());
        let dict = whimpr_core::DictionaryStore::load(&dict_path());
        eprintln!(
            "[whimpr] cleanup mode: {:?}, level: {:?}",
            settings.cleanup_mode, settings.cleanup_level
        );
        let _ = SETTINGS.set(Mutex::new(settings));
        let _ = DICTIONARY.set(Mutex::new(dict));
        let _ = STATS.set(Mutex::new(whimpr_core::StatsStore::load(&stats_path())));

        // Snippets: seed a self-explanatory example on first run.
        let mut snips = whimpr_core::SnippetStore::load(&snippets_path());
        if snips.entries.is_empty() && !snippets_path().exists() {
            snips.add("my email address", "you@example.com");
            let _ = snips.save(&snippets_path());
        }
        let _ = SNIPPETS.set(Mutex::new(snips));

        // Transforms: defaults on first run.
        let transforms: Vec<super::Transform> = std::fs::read_to_string(transforms_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| {
                let d = super::default_transforms();
                let _ = std::fs::write(
                    transforms_path(),
                    serde_json::to_string_pretty(&d).unwrap_or_default(),
                );
                d
            });
        let _ = TRANSFORMS.set(Mutex::new(transforms));

        rebuild_providers();

        // Start the local cleanup worker in the background (model load takes a few
        // seconds; the first local cleanup waits for it, subsequent ones are fast).
        std::thread::spawn(|| {
            let mut worker = crate::local_llm::spawn_default();
            // Prefill the shared system+few-shot prompt prefix once now; the
            // worker's KV cache keeps it, so the first real cleanup only pays
            // for the transcript tokens.
            if let Some(w) = worker.as_mut() {
                let msgs = whimpr_core::cleanup::build_messages(
                    "warm up",
                    &whimpr_core::cleanup::CleanupContext::default(),
                );
                let t = Instant::now();
                let _ = w.cleanup(&msgs);
                eprintln!("[whimpr] local LLM warmed up in {:?}", t.elapsed());
            }
            let _ = LOCAL.set(Mutex::new(worker));
        });

        // Accessibility is the ONE permission that makes the Fn CGEventTap global AND
        // lets us post the Cmd+V paste into other apps. Without it, a keyboard tap is
        // silently limited to frontmost-only — the exact bug. Prompt for it up front.
        if crate::paste::is_trusted() {
            eprintln!("[whimpr] Accessibility granted — Fn works in every app, paste enabled");
        } else {
            eprintln!(
                "[whimpr] ⚠ Accessibility NOT granted — Fn only works while WhimprFlow is \
                 frontmost and paste is disabled. Prompting; grant WhimprFlow under System \
                 Settings → Privacy & Security → Accessibility (no relaunch needed)."
            );
            crate::paste::prompt_accessibility();
        }
        // Input Monitoring is NOT the gate for a CGEventTap — kept only as diagnostics.
        eprintln!(
            "[whimpr] (info) Input Monitoring: {}",
            crate::paste::input_monitoring_granted()
        );

        // Periodic tick drives the double-tap timeout / session cap.
        std::thread::spawn(|| loop {
            std::thread::sleep(Duration::from_millis(100));
            handle_input(Input::Tick { now_ms: now_ms() });
        });

        // The event tap runs on a thread with its own CFRunLoop. CRITICAL: create it
        // ONLY after the process is trusted for Accessibility. macOS fixes a keyboard
        // tap's privilege at CGEventTapCreate time — a tap born untrusted is
        // permanently frontmost-only and is NOT upgraded when the grant later arrives.
        // Polling here also means the Fn key starts working the moment the user grants
        // Accessibility, without a relaunch.
        std::thread::spawn(|| {
            while !crate::paste::is_trusted() {
                std::thread::sleep(Duration::from_millis(500));
            }
            eprintln!("[whimpr] Accessibility present — creating global Fn tap");
            let port = unsafe {
                CGEventTapCreate(
                    K_CG_SESSION_EVENT_TAP,
                    K_CG_HEAD_INSERT,
                    K_CG_TAP_OPTION_DEFAULT,
                    EVENTS_OF_INTEREST,
                    tap_callback,
                    null_mut(),
                )
            };
            if port.is_null() {
                eprintln!(
                    "[whimpr] Fn tap null despite Accessibility — likely a stale TCC entry from \
                     an earlier build. Run: tccutil reset Accessibility com.whimpr.whimprflow, \
                     then re-grant and relaunch."
                );
                return;
            }
            TAP_PORT.store(port, Ordering::SeqCst);
            unsafe {
                let source = CFMachPortCreateRunLoopSource(null(), port, 0);
                CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopDefaultMode);
                CGEventTapEnable(port, true);
                CFRunLoopRun();
            }
        });
    }
}

#[cfg(target_os = "macos")]
pub use imp::{
    current_settings, dictionary_add, dictionary_entries, dictionary_learn, dictionary_remove,
    history, install, last_dictation, rebuild_providers, run_transform, scratchpad_get,
    scratchpad_set, set_overlay_hit, snippets_add, snippets_entries, snippets_remove,
    stats_summary, transforms_list, transforms_save, trigger_cancel, trigger_hands_free,
    trigger_stop, update_settings,
};

// The UI trigger commands are macOS-only for now; inert elsewhere.
#[cfg(not(target_os = "macos"))]
pub fn trigger_hands_free() {}
#[cfg(not(target_os = "macos"))]
pub fn trigger_stop() {}
#[cfg(not(target_os = "macos"))]
pub fn trigger_cancel() {}
#[cfg(not(target_os = "macos"))]
pub fn set_overlay_hit(_hit: OverlayHit) {}
#[cfg(not(target_os = "macos"))]
pub fn snippets_entries() -> Vec<whimpr_core::Snippet> {
    Vec::new()
}
#[cfg(not(target_os = "macos"))]
pub fn snippets_add(_trigger: String, _replacement: String) {}
#[cfg(not(target_os = "macos"))]
pub fn snippets_remove(_trigger: &str) {}
#[cfg(not(target_os = "macos"))]
pub fn scratchpad_get() -> String {
    String::new()
}
#[cfg(not(target_os = "macos"))]
pub fn scratchpad_set(_text: &str) {}
#[cfg(not(target_os = "macos"))]
pub fn transforms_list() -> Vec<Transform> {
    default_transforms()
}
#[cfg(not(target_os = "macos"))]
pub fn transforms_save(_list: Vec<Transform>) {}
#[cfg(not(target_os = "macos"))]
pub fn run_transform(_shortcut: &str) {}
#[cfg(not(target_os = "macos"))]
pub fn last_dictation() -> Option<String> {
    None
}

// Windows uses the real (but unverified) platform layer in `crate::win`.
#[cfg(target_os = "windows")]
pub use crate::win::{
    current_settings, dictionary_add, dictionary_entries, dictionary_learn, dictionary_remove,
    history, install, rebuild_providers, stats_summary, update_settings,
};

// Other platforms (Linux, etc.): inert stubs so the crate still builds.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other {
    pub fn install(_app: tauri::AppHandle) {}
    pub fn current_settings() -> whimpr_core::Settings {
        whimpr_core::Settings::default()
    }
    pub fn update_settings(_new: whimpr_core::Settings) {}
    pub fn rebuild_providers() {}
    pub fn stats_summary(tz_offset_minutes: i32) -> whimpr_core::StatsSummary {
        whimpr_core::StatsStore::default().summary(tz_offset_minutes, 0)
    }
    pub fn history(_limit: usize) -> Vec<whimpr_core::HistoryItem> {
        Vec::new()
    }
    pub fn dictionary_entries() -> Vec<super::DictEntryDto> {
        Vec::new()
    }
    pub fn dictionary_add(_correct: String, _mishears: Vec<String>) {}
    pub fn dictionary_remove(_correct: &str) {}
    pub fn dictionary_learn(_correct: String, _mishears: Vec<String>) {}
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub use other::{
    current_settings, dictionary_add, dictionary_entries, dictionary_learn, dictionary_remove,
    history, install, rebuild_providers, stats_summary, update_settings,
};
