//! Frontmost-app detection: which app is the paste target right now, so the
//! cleanup layer can format the output for the medium (email vs. text vs. chat).
//!
//! On macOS this reads `NSWorkspace.frontmostApplication` — the identity of the
//! focused app is public information and needs **no** extra TCC permission
//! (unlike reading the window's text, which would need Accessibility). The pill
//! overlay is non-activating, so the app the user is dictating into stays
//! frontmost; we capture its bundle id at record-start (Fn down).

/// Bundle id of the frontmost application — the paste target — e.g.
/// `com.apple.mail`. Returns `None` when it can't be determined or when
/// WhimprFlow itself is frontmost (so we don't format for our own Hub window).
#[cfg(target_os = "macos")]
#[allow(unused_unsafe)]
pub fn frontmost_bundle_id() -> Option<String> {
    use objc2_app_kit::NSWorkspace;
    // NSWorkspace reads are thread-safe; safe to call from the tap thread.
    let bid = unsafe {
        let ws = NSWorkspace::sharedWorkspace();
        let app = ws.frontmostApplication()?;
        app.bundleIdentifier()?
    };
    let bid = bid.to_string();
    if bid == "com.whimpr.whimprflow" {
        None
    } else {
        Some(bid)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn frontmost_bundle_id() -> Option<String> {
    None
}

/// Read up to `max_chars` (tail) of the focused UI element's text via the
/// Accessibility API — the text around the caret in the app being dictated
/// into. Needs the Accessibility permission the app already has. Returns None
/// for non-text elements, secure fields, or apps that don't expose AX values.
#[cfg(target_os = "macos")]
pub fn focused_text_context(max_chars: usize) -> Option<String> {
    use std::ffi::c_void;
    use std::os::raw::c_char;

    type AXUIElementRef = *const c_void;
    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFAllocatorRef = *const c_void;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> i32;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithCString(
            alloc: CFAllocatorRef,
            c_str: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFStringGetLength(s: CFStringRef) -> isize;
        fn CFStringGetCString(
            s: CFStringRef,
            buffer: *mut c_char,
            buffer_size: isize,
            encoding: u32,
        ) -> bool;
        fn CFRelease(cf: CFTypeRef);
        fn CFGetTypeID(cf: CFTypeRef) -> usize;
        fn CFStringGetTypeID() -> usize;
    }
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

    unsafe fn cfstr(s: &str) -> CFStringRef {
        let c = std::ffi::CString::new(s).unwrap();
        unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), K_CF_STRING_ENCODING_UTF8) }
    }

    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return None;
        }
        let attr_focused = cfstr("AXFocusedUIElement");
        let mut focused: CFTypeRef = std::ptr::null();
        let err = AXUIElementCopyAttributeValue(system, attr_focused, &mut focused);
        CFRelease(attr_focused);
        CFRelease(system);
        if err != 0 || focused.is_null() {
            return None;
        }

        let attr_value = cfstr("AXValue");
        let mut value: CFTypeRef = std::ptr::null();
        let err = AXUIElementCopyAttributeValue(focused as AXUIElementRef, attr_value, &mut value);
        CFRelease(attr_value);
        CFRelease(focused);
        if err != 0 || value.is_null() {
            return None;
        }
        if CFGetTypeID(value) != CFStringGetTypeID() {
            CFRelease(value);
            return None;
        }

        let len = CFStringGetLength(value);
        // Generous UTF-8 buffer (4 bytes per UTF-16 unit + NUL).
        let cap = (len as usize) * 4 + 1;
        let mut buf = vec![0u8; cap.min(1_000_000)];
        let ok = CFStringGetCString(value, buf.as_mut_ptr() as *mut c_char, buf.len() as isize, K_CF_STRING_ENCODING_UTF8);
        CFRelease(value);
        if !ok {
            return None;
        }
        let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        let s = String::from_utf8_lossy(&buf[..nul]).into_owned();
        let t = s.trim();
        if t.is_empty() {
            return None;
        }
        // Tail: the caret is almost always at the end while dictating.
        let tail: String = t
            .chars()
            .rev()
            .take(max_chars)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        Some(tail)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn focused_text_context(_max_chars: usize) -> Option<String> {
    None
}

/// Whether this process holds a working Accessibility grant (AX API usable).
#[cfg(target_os = "macos")]
pub fn ax_trusted() -> bool {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    unsafe { AXIsProcessTrusted() }
}

#[cfg(not(target_os = "macos"))]
pub fn ax_trusted() -> bool {
    true
}

/// Is the focused UI element a text field (has an AXValue), even an empty one?
/// Used to detect "paste would go nowhere" before typing the dictation.
pub fn has_text_focus() -> bool {
    has_text_focus_diag().0
}

