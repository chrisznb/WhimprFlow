//! WhimprFlow Tauri shell.
//!
//! Runs as a macOS accessory (menu-bar) app: a tray item, a transparent
//! always-on-top Flow Bar overlay, and a hidden Hub window. This is the M0
//! skeleton — the sidecar supervisor, real state-machine bridge, and native
//! panel promotion arrive in later milestones. The overlay already listens for
//! `whimpr://flowbar/state`, so the tray demo items prove the event pipeline.

mod appctx;
mod autolearn;
mod hotkey;
mod local_llm;
mod paste;
#[cfg(target_os = "windows")]
mod win;

use std::sync::Mutex;

use serde::Serialize;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

const OVERLAY_LABEL: &str = "whimpr_bar";
const HUB_LABEL: &str = "main";

#[derive(Clone, Serialize)]
struct BarStatePayload {
    state: &'static str,
}

/// Anchor the overlay window bottom-center of its monitor.
/// User-chosen pill anchor (physical px): center-x + bottom-y. Set by dragging
/// the pill; persisted to `overlay-pos.json` in app support.
static OVERLAY_ANCHOR: Mutex<Option<(f64, f64)>> = Mutex::new(None);
/// Pill orientation: vertical when snapped to the left/right edge midpoints.
static OVERLAY_VERTICAL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn overlay_vertical() -> bool {
    OVERLAY_VERTICAL.load(std::sync::atomic::Ordering::Relaxed)
}

fn anchor_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::PathBuf::from(home).join("Library/Application Support/WhimprFlow/overlay-pos.json")
}

