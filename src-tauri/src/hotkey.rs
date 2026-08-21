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
            // Wake sleeping Electron accessibility trees up front so both this
            // snapshot and the later pre-paste focus probe can see the field.
            crate::appctx::wake_ax_for_frontmost();
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

    /// Append each raw/clean pair to cleanup-debug.log (stays on this Mac, like
    /// the history) so cleanup quality is tunable against real dictations.
    /// Truncates once the log grows past ~500 KB.
    fn log_cleanup_pair(raw: &str, clean: &str) {
        use std::io::Write;
        let path = support_dir().join("cleanup-debug.log");
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() > 500_000 {
                let _ = std::fs::remove_file(&path);
            }
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "RAW: {raw}\nOUT: {clean}\n---");
        }
    }

    /// Append one line to music-debug.log so pause/resume behavior is
    /// diagnosable for an app launched via Finder (stderr goes nowhere).
    fn music_log(line: &str) {
        use std::io::Write;
        let msg = format!("{} {}\n", now_ms(), line);
        eprint!("[whimpr] music: {msg}");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(support_dir().join("music-debug.log"))
        {
            let _ = f.write_all(msg.as_bytes());
        }
    }

    fn run_music_script(cmd: &str) -> (bool, String, String) {
        let out = std::process::Command::new("osascript")
            .arg("-e")
            .arg(music_script(cmd))
            .output();
        match out {
            Ok(o) => (
                o.status.success(),
                String::from_utf8_lossy(&o.stdout).trim().to_string(),
                String::from_utf8_lossy(&o.stderr).trim().to_string(),
            ),
            Err(e) => (false, String::new(), format!("spawn failed: {e}")),
        }
    }

    fn pause_music_if_playing() {
        if !current_settings().mute_music_while_dictating {
            return;
        }
        std::thread::spawn(|| {
            music_log("pause: start");
            let (ok, acted, err) = run_music_script("pause");
            music_log(&format!("pause: ok={ok} acted=\"{acted}\" err=\"{err}\""));
            if ok && !acted.is_empty() {
                MUSIC_PAUSED.store(true, Ordering::SeqCst);
            }
        });
    }

    fn resume_music_if_we_paused() {
        if !MUSIC_PAUSED.swap(false, Ordering::SeqCst) {
            music_log("resume: skipped (flag not set)");
            return;
        }
        std::thread::spawn(|| {
            music_log("resume: start");
            let (ok, acted, err) = run_music_script("play");
            music_log(&format!("resume: ok={ok} acted=\"{acted}\" err=\"{err}\""));
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

    /// Load (or re-load after a download) the whisper engine off-thread.
    /// No-op when it's already loaded or no model file exists yet.
    pub fn load_asr() {
        std::thread::spawn(|| {
            if ASR.get().is_some() {
                return;
            }
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
    }

    /// Spawn the local cleanup worker after its model was downloaded at
    /// runtime. No-op when a worker is already running.
    pub fn load_local_llm() {
        std::thread::spawn(|| {
            // install() populates LOCAL from its own thread; give it a moment.
            for _ in 0..10 {
                if LOCAL.get().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            let Some(slot) = LOCAL.get() else { return };
            if slot.lock().unwrap().is_some() {
                return;
            }
            let mut worker = crate::local_llm::spawn_default();
            if let Some(w) = worker.as_mut() {
                let msgs = whimpr_core::cleanup::build_messages(
                    "warm up",
                    &whimpr_core::cleanup::CleanupContext::default(),
                );
                let t = Instant::now();
                let _ = w.cleanup(&msgs);
                eprintln!("[whimpr] local LLM warmed up in {:?}", t.elapsed());
            }
            if worker.is_some() {
                *slot.lock().unwrap() = worker;
            }
        });
    }

    /// Which model files are present (speech recognition, local cleanup LLM).
    pub fn model_status() -> (bool, bool) {
        (model_path().exists(), crate::local_llm::model_present())
    }

    #[derive(Clone, Serialize)]
    struct ModelProgress {
        kind: String,
        done: u64,
        total: u64,
    }

    /// Download a model into the support dir with progress events for the Hub
    /// (`whimpr://model/progress`), then bring the matching engine up live.
    pub fn download_model(kind: &str) -> Result<(), String> {
        let (url, name) = match kind {
            "asr" => (
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
                "ggml-large-v3-turbo-q5_0.bin",
            ),
            "llm" => (
                "https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
                "qwen3-4b-instruct-2507-q4_k_m.gguf",
            ),
            _ => return Err("unknown model kind".into()),
        };
        let dest = support_dir().join("models").join(name);
        if !dest.exists() {
            let app = APP.get().cloned();
            let kind_owned = kind.to_string();
            let mut last_emit: u64 = 0;
            whimpr_cleanup::download_file(url, &dest, |done, total| {
                // Throttle UI updates to roughly every 2 MB (plus the final one).
                if done.saturating_sub(last_emit) > 2_000_000 || done == total {
                    last_emit = done;
                    if let Some(a) = &app {
                        let _ = a.emit(
                            "whimpr://model/progress",
                            ModelProgress { kind: kind_owned.clone(), done, total },
                        );
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        }
        match kind {
            "asr" => load_asr(),
            "llm" => load_local_llm(),
            _ => {}
        }
        Ok(())
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
            .map(|m| m.lock().unwrap().summary(tz_offset_minutes, unix_now(), current_settings().typing_wpm))
            .unwrap_or_else(|| {
                whimpr_core::StatsStore::default().summary(tz_offset_minutes, unix_now(), 45)
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
        // Login keychain via the security CLI (matches how set_api_key stores
        // it); the keyring crate's data-protection keychain is a fallback for
        // items created by older builds.
        if let Ok(out) = std::process::Command::new("/usr/bin/security")
            .args(["find-generic-password", "-s", "com.whimpr.whimprflow", "-a", account, "-w"])
            .output()
        {
            if out.status.success() {
                let k = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !k.is_empty() {
                    return Some(k);
                }
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
    #[allow(dead_code)]
    fn clean_transcript(raw: &str) -> String {
        clean_transcript_with(raw, None)
    }

    fn clean_transcript_with(raw: &str, translate_to: Option<String>) -> String {
        let cleaned = clean_transcript_inner(raw, translate_to);
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

    /// Send one chunk through the configured cleanup provider, falling back to the
    /// on-device worker when a cloud key can't be read. `None` means "no provider
    /// available" (or Raw mode), which callers treat as "keep the text as is".
    fn run_cleanup_providers(
        raw: &str,
        ctx: &CleanupContext,
        mode: CleanupMode,
    ) -> Option<anyhow::Result<String>> {
        // System prompt + few-shot demonstration turns + the transcript, so the
        // on-device model actually produces newlines/lists and resolves
        // self-corrections instead of just being told to.
        let run_local = || -> Option<anyhow::Result<String>> {
            LOCAL.get().and_then(|m| {
                m.lock().unwrap().as_mut().map(|w| {
                    let messages = whimpr_core::cleanup::build_messages(raw, ctx);
                    w.cleanup(&messages)
                })
            })
        };
        match mode {
            CleanupMode::OpenAi => OPENAI
                .get()
                .and_then(|m| m.lock().unwrap().as_ref().map(|p| p.cleanup(raw, ctx)))
                .or_else(run_local),
            CleanupMode::Anthropic => ANTHROPIC
                .get()
                .and_then(|m| m.lock().unwrap().as_ref().map(|p| p.cleanup(raw, ctx)))
                .or_else(run_local),
            CleanupMode::Local => run_local(),
            CleanupMode::Raw => None,
        }
    }

    /// Split text into chunks of at most `max_chars`, breaking on paragraph and
    /// then sentence boundaries so a chunk never ends mid-sentence.
    fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
        let mut chunks: Vec<String> = Vec::new();
        let mut cur = String::new();
        for para in text.split("\n\n") {
            for sentence in split_sentences(para) {
                if !cur.is_empty() && cur.chars().count() + sentence.chars().count() > max_chars {
                    chunks.push(std::mem::take(&mut cur));
                }
                cur.push_str(&sentence);
            }
            if !cur.is_empty() {
                cur.push_str("\n\n");
            }
        }
        let last = cur.trim_end().to_string();
        if !last.is_empty() {
            chunks.push(last);
        }
        chunks
    }

    /// Split on sentence enders, keeping the punctuation and trailing space.
    fn split_sentences(para: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = String::new();
        for ch in para.chars() {
            cur.push(ch);
            if matches!(ch, '.' | '!' | '?') {
                out.push(std::mem::take(&mut cur));
            }
        }
        if !cur.trim().is_empty() {
            out.push(cur);
        }
        out
    }

    /// Clean arbitrary text on request (the Transcribe pane's cleanup button):
    /// the same engine and level the user configured, but with no paste target,
    /// no app style, and no window context — and chunked so long transcripts do
    /// not get truncated by the model. Returns the input unchanged when cleanup
    /// is off or no provider is available.
    pub fn cleanup_text_on_demand(text: &str) -> String {
        let settings = current_settings();
        let level = settings.cleanup_level;
        if matches!(settings.cleanup_mode, CleanupMode::Raw) || level.bypasses_llm() {
            return text.to_string();
        }
        let mut out: Vec<String> = Vec::new();
        for chunk in chunk_text(text, 1200) {
            let vocab = DICTIONARY
                .get()
                .map(|d| d.lock().unwrap().prefilter(&chunk, 15))
                .unwrap_or_default();
            let ctx = CleanupContext { level, vocab, ..Default::default() };
            let cleaned = match run_cleanup_providers(&chunk, &ctx, settings.cleanup_mode) {
                Some(Ok(c)) => {
                    let c = whimpr_core::cleanup::post_process(&c);
                    if whimpr_core::cleanup::evaluate_gates(&chunk, &c, level).passed() {
                        c
                    } else {
                        eprintln!("[whimpr] on-demand cleanup gate rejected a chunk — keeping it raw");
                        chunk.clone()
                    }
                }
                Some(Err(e)) => {
                    eprintln!("[whimpr] on-demand cleanup failed ({e}) — keeping the chunk raw");
                    chunk.clone()
                }
                None => chunk.clone(),
            };
            out.push(cleaned);
        }
        out.join("\n\n")
    }

    fn clean_transcript_inner(raw: &str, translate_to: Option<String>) -> String {
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
        let raw_norm =
            whimpr_core::cleanup::strip_hesitations(&whimpr_core::cleanup::pre_normalize_layout(raw));
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
        let translating = translate_to.is_some();
        let ctx = CleanupContext {
            level,
            vocab,
            app_bundle_id,
            style: Some(style),
            window_context,
            translate_to,
            ..Default::default()
        };
        let result = run_cleanup_providers(raw, &ctx, settings.cleanup_mode);
        match result {
            Some(Ok(cleaned)) => {
                // Deterministic safety net: convert any leftover spoken layout cue the
                // model missed into real line breaks, strip stray code fences, cap blank
                // lines. Guarantees no "new line"/"new paragraph" word reaches the cursor.
                let cleaned = whimpr_core::cleanup::post_process(&cleaned);
                if translating
                    || whimpr_core::cleanup::evaluate_gates(&raw_out, &cleaned, level).passed()
                {
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

    /// Minimal WAV container around 16 kHz mono PCM for cloud transcription.
    fn wav_from_pcm16k(pcm: &[f32]) -> Vec<u8> {
        let data_len = pcm.len() * 2;
        let mut w = Vec::with_capacity(44 + data_len);
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data_len as u32).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes()); // PCM
        w.extend_from_slice(&1u16.to_le_bytes()); // mono
        w.extend_from_slice(&16_000u32.to_le_bytes());
        w.extend_from_slice(&32_000u32.to_le_bytes()); // byte rate
        w.extend_from_slice(&2u16.to_le_bytes()); // block align
        w.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(data_len as u32).to_le_bytes());
        for &sample in pcm {
            w.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
        w
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
        if current_settings().mute_music_while_dictating {
            music_log(&format!("pill -> {state}"));
        }
        match state {
            "recording" | "locked" => {
                play_sound("Tink");
                pause_music_if_playing();
            }
            // Resume as soon as the mic is off — no need to sit in silence
            // through transcription/cleanup.
            "transcribing" => resume_music_if_we_paused(),
            "done" => {
                play_sound("Pop");
                resume_music_if_we_paused();
            }
            "idle" | "cancelled" | "error" | "clipboard" => resume_music_if_we_paused(),
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
                // The clipboard notice is the longest pill text (longer still
                // in German) — give the window room so the edge never clips it.
                ("clipboard", false) => (256.0, 52.0),
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
                    let pcm = whimpr_audio::resample_to_16k(&res.samples, res.sample_rate);
                    let asr_settings = current_settings();
                    // Cloud ASR first when selected (Mistral Voxtral / Groq / OpenAI),
                    // falling back to the local model on any failure.
                    let cloud_text: Option<String> =
                        if matches!(asr_settings.asr_mode, whimpr_core::AsrMode::Cloud) {
                            match read_openai_key() {
                                Some(key) => {
                                    let t0 = Instant::now();
                                    let wav = wav_from_pcm16k(&pcm);
                                    match whimpr_cleanup::transcribe_cloud(
                                        &asr_settings.asr_base_url,
                                        &key,
                                        &asr_settings.asr_model,
                                        wav,
                                    ) {
                                        Ok(t) => {
                                            eprintln!(
                                                "[whimpr] cloud ASR ({}) in {:?}",
                                                asr_settings.asr_model,
                                                t0.elapsed()
                                            );
                                            Some(t)
                                        }
                                        Err(e) => {
                                            eprintln!(
                                                "[whimpr] cloud ASR failed ({e}) — local fallback"
                                            );
                                            None
                                        }
                                    }
                                }
                                None => {
                                    eprintln!("[whimpr] cloud ASR: no API key — local fallback");
                                    None
                                }
                            }
                        } else {
                            None
                        };
                    let transcribed: anyhow::Result<String> = match cloud_text {
                        // Cloud sometimes returns empty for quiet-but-real audio;
                        // give the local model a shot before giving up.
                        Some(t) if t.trim().is_empty() && peak > 0.02 => {
                            eprintln!("[whimpr] cloud ASR empty despite audio — trying local");
                            match ASR.get().cloned() {
                                Some(asr) => asr.transcribe(&pcm).map(|t| t.text),
                                None => Ok(t),
                            }
                        }
                        Some(t) => Ok(t),
                        None => match ASR.get().cloned() {
                            Some(asr) => asr.transcribe(&pcm).map(|t| t.text),
                            None => Err(anyhow::anyhow!(
                                "ASR not ready (model still loading or missing)"
                            )),
                        },
                    };
                    match transcribed {
                        Ok(t) => {
                            let mut raw = t;
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
                            match SPEAK_MODE.swap(0, Ordering::SeqCst) {
                                1 => {
                                    if !raw.is_empty() {
                                        run_spoken_command(&raw);
                                    }
                                    finish();
                                    return;
                                }
                                2 => {
                                    if !raw.is_empty() {
                                        run_spoken_ask(&raw);
                                    }
                                    finish();
                                    return;
                                }
                                _ => {}
                            }
                            let (translate_to, raw) = split_translate_prefix(&raw);
                            let mut text = clean_transcript_with(&raw, translate_to);
                            if text != raw {
                                eprintln!("[whimpr] CLEANED:   \"{}\"", text);
                            }
                            log_cleanup_pair(&raw, &text);
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
                                // Paste with landing verification: if the text
                                // can't be confirmed in the focused field, it
                                // stays in the clipboard and the pill says so.
                                let (probe, ax_err1, ax_err2) =
                                    crate::appctx::probe_text_focus();
                                music_log(&format!(
                                    "paste: trusted={} probe={:?} err1={} err2={} front={:?}",
                                    crate::appctx::ax_trusted(),
                                    probe,
                                    ax_err1,
                                    ax_err2,
                                    crate::appctx::frontmost_bundle_id()
                                ));
                                let pasted = match probe {
                                    crate::appctx::FocusProbe::Text => {
                                        let r = crate::paste::paste_text_verified(&text)
                                            .unwrap_or(false);
                                        music_log(&format!("paste: verified={r}"));
                                        // Unverified is usually just AX not
                                        // reflecting the field (Electron), not
                                        // a failed paste: Cmd+V went to a real
                                        // text field. Don't flash "Not pasted";
                                        // the dictation stays in the clipboard
                                        // as the safety net either way.
                                        true
                                    }
                                    // AX can't see into this app (sleeping
                                    // Electron tree): paste blind like the
                                    // pre-verify builds — verification would
                                    // be blind too and only cry wolf.
                                    crate::appctx::FocusProbe::Unknown => {
                                        let r = crate::paste::paste_text(&text).is_ok();
                                        music_log(&format!("paste: blind ok={r}"));
                                        r
                                    }
                                    crate::appctx::FocusProbe::NoText => {
                                        if let Ok(mut cb) = arboard::Clipboard::new() {
                                            let _ = cb.set_text(text.clone());
                                        }
                                        false
                                    }
                                };
                                if !pasted {
                                    eprintln!(
                                        "[whimpr] paste not confirmed — dictation kept in clipboard"
                                    );
                                    emit_bar(&app2, "clipboard");
                                    let app3 = app2.clone();
                                    std::thread::spawn(move || {
                                        std::thread::sleep(Duration::from_millis(3200));
                                        emit_bar(&app3, "idle");
                                    });
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

    // --- Audio-file transcription (drag a file into the hub) --------------

    /// Parse a 16 kHz mono 16-bit WAV (as produced by afconvert) into f32 PCM.
    fn wav_to_pcm16k(bytes: &[u8]) -> Option<Vec<f32>> {
        if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return None;
        }
        let mut i = 12;
        while i + 8 <= bytes.len() {
            let id = &bytes[i..i + 4];
            let size = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().ok()?) as usize;
            if id == b"data" {
                let data = bytes.get(i + 8..i + 8 + size)?;
                return Some(
                    data.chunks_exact(2)
                        .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
                        .collect(),
                );
            }
            i += 8 + size + (size & 1);
        }
        None
    }

    fn file_transcripts_path() -> PathBuf {
        support_dir().join("file-transcripts.json")
    }

    fn save_file_transcript(filename: &str, text: &str) {
        let mut list: Vec<serde_json::Value> = std::fs::read_to_string(file_transcripts_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        list.insert(
            0,
            serde_json::json!({ "filename": filename, "text": text, "ts_unix": ts }),
        );
        list.truncate(50);
        let _ = std::fs::write(
            file_transcripts_path(),
            serde_json::to_string_pretty(&list).unwrap_or_default(),
        );
    }

    pub fn file_transcripts() -> Vec<serde_json::Value> {
        std::fs::read_to_string(file_transcripts_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Transcribe an audio file (m4a/mp3/wav/aiff/…): convert with the built-in
    /// afconvert, then run the configured engine (cloud first, local fallback).
    pub fn transcribe_audio_file(path: &str) -> Result<String, String> {
        let src = std::path::Path::new(path);
        if !src.exists() {
            return Err(format!("file not found: {path}"));
        }
        let tmp = support_dir().join("tmp-transcribe.wav");
        let out = std::process::Command::new("afconvert")
            .arg("-f")
            .arg("WAVE")
            .arg("-d")
            .arg("LEI16@16000")
            .arg("-c")
            .arg("1")
            .arg(src)
            .arg(&tmp)
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!(
                "could not decode this file: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let bytes = std::fs::read(&tmp).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&tmp);

        let settings = current_settings();
        // Cloud engine first when selected and available.
        if matches!(settings.asr_mode, whimpr_core::AsrMode::Cloud) {
            if let Some(key) = read_openai_key() {
                let t0 = Instant::now();
                match whimpr_cleanup::transcribe_cloud(
                    &settings.asr_base_url,
                    &key,
                    &settings.asr_model,
                    bytes.clone(),
                ) {
                    Ok(t) if !t.trim().is_empty() => {
                        eprintln!(
                            "[whimpr] file transcribed via cloud in {:?} ({} chars)",
                            t0.elapsed(),
                            t.len()
                        );
                        let t = apply_dictionary_spellings(&t);
                        save_file_transcript(
                            src.file_name().and_then(|f| f.to_str()).unwrap_or("audio"),
                            &t,
                        );
                        return Ok(t);
                    }
                    Ok(_) => eprintln!("[whimpr] cloud file ASR empty — trying local"),
                    Err(e) => eprintln!("[whimpr] cloud file ASR failed ({e}) — trying local"),
                }
            }
        }
        let pcm = wav_to_pcm16k(&bytes).ok_or("could not parse converted audio")?;
        let asr = ASR
            .get()
            .cloned()
            .ok_or("local speech model not loaded (and no cloud engine available)")?;
        let t0 = Instant::now();
        let t = asr.transcribe(&pcm).map_err(|e| e.to_string())?;
        eprintln!(
            "[whimpr] file transcribed locally in {:?} ({} chars)",
            t0.elapsed(),
            t.text.len()
        );
        let text = apply_dictionary_spellings(&t.text);
        save_file_transcript(
            src.file_name().and_then(|f| f.to_str()).unwrap_or("audio"),
            &text,
        );
        Ok(text)
    }

    /// Fix known mishears and casing from the user's dictionary. Deterministic and
    /// model-free, so it is safe to run on a whole file transcript.
    fn apply_dictionary_spellings(text: &str) -> String {
        DICTIONARY
            .get()
            .map(|d| d.lock().unwrap().apply_spellings(text))
            .unwrap_or_else(|| text.to_string())
    }

    // --- In-app assistant ("Ask Whimpr") ----------------------------------

    const ASSISTANT_SYSTEM: &str = r#"You are Whimpr, the assistant inside the WhimprFlow dictation app. You chat naturally in the user's language (usually German) and you can change the app's data when asked.

To perform actions, reply with EXACTLY this JSON shape and nothing else:
{"reply":"<your short answer to the user>","actions":[...]}

Supported actions:
- {"type":"add_snippet","trigger":"<spoken phrase>","replacement":"<text it expands to>"}
- {"type":"remove_snippet","trigger":"<spoken phrase>"}
- {"type":"add_dictionary","correct":"<correct spelling>","mishears":["<misheard as>", ...]}
- {"type":"append_scratchpad","text":"<text to append>"}
- {"type":"search_history","query":"<keyword>"} (searches the user's dictation history; results come back to you in a follow-up turn, then answer)

Rules: only include actions the user clearly asked for; use an empty actions array for plain conversation; never invent personal data; keep replies short and concrete. Always return valid JSON, no markdown fences."#;

    /// One assistant turn: history in, {reply, actions_done} out. Actions the
    /// model requests are executed against the real stores.
    pub fn assistant_chat(history: Vec<(String, String)>) -> serde_json::Value {
        let settings = current_settings();
        let mut messages: Vec<(String, String)> =
            vec![("system".to_string(), ASSISTANT_SYSTEM.to_string())];
        messages.extend(history);

        let raw = read_openai_key()
            .and_then(|key| {
                whimpr_cleanup::chat_completion_messages(
                    &settings.openai_base_url,
                    &key,
                    &settings.openai_model,
                    &messages,
                )
                .map_err(|e| eprintln!("[whimpr] assistant cloud failed: {e}"))
                .ok()
            })
            .or_else(|| {
                LOCAL.get().and_then(|m| {
                    m.lock().unwrap().as_mut().and_then(|w| {
                        let msgs: Vec<whimpr_core::cleanup::CleanupMsg> = messages
                            .iter()
                            .map(|(role, content)| whimpr_core::cleanup::CleanupMsg {
                                role: match role.as_str() {
                                    "system" => "system",
                                    "assistant" => "assistant",
                                    _ => "user",
                                },
                                content: content.clone(),
                            })
                            .collect();
                        w.cleanup(&msgs).ok()
                    })
                })
            });

        let Some(raw) = raw else {
            return serde_json::json!({
                "reply": "No language model available. Add an API key in Settings or install the local model.",
                "actions_done": [],
            });
        };

        // Model may wrap JSON in fences or slip into plain text; handle both.
        let cleaned = raw
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
            .to_string();
        let parsed: Option<serde_json::Value> = serde_json::from_str(&cleaned).ok().or_else(|| {
            let start = cleaned.find('{')?;
            let end = cleaned.rfind('}')?;
            serde_json::from_str(&cleaned[start..=end]).ok()
        });

        let Some(v) = parsed else {
            return serde_json::json!({ "reply": cleaned, "actions_done": [] });
        };
        let reply = v
            .get("reply")
            .and_then(|r| r.as_str())
            .unwrap_or("")
            .to_string();
        let mut done: Vec<String> = Vec::new();
        let mut search_results: Option<String> = None;
        if let Some(actions) = v.get("actions").and_then(|a| a.as_array()) {
            for a in actions {
                match a.get("type").and_then(|t| t.as_str()) {
                    Some("add_snippet") => {
                        let (Some(t), Some(r)) = (
                            a.get("trigger").and_then(|x| x.as_str()),
                            a.get("replacement").and_then(|x| x.as_str()),
                        ) else {
                            continue;
                        };
                        snippets_add(t.to_string(), r.to_string());
                        done.push(format!("Snippet: \"{t}\""));
                    }
                    Some("remove_snippet") => {
                        if let Some(t) = a.get("trigger").and_then(|x| x.as_str()) {
                            snippets_remove(t);
                            done.push(format!("Snippet removed: \"{t}\""));
                        }
                    }
                    Some("add_dictionary") => {
                        let Some(c) = a.get("correct").and_then(|x| x.as_str()) else {
                            continue;
                        };
                        let mishears: Vec<String> = a
                            .get("mishears")
                            .and_then(|m| m.as_array())
                            .map(|m| {
                                m.iter()
                                    .filter_map(|x| x.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();
                        dictionary_add(c.to_string(), mishears);
                        done.push(format!("Dictionary: {c}"));
                    }
                    Some("search_history") => {
                        if let Some(q) = a.get("query").and_then(|x| x.as_str()) {
                            let ql = q.to_lowercase();
                            let stats = STATS
                                .get()
                                .map(|s| s.lock().unwrap().clone())
                                .unwrap_or_default();
                            let mut hits: Vec<String> = stats
                                .sessions
                                .iter()
                                .rev()
                                .filter(|sess| sess.text.to_lowercase().contains(&ql))
                                .take(20)
                                .map(|sess| {
                                    let days_ago =
                                        (std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .map(|d| d.as_secs() as i64)
                                            .unwrap_or(0)
                                            - sess.ts_unix as i64)
                                            / 86_400;
                                    format!("[{days_ago}d ago] {}", sess.text)
                                })
                                .collect();
                            if hits.is_empty() {
                                hits.push("(no matches)".to_string());
                            }
                            search_results = Some(hits.join("\n"));
                            done.push(format!("History searched: \"{q}\""));
                        }
                    }
                    Some("append_scratchpad") => {
                        if let Some(t) = a.get("text").and_then(|x| x.as_str()) {
                            let mut cur = scratchpad_get();
                            if !cur.is_empty() && !cur.ends_with('\n') {
                                cur.push('\n');
                            }
                            cur.push_str(t);
                            cur.push('\n');
                            scratchpad_set(&cur);
                            done.push("Scratchpad updated".to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
        // Second round when the model asked for a history search.
        if let Some(results) = search_results {
            let mut follow = messages.clone();
            follow.push(("assistant".to_string(), cleaned.clone()));
            follow.push((
                "user".to_string(),
                format!(
                    "[search results]\n{results}\n\nAnswer the user's question now (same JSON \
                     shape, usually with an empty actions array)."
                ),
            ));
            if let Some(second) = read_openai_key().and_then(|key| {
                whimpr_cleanup::chat_completion_messages(
                    &current_settings().openai_base_url,
                    &key,
                    &current_settings().openai_model,
                    &follow,
                )
                .ok()
            }) {
                let cleaned2 = second
                    .trim()
                    .trim_start_matches("```json")
                    .trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim()
                    .to_string();
                let reply2 = serde_json::from_str::<serde_json::Value>(&cleaned2)
                    .ok()
                    .and_then(|v| v.get("reply").and_then(|r| r.as_str()).map(String::from))
                    .unwrap_or(cleaned2);
                return serde_json::json!({ "reply": reply2, "actions_done": done });
            }
        }
        serde_json::json!({ "reply": reply, "actions_done": done })
    }

    // --- Command mode / Ask-anywhere (hold shortcut, speak) ---------------

    /// 0 = normal dictation, 1 = command mode (rewrite selection), 2 = ask.
    static SPEAK_MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
    static CMD_SELECTION: OnceLock<Mutex<Option<String>>> = OnceLock::new();

    /// Hold-to-speak command mode: grabs the selection, then records the spoken
    /// instruction while the shortcut is held.
    pub fn command_mode_down() {
        let sel = crate::paste::copy_selection().ok().flatten();
        if sel.is_none() {
            eprintln!("[whimpr] command mode: no selection");
            if let Some(app) = APP.get() {
                emit_bar(app, "error");
                reset_bar_soon(app);
            }
            return;
        }
        *CMD_SELECTION.get_or_init(|| Mutex::new(None)).lock().unwrap() = sel;
        SPEAK_MODE.store(1, Ordering::SeqCst);
        let target = crate::appctx::frontmost_bundle_id();
        *TARGET_APP.get_or_init(|| Mutex::new(None)).lock().unwrap() = target;
        handle_input(Input::Trigger(TriggerToken::Down {
            binding: BindingId::HandsFree,
            at_ms: now_ms(),
        }));
    }

    pub fn command_mode_up() {
        if SPEAK_MODE.load(Ordering::SeqCst) != 1 {
            return;
        }
        handle_input(Input::Trigger(TriggerToken::Down {
            binding: BindingId::PushToTalk,
            at_ms: now_ms(),
        }));
    }

    /// Hold-to-speak "ask anywhere": records a question, answers via the
    /// assistant, pastes the reply at the cursor.
    pub fn ask_mode_down() {
        SPEAK_MODE.store(2, Ordering::SeqCst);
        let target = crate::appctx::frontmost_bundle_id();
        *TARGET_APP.get_or_init(|| Mutex::new(None)).lock().unwrap() = target;
        handle_input(Input::Trigger(TriggerToken::Down {
            binding: BindingId::HandsFree,
            at_ms: now_ms(),
        }));
    }

    pub fn ask_mode_up() {
        if SPEAK_MODE.load(Ordering::SeqCst) != 2 {
            return;
        }
        handle_input(Input::Trigger(TriggerToken::Down {
            binding: BindingId::PushToTalk,
            at_ms: now_ms(),
        }));
    }

    /// Apply a spoken instruction to the captured selection (command mode).
    fn run_spoken_command(instruction: &str) {
        let Some(selection) = CMD_SELECTION
            .get()
            .and_then(|m| m.lock().unwrap().take())
        else {
            return;
        };
        let system = format!(
            "You are a text editing engine. Apply the spoken instruction to the text. \
             Return ONLY the rewritten text, no preamble, no fences, keep the text's \
             language unless the instruction says otherwise.\nInstruction: {instruction}"
        );
        let settings = current_settings();
        let result = read_openai_key()
            .and_then(|key| {
                whimpr_cleanup::chat_completion(
                    &settings.openai_base_url,
                    &key,
                    &settings.openai_model,
                    &system,
                    &selection,
                )
                .ok()
            })
            .or_else(|| {
                LOCAL.get().and_then(|m| {
                    m.lock().unwrap().as_mut().and_then(|w| {
                        w.cleanup(&[
                            whimpr_core::cleanup::CleanupMsg {
                                role: "system",
                                content: system.clone(),
                            },
                            whimpr_core::cleanup::CleanupMsg {
                                role: "user",
                                content: selection.clone(),
                            },
                        ])
                        .ok()
                    })
                })
            });
        match result {
            Some(out) if !out.trim().is_empty() => {
                if let Err(e) = crate::paste::paste_text(out.trim()) {
                    eprintln!("[whimpr] command paste failed: {e}");
                }
            }
            _ => eprintln!("[whimpr] spoken command produced no output"),
        }
    }

    /// Answer a spoken question via the assistant and paste the reply.
    fn run_spoken_ask(question: &str) {
        let res = assistant_chat(vec![("user".to_string(), question.to_string())]);
        let reply = res.get("reply").and_then(|r| r.as_str()).unwrap_or("");
        if !reply.is_empty() {
            if let Err(e) = crate::paste::paste_text(reply) {
                eprintln!("[whimpr] ask paste failed: {e}");
            }
        }
    }

    /// Spoken translation prefix ("auf Englisch: ..."), returning the target
    /// language and the remaining transcript.
    fn split_translate_prefix(raw: &str) -> (Option<String>, String) {
        let trimmed = raw.trim_start();
        let lower = trimmed.to_lowercase();
        const PREFIXES: [(&str, &str); 8] = [
            ("auf englisch", "English"),
            ("in english", "English"),
            ("auf deutsch", "German"),
            ("in german", "German"),
            ("auf spanisch", "Spanish"),
            ("auf franzoesisch", "French"),
            ("auf französisch", "French"),
            ("auf italienisch", "Italian"),
        ];
        for (pfx, lang) in PREFIXES {
            if lower.starts_with(pfx) {
                let rest = trimmed[pfx.len()..]
                    .trim_start_matches([':', ',', '.', '!', ' '])
                    .to_string();
                if rest.chars().count() > 2 {
                    return (Some(lang.to_string()), rest);
                }
            }
        }
        (None, raw.to_string())
    }

    // --- Contacts import ---------------------------------------------------

    /// Pull contact names from macOS Contacts into the dictionary (spelling
    /// authority for names). Asks for the Contacts permission on first use.
    pub fn import_contacts() -> Result<u32, String> {
        let out = std::process::Command::new("osascript")
            .arg("-e")
            .arg(r#"tell application "Contacts" to get name of every person"#)
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        let names = String::from_utf8_lossy(&out.stdout);
        let existing: std::collections::HashSet<String> = dictionary_entries()
            .into_iter()
            .map(|e| e.correct.to_lowercase())
            .collect();
        let mut added = 0u32;
        for name in names.split(", ") {
            let name = name.trim();
            if name.chars().count() < 3 || name.chars().any(|c| c.is_ascii_digit()) {
                continue;
            }
            if existing.contains(&name.to_lowercase()) {
                continue;
            }
            dictionary_add(name.to_string(), Vec::new());
            added += 1;
        }
        Ok(added)
    }

    // --- Snippet suggestions ----------------------------------------------

    /// Frequently repeated 4-6-word phrases from the history that aren't
    /// snippets yet — candidates for one-phrase expansion.
    pub fn snippet_suggestions() -> Vec<serde_json::Value> {
        let stats = STATS
            .get()
            .map(|s| s.lock().unwrap().clone())
            .unwrap_or_default();
        let existing: std::collections::HashSet<String> = snippets_entries()
            .into_iter()
            .map(|s| s.replacement.to_lowercase())
            .collect();
        let mut freq: std::collections::HashMap<String, u32> = Default::default();
        for sess in &stats.sessions {
            let words: Vec<&str> = sess.text.split_whitespace().collect();
            for n in 4..=6 {
                for win in words.windows(n) {
                    let g = win.join(" ");
                    if g.chars().count() >= 18 {
                        *freq.entry(g).or_default() += 1;
                    }
                }
            }
        }
        let mut list: Vec<(String, u32)> = freq
            .into_iter()
            .filter(|(g, c)| *c >= 4 && !existing.contains(&g.to_lowercase()))
            .collect();
        list.sort_by_key(|(g, c)| std::cmp::Reverse((*c, g.len())));
        // Drop overlapping sub-phrases of higher-ranked suggestions.
        let mut out: Vec<(String, u32)> = Vec::new();
        for (g, c) in list {
            if !out
                .iter()
                .any(|(o, _)| o.to_lowercase().contains(&g.to_lowercase()))
            {
                out.push((g, c));
            }
            if out.len() >= 5 {
                break;
            }
        }
        out.into_iter()
            .map(|(g, c)| serde_json::json!({ "phrase": g, "count": c }))
            .collect()
    }

    // --- Update check + weekly review -------------------------------------

    /// UI language for Rust-side strings (notifications): the settings value,
    /// with "system" resolved from macOS's preferred languages once.
    fn ui_lang_is_de() -> bool {
        match current_settings().language.as_str() {
            "de" => true,
            "en" => false,
            _ => {
                static SYS_DE: OnceLock<bool> = OnceLock::new();
                *SYS_DE.get_or_init(|| {
                    std::process::Command::new("defaults")
                        .args(["read", "-g", "AppleLanguages"])
                        .output()
                        .map(|o| String::from_utf8_lossy(&o.stdout).contains("\"de"))
                        .unwrap_or(false)
                })
            }
        }
    }

    /// Pick the English or German variant of a notification string.
    fn tr<'a>(en: &'a str, de: &'a str) -> &'a str {
        if ui_lang_is_de() { de } else { en }
    }

    fn notify(title: &str, body: &str) {
        // Native notification from the app itself, so a click opens WhimprFlow
        // (an osascript notification belongs to Script Editor and opens that).
        if let Some(app) = APP.get() {
            use tauri_plugin_notification::NotificationExt;
            if app
                .notification()
                .builder()
                .title(title)
                .body(body)
                .show()
                .is_ok()
            {
                return;
            }
        }
        // Fallback for the moments before the app handle exists.
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            body.replace('"', "'"),
            title.replace('"', "'")
        );
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .spawn();
    }

    /// Download the newest release zip, swap /Applications/WhimprFlow.app, and
    /// relaunch. The release is signed with the same identity, so macOS
    /// permissions survive the swap.
    pub fn self_update() -> Result<(), String> {
        let v = whimpr_cleanup::http_get_json(
            "https://api.github.com/repos/chrisznb/WhimprFlow/releases/latest",
        )
        .map_err(|e| e.to_string())?;
        let url = v["assets"]
            .as_array()
            .and_then(|a| {
                a.iter().find(|x| {
                    x["name"].as_str().map(|n| n.ends_with(".zip")).unwrap_or(false)
                })
            })
            .and_then(|x| x["browser_download_url"].as_str())
            .ok_or("no zip asset in the latest release")?
            .to_string();
        let tag = v["tag_name"].as_str().unwrap_or("latest").to_string();
        notify("WhimprFlow", &format!("{} {tag}…", tr("Downloading update", "Lade Update")));

        let tmp_zip = std::env::temp_dir().join("whimpr-update.zip");
        let tmp_dir = std::env::temp_dir().join("whimpr-update");
        let ok = std::process::Command::new("/usr/bin/curl")
            .args(["-sL", "-o"])
            .arg(&tmp_zip)
            .arg(&url)
            .status()
            .map_err(|e| e.to_string())?;
        if !ok.success() {
            return Err("download failed".into());
        }
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
        let ok = std::process::Command::new("ditto")
            .arg("-xk")
            .arg(&tmp_zip)
            .arg(&tmp_dir)
            .status()
            .map_err(|e| e.to_string())?;
        if !ok.success() {
            return Err("unzip failed".into());
        }
        let new_app = tmp_dir.join("WhimprFlow.app");
        if !new_app.exists() {
            return Err("archive did not contain WhimprFlow.app".into());
        }
        let _ = std::process::Command::new("xattr")
            .args(["-dr", "com.apple.quarantine"])
            .arg(&new_app)
            .status();
        let dest = std::path::Path::new("/Applications/WhimprFlow.app");
        std::fs::remove_dir_all(dest).map_err(|e| format!("could not remove old app: {e}"))?;
        let ok = std::process::Command::new("ditto")
            .arg(&new_app)
            .arg(dest)
            .status()
            .map_err(|e| e.to_string())?;
        if !ok.success() {
            return Err("install failed".into());
        }
        let _ = std::fs::remove_file(&tmp_zip);
        let _ = std::fs::remove_dir_all(&tmp_dir);
        notify("WhimprFlow", tr("Update installed. Restarting…", "Update installiert. Starte neu…"));
        let _ = std::process::Command::new("open").arg(dest).spawn();
        std::thread::sleep(Duration::from_millis(600));
        std::process::exit(0);
    }

    /// Tray "Check for updates": compare, then install if newer.
    pub fn check_and_install_update(current: String) {
        std::thread::spawn(move || {
            let latest = whimpr_cleanup::http_get_json(
                "https://api.github.com/repos/chrisznb/WhimprFlow/releases/latest",
            )
            .ok()
            .and_then(|v| v.get("tag_name").and_then(|t| t.as_str().map(String::from)))
            .map(|t| t.trim_start_matches('v').to_string());
            match latest {
                Some(l) if !l.is_empty() && l != current => {
                    if let Err(e) = self_update() {
                        eprintln!("[whimpr] self-update failed: {e}");
                        notify(tr("WhimprFlow update failed", "WhimprFlow-Update fehlgeschlagen"), &e);
                    }
                }
                Some(_) => notify("WhimprFlow", tr("You are up to date.", "Du bist auf dem neuesten Stand.")),
                None => notify("WhimprFlow", tr("Could not reach GitHub.", "GitHub nicht erreichbar.")),
            }
        });
    }

    /// Once at startup: compare the newest GitHub release against this build.
    fn check_for_update(current: String) {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(15));
            let Ok(v) = whimpr_cleanup::http_get_json(
                "https://api.github.com/repos/chrisznb/WhimprFlow/releases/latest",
            ) else {
                return;
            };
            let latest = v
                .get("tag_name")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .trim_start_matches('v')
                .to_string();
            if !latest.is_empty() && latest != current {
                eprintln!("[whimpr] update available: {latest} (running {current})");
                notify(
                    tr("WhimprFlow update available", "WhimprFlow-Update verfügbar"),
                    &format!(
                        "{}",
                        if ui_lang_is_de() {
                            format!("Version {latest} ist auf GitHub (du nutzt {current}).")
                        } else {
                            format!("Version {latest} is on GitHub (you run {current}).")
                        }
                    ),
                );
            }
        });
    }

    fn week_key(secs: i64) -> i64 {
        // Monday-based week bucket of local time.
        let days = (secs + tz_offset_secs()).div_euclid(86_400);
        (days + 3).div_euclid(7) // 1970-01-01 was a Thursday -> +3 aligns Monday
    }

    /// Sunday-evening weekly review: short LLM summary of the week's dictation,
    /// delivered as a notification and appended to the scratchpad.
    fn spawn_weekly_review() {
        std::thread::spawn(|| loop {
            std::thread::sleep(Duration::from_secs(1800));
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let local = now + tz_offset_secs();
            let weekday = ((local.div_euclid(86_400) + 4).rem_euclid(7)) as u32; // 0=Sun
            let hour = (local.rem_euclid(86_400) / 3600) as u32;
            if weekday != 0 || hour < 18 {
                continue;
            }
            let marker = support_dir().join("last-review.json");
            let this_week = week_key(now);
            let last: i64 = std::fs::read_to_string(&marker)
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);
            if last >= this_week {
                continue;
            }
            let stats = STATS
                .get()
                .map(|s| s.lock().unwrap().clone())
                .unwrap_or_default();
            let week_sessions: Vec<_> = stats
                .sessions
                .iter()
                .filter(|s| now - (s.ts_unix as i64) < 7 * 86_400)
                .collect();
            if week_sessions.len() < 5 {
                let _ = std::fs::write(&marker, this_week.to_string());
                continue;
            }
            let words: u64 = week_sessions.iter().map(|s| s.words as u64).sum();
            let sample: String = week_sessions
                .iter()
                .rev()
                .take(50)
                .map(|s| format!("- {}\n", s.text))
                .collect();
            let settings = current_settings();
            let summary = read_openai_key().and_then(|key| {
                whimpr_cleanup::chat_completion(
                    &settings.openai_base_url,
                    &key,
                    &settings.openai_model,
                    "Write a friendly 3-4 sentence weekly dictation review: main topics, \
                     notable patterns, one fun observation. Use the dominant language of \
                     the samples. No dashes, no bullet points, plain sentences only.",
                    &format!("This week: {words} words across {} dictations.\nSamples:\n{sample}",
                        week_sessions.len()),
                )
                .ok()
            });
            if let Some(text) = summary {
                let mut pad = scratchpad_get();
                if !pad.is_empty() && !pad.ends_with('\n') {
                    pad.push('\n');
                }
                pad.push_str(&format!("\n## Wochen-Review\n{text}\n"));
                scratchpad_set(&pad);
                notify(
                    tr("WhimprFlow weekly review", "WhimprFlow Wochen-Review"),
                    &format!(
                        "{}",
                        if ui_lang_is_de() {
                            format!("{words} Wörter diese Woche. Das volle Review steht im Notizblock.")
                        } else {
                            format!("{words} words this week. Full review is in your Scratchpad.")
                        }
                    ),
                );
                eprintln!("[whimpr] weekly review written");
            }
            let _ = std::fs::write(&marker, this_week.to_string());
        });
    }

    // --- Voice profile (Insights -> Your Voice) ---------------------------

    fn voice_profile_path() -> PathBuf {
        support_dir().join("voice-profile.json")
    }

    /// Regenerate every N new words, like the original's "next update in ...".
    const VOICE_REGEN_WORDS: u64 = 20_000;

    pub fn voice_profile(force: bool) -> serde_json::Value {
        let stats = STATS
            .get()
            .map(|s| s.lock().unwrap().clone())
            .unwrap_or_default();
        let total_words: u64 = stats.sessions.iter().map(|s| s.words as u64).sum();

        if !force {
            if let Ok(txt) = std::fs::read_to_string(voice_profile_path()) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                    let gen_at = v.get("generated_at_words").and_then(|x| x.as_u64()).unwrap_or(0);
                    if total_words < gen_at + VOICE_REGEN_WORDS {
                        let mut v = v;
                        v["total_words"] = total_words.into();
                        return v;
                    }
                }
            }
        }

        // --- Pure-Rust analysis over all session texts ---
        let texts: Vec<&str> = stats.sessions.iter().map(|s| s.text.as_str()).collect();
        let mut word_freq: std::collections::HashMap<String, u32> = Default::default();
        let mut ngram_freq: std::collections::HashMap<String, u32> = Default::default();
        for t in &texts {
            let words: Vec<String> = t
                .split_whitespace()
                .map(|w| {
                    w.trim_matches(|c: char| c.is_ascii_punctuation() || c == '\u{201e}' || c == '\u{201c}')
                        .to_lowercase()
                })
                .filter(|w| !w.is_empty())
                .collect();
            for w in &words {
                if w.chars().count() > 2 {
                    *word_freq.entry(w.clone()).or_default() += 1;
                }
            }
            for n in 3..=4 {
                for win in words.windows(n) {
                    let g = win.join(" ");
                    if g.chars().count() >= 10 {
                        *ngram_freq.entry(g).or_default() += 1;
                    }
                }
            }
        }
        let most_used = word_freq
            .iter()
            .max_by_key(|(_, c)| **c)
            .map(|(w, _)| w.clone())
            .unwrap_or_default();
        let catchphrase = ngram_freq
            .iter()
            .filter(|(_, c)| **c >= 3)
            .max_by_key(|(g, c)| (**c, g.len()))
            .map(|(g, _)| g.clone())
            .unwrap_or_else(|| most_used.clone());

        // Peak time: weekday+hour bucket with the most sessions (local time).
        let mut buckets: std::collections::HashMap<(u32, u32), u32> = Default::default();
        for sess in &stats.sessions {
            let secs = sess.ts_unix as i64 + tz_offset_secs();
            let days = secs.div_euclid(86_400);
            let weekday = ((days + 4).rem_euclid(7)) as u32; // 1970-01-01 was a Thursday
            let hour = (secs.rem_euclid(86_400) / 3600) as u32;
            *buckets.entry((weekday, hour)).or_default() += 1;
        }
        const DAYS: [&str; 7] = [
            "Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday",
        ];
        let peak = buckets
            .iter()
            .max_by_key(|(_, c)| **c)
            .map(|((d, h), _)| {
                let ampm = if *h < 12 { "a.m." } else { "p.m." };
                let h12 = if *h % 12 == 0 { 12 } else { *h % 12 };
                format!("{} at {} {}", DAYS[*d as usize % 7], h12, ampm)
            })
            .unwrap_or_default();

        // Most corrected word: the dictionary entry with the most mishears.
        let most_corrected = DICTIONARY
            .get()
            .map(|d| d.lock().unwrap().entries.clone())
            .unwrap_or_default()
            .into_iter()
            .max_by_key(|e| e.mishears.len())
            .filter(|e| !e.mishears.is_empty())
            .map(|e| e.correct)
            .unwrap_or_default();

        // Top apps for the LLM context.
        let mut app_freq: std::collections::HashMap<String, u32> = Default::default();
        for sess in &stats.sessions {
            if let Some(a) = &sess.app {
                *app_freq.entry(a.clone()).or_default() += 1;
            }
        }
        let mut apps: Vec<(String, u32)> = app_freq.into_iter().collect();
        apps.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
        let top_apps: Vec<String> = apps.into_iter().take(3).map(|(a, _)| a).collect();

        // LLM profile text (cloud if configured, else local worker).
        let sample: String = texts
            .iter()
            .rev()
            .take(40)
            .map(|t| format!("- {t}\n"))
            .collect();
        let system = "You write the short 'voice profile' blurb for a dictation app's \
                      insights page. From the user's recent dictations, describe in 2-3 \
                      sentences what they use their voice for and how they communicate, \
                      in the style 'Voice is where you ...'. Write in the dominant \
                      language of the samples. Return only the profile text.";
        let user_msg = format!(
            "Most-used apps: {}\nRecent dictations:\n{}",
            top_apps.join(", "),
            sample
        );
        let settings = current_settings();
        let profile_text = read_openai_key()
            .and_then(|key| {
                whimpr_cleanup::chat_completion(
                    &settings.openai_base_url,
                    &key,
                    &settings.openai_model,
                    system,
                    &user_msg,
                )
                .map_err(|e| eprintln!("[whimpr] voice profile via cloud failed: {e}"))
                .ok()
            })
            .or_else(|| {
                LOCAL.get().and_then(|m| {
                    m.lock().unwrap().as_mut().and_then(|w| {
                        w.cleanup(&[
                            whimpr_core::cleanup::CleanupMsg {
                                role: "system",
                                content: system.to_string(),
                            },
                            whimpr_core::cleanup::CleanupMsg {
                                role: "user",
                                content: user_msg.clone(),
                            },
                        ])
                        .ok()
                    })
                })
            })
            .unwrap_or_default();

        let v = serde_json::json!({
            "profile_text": profile_text,
            "catchphrase": catchphrase,
            "most_used_word": most_used,
            "most_corrected_word": most_corrected,
            "peak_time": peak,
            "generated_at_words": total_words,
            "total_words": total_words,
            "regen_after_words": VOICE_REGEN_WORDS,
        });
        let _ = std::fs::write(
            voice_profile_path(),
            serde_json::to_string_pretty(&v).unwrap_or_default(),
        );
        v
    }

    fn tz_offset_secs() -> i64 {
        // Cheap local-offset probe via `date +%z`.
        std::process::Command::new("date")
            .arg("+%z")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| {
                let s = s.trim();
                if s.len() >= 5 {
                    let sign = if s.starts_with('-') { -1 } else { 1 };
                    let h: i64 = s[1..3].parse().ok()?;
                    let m: i64 = s[3..5].parse().ok()?;
                    Some(sign * (h * 3600 + m * 60))
                } else {
                    None
                }
            })
            .unwrap_or(0)
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

    /// Custom dictation hotkey: behaves exactly like the Fn key (hold = PTT,
    /// tap = toggle) by feeding the same state-machine triggers.
    pub fn dictation_key_down() {
        let target = crate::appctx::frontmost_bundle_id();
        *TARGET_APP.get_or_init(|| Mutex::new(None)).lock().unwrap() = target;
        snapshot_window_ctx();
        handle_input(Input::Trigger(TriggerToken::Down {
            binding: BindingId::PushToTalk,
            at_ms: now_ms(),
        }));
    }

    pub fn dictation_key_up() {
        handle_input(Input::Trigger(TriggerToken::Up {
            binding: BindingId::PushToTalk,
            at_ms: now_ms(),
        }));
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
        load_asr();

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

        check_for_update(
            APP.get()
                .map(|a| a.package_info().version.to_string())
                .unwrap_or_default(),
        );
        spawn_weekly_review();

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
    trigger_stop, update_settings, voice_profile,
};
#[cfg(target_os = "macos")]
pub use imp::{
    ask_mode_down, ask_mode_up, check_and_install_update, command_mode_down, command_mode_up,
    cleanup_text_on_demand, dictation_key_down, dictation_key_up, download_model, file_transcripts,
    import_contacts, model_status, snippet_suggestions, transcribe_audio_file,
};
#[cfg(not(target_os = "macos"))]
pub fn dictation_key_down() {}
#[cfg(not(target_os = "macos"))]
pub fn dictation_key_up() {}
#[cfg(not(target_os = "macos"))]
pub fn command_mode_down() {}
#[cfg(not(target_os = "macos"))]
pub fn command_mode_up() {}
#[cfg(not(target_os = "macos"))]
pub fn ask_mode_down() {}
#[cfg(not(target_os = "macos"))]
pub fn ask_mode_up() {}
#[cfg(not(target_os = "macos"))]
pub fn import_contacts() -> Result<u32, String> {
    Ok(0)
}
#[cfg(not(target_os = "macos"))]
pub fn snippet_suggestions() -> Vec<serde_json::Value> {
    Vec::new()
}
#[cfg(not(target_os = "macos"))]
pub fn transcribe_audio_file(_path: &str) -> Result<String, String> {
    Err("macOS only".to_string())
}
#[cfg(not(target_os = "macos"))]
pub fn file_transcripts() -> Vec<serde_json::Value> {
    Vec::new()
}
#[cfg(not(target_os = "macos"))]
pub fn check_and_install_update(_current: String) {}
#[cfg(target_os = "macos")]
pub use imp::assistant_chat;
#[cfg(not(target_os = "macos"))]
pub fn assistant_chat(_history: Vec<(String, String)>) -> serde_json::Value {
    serde_json::json!({"reply": "Assistant is macOS-only for now.", "actions_done": []})
}

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
#[cfg(not(target_os = "macos"))]
pub fn voice_profile(_force: bool) -> serde_json::Value {
    serde_json::json!({})
}
#[cfg(not(target_os = "macos"))]
pub fn cleanup_text_on_demand(text: &str) -> String {
    text.to_string()
}
#[cfg(not(target_os = "macos"))]
pub fn model_status() -> (bool, bool) {
    (true, true)
}
#[cfg(not(target_os = "macos"))]
pub fn download_model(_kind: &str) -> Result<(), String> {
    Err("model downloads are macOS-only right now".into())
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
        whimpr_core::StatsStore::default().summary(tz_offset_minutes, 0, 45)
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