/// `has_text_focus` plus the raw AX error codes (focused-element lookup, value
/// lookup) so failures are diagnosable: -25204 = AX API disabled (stale grant),
/// -25212 = attribute unsupported, 0 = ok.
#[cfg(target_os = "macos")]
pub fn has_text_focus_diag() -> (bool, i32, i32) {
    use std::ffi::c_void;
    use std::os::raw::c_char;

    type AXUIElementRef = *const c_void;
    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFAllocatorRef = *const c_void;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> i32;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithCString(
            alloc: CFAllocatorRef,
            c_str: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFRelease(cf: CFTypeRef);
        fn CFGetTypeID(cf: CFTypeRef) -> usize;
        fn CFStringGetTypeID() -> usize;
    }
    const UTF8: u32 = 0x0800_0100;

    unsafe {
        let mk = |s: &str| {
            let c = std::ffi::CString::new(s).unwrap();
            CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), UTF8)
        };
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return (false, -1, 0);
        }
        let attr_focused = mk("AXFocusedUIElement");
        let mut focused: CFTypeRef = std::ptr::null();
        let err1 = AXUIElementCopyAttributeValue(system, attr_focused, &mut focused);
        CFRelease(attr_focused);
        CFRelease(system);
        if err1 != 0 || focused.is_null() {
            return (false, err1, 0);
        }
        let attr_value = mk("AXValue");
        let mut value: CFTypeRef = std::ptr::null();
        let err2 = AXUIElementCopyAttributeValue(focused as AXUIElementRef, attr_value, &mut value);
        CFRelease(attr_value);
        CFRelease(focused);
        if err2 != 0 || value.is_null() {
            return (false, err1, err2);
        }
        let is_string = CFGetTypeID(value) == CFStringGetTypeID();
        CFRelease(value);
        (is_string, 0, 0)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn has_text_focus_diag() -> (bool, i32, i32) {
    (true, 0, 0)
}

/// What the pre-paste focus probe could find out about the target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusProbe {
    /// A text field is focused — paste and verify the landing.
    Text,
    /// AX answered and the focused element is not editable text — don't paste.
    NoText,
    /// AX gave no usable answer (e.g. an Electron app with its accessibility
    /// tree still asleep) — paste blind like the pre-verify builds did.
    Unknown,
}

/// Electron/Chromium apps keep their accessibility tree disabled until an
/// assistive client wakes them. Setting `AXManualAccessibility` on the app
/// element does exactly that (and unlike `AXEnhancedUserInterface` it does not
/// cause window-resize glitches).
#[cfg(target_os = "macos")]
pub fn wake_ax_for_frontmost() {
    use objc2_app_kit::NSWorkspace;
    use std::ffi::c_void;
    use std::os::raw::c_char;

    type AXUIElementRef = *const c_void;
    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFAllocatorRef = *const c_void;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        fn AXUIElementSetAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: CFTypeRef,
        ) -> i32;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithCString(
            alloc: CFAllocatorRef,
            c_str: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFRelease(cf: CFTypeRef);
        static kCFBooleanTrue: CFTypeRef;
    }
    const UTF8: u32 = 0x0800_0100;

    let pid = unsafe {
        let ws = NSWorkspace::sharedWorkspace();
        match ws.frontmostApplication() {
            Some(app) => app.processIdentifier(),
            None => return,
        }
    };
    unsafe {
        let app = AXUIElementCreateApplication(pid);
        if app.is_null() {
            return;
        }
        let c = std::ffi::CString::new("AXManualAccessibility").unwrap();
        let attr = CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), UTF8);
        let _ = AXUIElementSetAttributeValue(app, attr, kCFBooleanTrue);
        CFRelease(attr);
        CFRelease(app);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn wake_ax_for_frontmost() {}

/// Probe the focused element ahead of a paste, waking sleeping Electron
/// accessibility once if the first attempt yields nothing. Returns the probe
/// verdict plus the raw AX error codes of the last attempt (for the debug log).
pub fn probe_text_focus() -> (FocusProbe, i32, i32) {
    // e1 != 0: the focused-element lookup itself failed — the tree is asleep or
    // unreadable, so we know nothing. e1 == 0: AX answered; a failing AXValue
    // lookup (e2) then just means "focused thing is not a text field" (button,
    // toolbar), which is a real "don't paste".
    let verdict = |ok: bool, e1: i32, e2: i32| {
        if e1 != 0 {
            FocusProbe::Unknown
        } else if ok {
            FocusProbe::Text
        } else {
            let _ = e2;
            FocusProbe::NoText
        }
    };
    let (ok, e1, e2) = has_text_focus_diag();
    if e1 == 0 {
        return (verdict(ok, e1, e2), e1, e2);
    }
    // Lookup failed — wake the app's tree (Electron) and give it a beat.
    wake_ax_for_frontmost();
    std::thread::sleep(std::time::Duration::from_millis(120));
    let (ok, e1, e2) = has_text_focus_diag();
    (verdict(ok, e1, e2), e1, e2)
}