pub(crate) fn load_overlay_anchor() {
    if let Ok(s) = std::fs::read_to_string(anchor_path()) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
            if let (Some(cx), Some(by)) = (
                v.get("center_x").and_then(|x| x.as_f64()),
                v.get("bottom_y").and_then(|y| y.as_f64()),
            ) {
                *OVERLAY_ANCHOR.lock().unwrap() = Some((cx, by));
            }
            let vertical = v.get("vertical").and_then(|b| b.as_bool()).unwrap_or(false);
            OVERLAY_VERTICAL.store(vertical, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

/// Move the overlay window during a pill drag (physical px), clamped to the
/// monitor so the pill can never be dragged off-screen.
pub(crate) fn overlay_drag_move(app: &tauri::AppHandle, x: f64, y: f64) {
    if let Some(w) = app.get_webview_window(OVERLAY_LABEL) {
        let (mut xi, mut yi) = (x as i32, y as i32);
        if let (Ok(Some(m)), Ok(size)) = (w.current_monitor(), w.outer_size()) {
            let mp = m.position();
            let ms = m.size();
            xi = xi.clamp(mp.x, mp.x + ms.width as i32 - size.width as i32);
            yi = yi.clamp(mp.y, mp.y + ms.height as i32 - size.height as i32);
        }
        let _ = w.set_position(tauri::PhysicalPosition { x: xi, y: yi });
    }
}

/// Drag finished: snap to the nearest magnet position (edge midpoints and
/// corners of the work area) and persist that anchor (center-x, bottom-y).
pub(crate) fn overlay_drag_end(app: &tauri::AppHandle) {
    let Some(w) = app.get_webview_window(OVERLAY_LABEL) else {
        return;
    };
    let (Ok(pos), Ok(size)) = (w.outer_position(), w.outer_size()) else {
        return;
    };
    let current = (
        pos.x as f64 + size.width as f64 / 2.0,
        pos.y as f64 + size.height as f64,
    );

    let monitor = w
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| w.primary_monitor().ok().flatten());
    let anchor = if let Some(m) = monitor {
        let wa = m.work_area();
        let scale = m.scale_factor();
        let inset = 12.0 * scale;
        let (wx, wy) = (wa.position.x as f64, wa.position.y as f64);
        let (ww, wh) = (wa.size.width as f64, wa.size.height as f64);
        let (pw, ph) = (size.width as f64, size.height as f64);
        // Anchor candidates as (center-x, bottom-y).
        let xs = [wx + inset + pw / 2.0, wx + ww / 2.0, wx + ww - inset - pw / 2.0];
        let ys = [wy + inset + ph, wy + wh / 2.0 + ph / 2.0, wy + wh - inset];
        let mut best = current;
        let mut best_d = f64::MAX;
        let mut best_vertical = false;
        for (xi, &cx) in xs.iter().enumerate() {
            for (yi, &by) in ys.iter().enumerate() {
                // Skip dead center of the screen — not a useful pill spot.
                if xi == 1 && yi == 1 {
                    continue;
                }
                let d = (cx - current.0).powi(2) + (by - current.1).powi(2);
                if d < best_d {
                    best_d = d;
                    best = (cx, by);
                    // Left/right edge midpoints flip the pill vertical.
                    best_vertical = (xi == 0 || xi == 2) && yi == 1;
                }
            }
        }
        OVERLAY_VERTICAL.store(best_vertical, std::sync::atomic::Ordering::Relaxed);
        best
    } else {
        current
    };

    *OVERLAY_ANCHOR.lock().unwrap() = Some(anchor);
    let vertical = overlay_vertical();
    let json = format!(
        "{{\n  \"center_x\": {},\n  \"bottom_y\": {},\n  \"vertical\": {}\n}}\n",
        anchor.0, anchor.1, vertical
    );
    let _ = std::fs::write(anchor_path(), json);
    // Snap the window to the chosen magnet with the right orientation size.
    let (lw, lh) = if vertical { (44.0, 76.0) } else { (76.0, 44.0) };
    let _ = w.set_size(tauri::LogicalSize::new(lw, lh));
    // Tell the pill about its orientation right away (state stays untouched).
    let _ = w.emit("whimpr://flowbar/orient", vertical);
    position_overlay(&w);
    eprintln!(
        "[whimpr] pill anchor snapped: ({:.0},{:.0}) vertical={vertical}",
        anchor.0, anchor.1
    );
}

pub(crate) fn position_overlay(w: &WebviewWindow) {
    // current_monitor() can be None before the window maps; fall back sensibly.
    let monitor = w
        .primary_monitor()
        .ok()
        .flatten()
        .or_else(|| w.current_monitor().ok().flatten())
        .or_else(|| w.available_monitors().ok().and_then(|m| m.into_iter().next()));
    let Some(monitor) = monitor else {
        eprintln!("[whimpr] no monitor found — overlay stays at default position");
        return;
    };
    let scale = monitor.scale_factor();
    let msize = monitor.size();
    let mpos = monitor.position();
    // work_area excludes the Dock and menu bar, so the pill sits above the Dock
    // instead of colliding with it.
    let wa = monitor.work_area();
    let Ok(wsize) = w.outer_size() else { return };
    let inset = (12.0 * scale) as i32;
    // A dragged anchor wins over the default bottom-center placement. The
    // anchor is the pill's center-x/bottom-y, so state-change resizes keep the
    // pill growing around the same spot.
    let (mut x, mut y) = if let Some((cx, by)) = *OVERLAY_ANCHOR.lock().unwrap() {
        (
            (cx - wsize.width as f64 / 2.0) as i32,
            (by - wsize.height as f64) as i32,
        )
    } else {
        (
            wa.position.x + (wa.size.width as i32 - wsize.width as i32) / 2,
            wa.position.y + wa.size.height as i32 - wsize.height as i32 - inset,
        )
    };
    // Keep the window on the monitor.
    x = x.clamp(mpos.x, mpos.x + msize.width as i32 - wsize.width as i32);
    y = y.clamp(mpos.y, mpos.y + msize.height as i32 - wsize.height as i32);
    let _ = w.set_position(tauri::PhysicalPosition { x, y });
    eprintln!(
        "[whimpr] overlay placed: monitor {}x{} @({},{}) scale {:.1} -> window {}x{} @({},{})",
        msize.width, msize.height, mpos.x, mpos.y, scale, wsize.width, wsize.height, x, y
    );
}

/// WKWebView hover events are unreliable in a borderless, non-activating panel,
/// so hover is detected natively: poll the global mouse location and tell the
/// overlay when the cursor is over it. The webview listens for this event and
/// morphs the idle nub into the mic button.
#[cfg(target_os = "macos")]
fn spawn_hover_watcher(overlay: WebviewWindow) {
    #[repr(C)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    unsafe extern "C" {
        fn CGEventCreate(source: *const std::ffi::c_void) -> *mut std::ffi::c_void;
        fn CGEventGetLocation(event: *mut std::ffi::c_void) -> CGPoint;
        fn CFRelease(cf: *mut std::ffi::c_void);
    }
    std::thread::spawn(move || {
        let mut last = false;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(120));
            let inside = (|| -> Option<bool> {
                let ev = unsafe { CGEventCreate(std::ptr::null()) };
                if ev.is_null() {
                    return None;
                }
                let loc = unsafe { CGEventGetLocation(ev) };
                unsafe { CFRelease(ev) };
                let pos = overlay.outer_position().ok()?;
                let size = overlay.outer_size().ok()?;
                let scale = overlay.scale_factor().ok()?;
                // Publish the rect for the event tap's click routing.
                hotkey::set_overlay_hit(hotkey::OverlayHit {
                    x: pos.x as f64,
                    y: pos.y as f64,
                    w: size.width as f64,
                    h: size.height as f64,
                    scale,
                });
                // CGEventGetLocation is in logical points (top-left origin);
                // tauri positions are physical pixels.
                let (mx, my) = (loc.x * scale, loc.y * scale);
                // Small tolerance so the zone reaches slightly past the pill.
                let pad = 8.0 * scale;
                let hit = mx >= pos.x as f64 - pad
                    && mx <= pos.x as f64 + size.width as f64 + pad
                    && my >= pos.y as f64 - pad
                    && my <= pos.y as f64 + size.height as f64 + pad;
                if std::env::var_os("WHIMPR_HOVER_DEBUG").is_some() {
                    eprintln!(
                        "[whimpr] hover-dbg mouse=({mx:.0},{my:.0}) win=({},{} {}x{}) hit={hit}",
                        pos.x, pos.y, size.width, size.height
                    );
                }
                Some(hit)
            })()
            .unwrap_or(false);
            if inside != last {
                last = inside;
                eprintln!("[whimpr] hover -> {inside}");
                let _ = overlay.emit("whimpr://flowbar/hover", inside);
            }
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn spawn_hover_watcher(_overlay: WebviewWindow) {}

fn build_overlay(app: &tauri::App) -> tauri::Result<WebviewWindow> {
    let overlay = WebviewWindowBuilder::new(
        app,
        OVERLAY_LABEL,
        WebviewUrl::App("overlay.html".into()),
    )
    .title("WhimprBar")
    // Tight window so it only catches clicks right around the pill, not a big
    // invisible box over the app behind it.
    .inner_size(76.0, 44.0)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .resizable(false)
    .visible(true)
    .build()?;
    // `always_on_top` is only NSFloatingWindowLevel (3) — fullscreen apps, video
    // players, and some panels still cover the pill. Raise it to status-bar level
    // and let it follow the user across Spaces and fullscreen apps.
    #[cfg(target_os = "macos")]
    if let Ok(ptr) = overlay.ns_window() {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        // NSStatusWindowLevel = 25; canJoinAllSpaces (1) | stationary (16) |
        // ignoresCycle (64) | fullScreenAuxiliary (256) = 337.
        let win = ptr as *mut AnyObject;
        unsafe {
            let _: () = msg_send![win, setLevel: 25isize];
            let _: () = msg_send![win, setCollectionBehavior: 337usize];
            // NSWindow drops mouse-moved events by default — without them the
            // webview never sees hover, so the idle nub can't morph into the
            // mic button.
            let _: () = msg_send![win, setAcceptsMouseMovedEvents: true];
            // Never steal focus from the app the user is dictating into: clicks
            // on the pill (mic/stop/cancel) must leave the frontmost app active,
            // or the paste target snapshot would point at WhimprFlow itself.
            let responds: bool = msg_send![
                win,
                respondsToSelector: objc2::sel!(_setPreventsActivation:)
            ];
            if responds {
                let _: () = msg_send![win, _setPreventsActivation: true];
            }
        }
    }
    // The window is purely visual: clicks over the pill are handled (and
    // swallowed) by the global event tap, so the window itself never takes
    // mouse input — no first-click activation, no Hub popping up.
    let _ = overlay.set_ignore_cursor_events(true);
    if overlay_vertical() {
        let _ = overlay.set_size(tauri::LogicalSize::new(44.0, 76.0));
    }
    position_overlay(&overlay);
    let _ = overlay.show();
    Ok(overlay)
}

fn build_hub(app: &tauri::App) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(app, HUB_LABEL, WebviewUrl::App("index.html".into()))
        .title("WhimprFlow")
        .inner_size(920.0, 640.0)
        .min_inner_size(720.0, 480.0)
        .visible(true)
        .build()
}

fn emit_bar_state(app: &tauri::AppHandle, state: &'static str) {
    let _ = app.emit_to(OVERLAY_LABEL, "whimpr://flowbar/state", BarStatePayload { state });
}

#[tauri::command]
fn get_settings() -> whimpr_core::Settings {
    hotkey::current_settings()
}

#[tauri::command]
async fn transcribe_file(path: String) -> Result<String, String> {
    // Heavy work off the main thread — the UI stays responsive (no beachball).
    tauri::async_runtime::spawn_blocking(move || hotkey::transcribe_audio_file(&path))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn choose_audio_file() -> Option<String> {
    tauri::async_runtime::spawn_blocking(choose_audio_file_blocking)
        .await
        .ok()
        .flatten()
}

fn choose_audio_file_blocking() -> Option<String> {
    let out = std::process::Command::new("osascript")
        .arg("-e")
        .arg(r#"POSIX path of (choose file with prompt "Choose an audio file to transcribe")"#)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if p.is_empty() { None } else { Some(p) }
}

#[tauri::command]
fn get_file_transcripts() -> Vec<serde_json::Value> {
    hotkey::file_transcripts()
}

#[tauri::command]
async fn import_contacts() -> Result<u32, String> {
    tauri::async_runtime::spawn_blocking(hotkey::import_contacts)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_snippet_suggestions() -> Vec<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(hotkey::snippet_suggestions)
        .await
        .unwrap_or_default()
}

#[tauri::command]
async fn assistant_chat(history: Vec<(String, String)>) -> serde_json::Value {
    tauri::async_runtime::spawn_blocking(move || hotkey::assistant_chat(history))
        .await
        .unwrap_or_else(|e| serde_json::json!({"reply": e.to_string(), "actions_done": []}))
}

#[tauri::command]
async fn get_voice_profile(force: bool) -> serde_json::Value {
    tauri::async_runtime::spawn_blocking(move || hotkey::voice_profile(force))
        .await
        .unwrap_or_else(|_| serde_json::json!({}))
}

#[tauri::command]
fn get_orientation() -> bool {
    overlay_vertical()
}

#[tauri::command]
fn get_snippets() -> Vec<whimpr_core::Snippet> {
    hotkey::snippets_entries()
}

#[tauri::command]
fn add_snippet(trigger: String, replacement: String) {
    hotkey::snippets_add(trigger, replacement);
}

#[tauri::command]
fn remove_snippet(trigger: String) {
    hotkey::snippets_remove(&trigger);
}

#[tauri::command]
fn get_scratchpad() -> String {
    hotkey::scratchpad_get()
}

#[tauri::command]
fn set_scratchpad(text: String) {
    hotkey::scratchpad_set(&text);
}

#[tauri::command]
fn get_transforms() -> Vec<hotkey::Transform> {
    hotkey::transforms_list()
}

#[tauri::command]
fn set_transforms(app: tauri::AppHandle, list: Vec<hotkey::Transform>) {
    hotkey::transforms_save(list);
    register_transform_shortcuts(&app);
}

/// (Re-)register the global transform shortcuts from the saved list, plus the
/// hold-to-speak modes: Alt+Space = spoken command on the selection, Alt+W =
/// ask anywhere (answer pasted at the cursor).
fn register_transform_shortcuts(app: &tauri::AppHandle) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let _ = gs.on_shortcut("Alt+Space", |_app, _sc, event| match event.state {
        tauri_plugin_global_shortcut::ShortcutState::Pressed => hotkey::command_mode_down(),
        tauri_plugin_global_shortcut::ShortcutState::Released => hotkey::command_mode_up(),
    });
    let _ = gs.on_shortcut("Alt+W", |_app, _sc, event| match event.state {
        tauri_plugin_global_shortcut::ShortcutState::Pressed => hotkey::ask_mode_down(),
        tauri_plugin_global_shortcut::ShortcutState::Released => hotkey::ask_mode_up(),
    });
    // User-chosen extra dictation key (Fn always stays active).
    let custom = hotkey::current_settings().dictation_hotkey.trim().to_string();
    if !custom.is_empty() {
        if let Err(e) = gs.on_shortcut(custom.as_str(), |_app, _sc, event| match event.state {
            tauri_plugin_global_shortcut::ShortcutState::Pressed => hotkey::dictation_key_down(),
            tauri_plugin_global_shortcut::ShortcutState::Released => hotkey::dictation_key_up(),
        }) {
            eprintln!("[whimpr] dictation hotkey '{custom}' failed to register: {e}");
        } else {
            eprintln!("[whimpr] dictation hotkey '{custom}' active");
        }
    }
    for t in hotkey::transforms_list() {
        let accel = t.shortcut.clone();
        let accel_cb = accel.clone();
        if let Err(e) = gs.on_shortcut(accel.as_str(), move |_app, _sc, event| {
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                hotkey::run_transform(&accel_cb);
            }
        }) {
            eprintln!("[whimpr] shortcut '{}' failed to register: {e}", t.shortcut);
        }
    }
}

/// UI mic button: start a hands-free dictation (same path as a quick Fn tap).
#[tauri::command]
fn start_dictation() {
    hotkey::trigger_hands_free();
}

/// Pill stop button: finalize the running session.
#[tauri::command]
fn stop_dictation() {
    hotkey::trigger_stop();
}

/// Pill cancel button: discard the running session.
#[tauri::command]
fn cancel_dictation() {
    hotkey::trigger_cancel();
}

#[tauri::command]
fn set_settings(app: tauri::AppHandle, settings: whimpr_core::Settings) {
    hotkey::update_settings(settings);
    // Hotkeys may have changed.
    register_transform_shortcuts(&app);
}

/// Aggregated dictation stats for the Hub dashboard. `tz_offset_minutes` is the
/// browser's `Date.getTimezoneOffset()` so "today"/streak match the user's clock.
#[tauri::command]
fn get_stats(tz_offset_minutes: i32) -> whimpr_core::StatsSummary {
    hotkey::stats_summary(tz_offset_minutes)
}

/// Recent dictations for the Hub Home history list (newest first).
#[tauri::command]
fn get_history() -> Vec<whimpr_core::HistoryItem> {
    hotkey::history(200)
}

/// Dictionary entries for the Hub Dictionary screen.
#[tauri::command]
fn get_dictionary() -> Vec<hotkey::DictEntryDto> {
    hotkey::dictionary_entries()
}

/// Add a manual dictionary entry (word + optional known mishears).
#[tauri::command]
fn add_dictionary_entry(correct: String, mishears: Vec<String>) {
    hotkey::dictionary_add(correct, mishears);
}

/// Remove a dictionary entry by its spelling.
#[tauri::command]
fn remove_dictionary_entry(correct: String) {
    hotkey::dictionary_remove(&correct);
}

/// Permission + capability status shown in the Hub.
#[derive(Clone, Serialize)]
struct StatusReport {
    accessibility: bool,
    microphone: bool,
    input_monitoring: bool,
    has_openai_key: bool,
    has_anthropic_key: bool,
}

#[tauri::command]
fn get_status() -> StatusReport {
    StatusReport {
        accessibility: paste::is_trusted(),
        microphone: paste::microphone_granted(),
        input_monitoring: paste::input_monitoring_granted(),
        has_openai_key: has_key("openai_api_key"),
        has_anthropic_key: has_key("anthropic_api_key"),
    }
}

fn has_key(account: &str) -> bool {
    keyring::Entry::new("com.whimpr.whimprflow", account)
        .ok()
        .and_then(|e| e.get_password().ok())
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn open_url(url: &str) {
    let _ = std::process::Command::new("open").arg(url).spawn();
}

/// Request microphone access: trigger the native prompt (bundle has a usage string)
/// by briefly opening the input device, and open the Microphone settings pane.
#[tauri::command]
fn request_microphone() {
    #[cfg(target_os = "macos")]
    {
        std::thread::spawn(|| {
            if let Ok(h) = whimpr_audio::start(|_: &[f32]| {}) {
                std::thread::sleep(std::time::Duration::from_millis(400));
                let _ = h.stop();
            }
        });
        open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone");
    }
}

/// Request Accessibility — the permission that makes the Fn key work in every app and
/// lets us type into other apps. Fire the native prompt, then open the pane.
#[tauri::command]
fn request_accessibility() {
    #[cfg(target_os = "macos")]
    {
        let _ = paste::prompt_accessibility();
        open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility");
    }
}

/// Request Input Monitoring (needed for the Fn key to be seen in every app, not
/// just while WhimprFlow is frontmost): register + prompt, then open the pane.
#[tauri::command]
fn request_input_monitoring() {
    #[cfg(target_os = "macos")]
    {
        let _ = paste::request_input_monitoring();
        open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent");
    }
}

/// Save (or clear, when empty) an API key in the OS keychain, then rebuild providers
/// so it takes effect immediately.
#[tauri::command]
fn set_api_key(provider: String, key: String) -> Result<(), String> {
    let account = match provider.as_str() {
        "openai" => "openai_api_key",
        "anthropic" => "anthropic_api_key",
        _ => return Err(format!("unknown provider {provider}")),
    };
    let key = key.trim();
    // The macOS data-protection keychain (used by the keyring crate) needs an
    // entitlement our re-signed build doesn't carry, so writes fail silently.
    // The `security` CLI writes to the classic login keychain instead, and -T
    // grants this binary prompt-free read access.
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("/usr/bin/security")
            .args(["delete-generic-password", "-s", "com.whimpr.whimprflow", "-a", account])
            .output();
        if !key.is_empty() {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let out = std::process::Command::new("/usr/bin/security")
                .arg("add-generic-password")
                .args(["-U", "-s", "com.whimpr.whimprflow", "-a", account, "-w", key])
                .arg("-T")
                .arg(&exe)
                .args(["-T", "/usr/bin/security"])
                .output()
                .map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let entry =
            keyring::Entry::new("com.whimpr.whimprflow", account).map_err(|e| e.to_string())?;
        let _ = entry.delete_credential();
        if !key.is_empty() {
            entry.set_password(key).map_err(|e| e.to_string())?;
        }
    }
    hotkey::rebuild_providers();
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            // LaunchAgent: plain plist in ~/Library/LaunchAgents — no Automation
            // permission needed (AppleScript login items would require one).
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_settings,
            set_settings,
            get_orientation,
            get_voice_profile,
            assistant_chat,
            import_contacts,
            get_snippet_suggestions,
            transcribe_file,
            choose_audio_file,
            get_file_transcripts,
            get_snippets,
            add_snippet,
            remove_snippet,
            get_scratchpad,
            set_scratchpad,
            get_transforms,
            set_transforms,
            start_dictation,
            stop_dictation,
            cancel_dictation,
            get_stats,
            get_history,
            get_dictionary,
            add_dictionary_entry,
            remove_dictionary_entry,
            get_status,
            request_microphone,
            request_accessibility,
            request_input_monitoring,
            set_api_key
        ])
        .setup(|app| {
            // Menu-bar-only accessory app: no Dock icon, lives in the tray.
            // The Hub window still opens via the tray's "Open WhimprFlow".
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Start at login (visible as a normal login item in System Settings).
            {
                use tauri_plugin_autostart::ManagerExt;
                let al = app.autolaunch();
                if !al.is_enabled().unwrap_or(false) {
                    if let Err(e) = al.enable() {
                        eprintln!("[whimpr] autostart enable failed: {e}");
                    } else {
                        eprintln!("[whimpr] autostart at login enabled");
                    }
                }
            }

            load_overlay_anchor();
            let overlay = build_overlay(app)?;
            spawn_hover_watcher(overlay.clone());
            let hub = build_hub(app)?;
            let _ = hub.show();
            let _ = hub.set_focus();

            // Wire the Fn key to the pill via the real state machine.
            hotkey::install(app.handle().clone());

            // Global transform shortcuts (Alt+1/2/3 by default).
            register_transform_shortcuts(app.handle());

            let open = MenuItem::with_id(app, "open", "Open WhimprFlow", true, None::<&str>)?;
            let copy_last =
                MenuItem::with_id(app, "copy_last", "Copy last dictation", true, None::<&str>)?;
            let demo_rec =
                MenuItem::with_id(app, "demo_rec", "Demo: recording", true, None::<&str>)?;
            let demo_idle = MenuItem::with_id(app, "demo_idle", "Demo: idle", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "Quit WhimprFlow", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &copy_last, &sep, &quit])?;

            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    // Left click brings the Hub to the front (menu stays on right click).
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(w) = tray.app_handle().get_webview_window(HUB_LABEL) {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        if let Some(w) = app.get_webview_window(HUB_LABEL) {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "copy_last" => {
                        if let Some(text) = hotkey::last_dictation() {
                            if let Ok(mut cb) = arboard::Clipboard::new() {
                                let _ = cb.set_text(text);
                            }
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                });
            // Monochrome template icon: macOS tints it white/black to match the
            // menu bar automatically (instead of the full-color app icon).
            match tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png")) {
                Ok(icon) => {
                    tray = tray.icon(icon).icon_as_template(true);
                }
                Err(_) => {
                    if let Some(icon) = app.default_window_icon().cloned() {
                        tray = tray.icon(icon);
                    }
                }
            }
            tray.build(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running WhimprFlow");
}
