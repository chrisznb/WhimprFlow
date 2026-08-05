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
