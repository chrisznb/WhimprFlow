//! Auto-learn: after WhimprFlow pastes dictated text, watch the focused text field
//! (polled every 2 s for up to 30 s, the last stable state wins). If the user
//! corrects a distinctive name (1-3 words replaced by 1-3 words, e.g. "Heinz App"
//! -> "HeinsApp"), diff it out and add it to the dictionary - so next time ASR/
//! cleanup spell it right. This is the signal source Wispr's sparkle needs.
//!
//! It is deliberately conservative: it only learns on one contiguous substitution
//! inside the dictated text with the rest of the field unchanged, where the new
//! words look like a proper noun and are phonetically close to what they replaced. That avoids poisoning the
//! dictionary with common-word edits. Reads use the Accessibility API and only run
//! when Accessibility is granted.

#[cfg(target_os = "macos")]
mod imp {
    use std::os::raw::{c_char, c_void};
    use std::ptr;
    use std::time::Duration;

    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    type AXUIElementRef = *const c_void;

    const KCF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    /// Poll interval and overall observation window after a paste.
    const POLL_EVERY: Duration = Duration::from_secs(2);
    const POLL_MAX: Duration = Duration::from_secs(30);

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: CFTypeRef);
        fn CFStringCreateWithCString(
            alloc: CFTypeRef,
            cstr: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFStringGetLength(s: CFStringRef) -> isize;
        fn CFStringGetCString(s: CFStringRef, buf: *mut c_char, size: isize, encoding: u32) -> bool;
        fn CFStringGetMaximumSizeForEncoding(len: isize, encoding: u32) -> isize;
        fn CFGetTypeID(cf: CFTypeRef) -> usize;
        fn CFEqual(a: CFTypeRef, b: CFTypeRef) -> bool;
        fn CFStringGetTypeID() -> usize;
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> i32;
    }

    fn make_cfstring(s: &str) -> CFStringRef {
        let Ok(c) = std::ffi::CString::new(s) else {
            return ptr::null();
        };
        unsafe { CFStringCreateWithCString(ptr::null(), c.as_ptr(), KCF_STRING_ENCODING_UTF8) }
    }

    /// Convert a CFStringRef to a Rust String (None if it isn't actually a string).
    unsafe fn cfstring_to_string(s: CFStringRef) -> Option<String> {
        if s.is_null() || CFGetTypeID(s) != CFStringGetTypeID() {
            return None;
        }
        let len = CFStringGetLength(s);
        let max = CFStringGetMaximumSizeForEncoding(len, KCF_STRING_ENCODING_UTF8) + 1;
        if max <= 0 {
            return Some(String::new());
        }
        let mut buf = vec![0i8; max as usize];
        if CFStringGetCString(s, buf.as_mut_ptr(), max, KCF_STRING_ENCODING_UTF8) {
            std::ffi::CStr::from_ptr(buf.as_ptr())
                .to_str()
                .ok()
                .map(|x| x.to_string())
        } else {
            None
        }
    }

    /// Copy the system-wide focused UI element (retained — caller CFReleases it).
    unsafe fn copy_focused_element() -> AXUIElementRef {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return ptr::null();
        }
        let attr = make_cfstring("AXFocusedUIElement");
        let mut focused: CFTypeRef = ptr::null();
        let err = AXUIElementCopyAttributeValue(system, attr, &mut focused);
        if !attr.is_null() {
            CFRelease(attr);
        }
        CFRelease(system);
        if err != 0 {
            return ptr::null();
        }
        focused as AXUIElementRef
    }

    /// Read a text element's AXValue as a string.
    unsafe fn element_value(element: AXUIElementRef) -> Option<String> {
        if element.is_null() {
            return None;
        }
        let attr = make_cfstring("AXValue");
        let mut value: CFTypeRef = ptr::null();
        let err = AXUIElementCopyAttributeValue(element, attr, &mut value);
        if !attr.is_null() {
            CFRelease(attr);
        }
        if err != 0 || value.is_null() {
            return None;
        }
        let s = cfstring_to_string(value);
        CFRelease(value);
        s
    }

    /// A raw AX pointer we deliberately move to the observer thread. Safe because
    /// CF/AX types are internally thread-safe and we retain it before sending.
    struct SendPtr(AXUIElementRef);
    unsafe impl Send for SendPtr {}

    /// Right after paste, watch the focused field: poll every 2 s for up to 30 s.
    /// The baseline is the first reading that contains the inserted text. The
    /// result is judged on the last stable state: two identical readings after a
    /// change, or the last reading when the field loses focus / disappears.
    pub fn watch_correction(inserted: &str) {
        // Reads require Accessibility; also skip trivial dictations.
        if !crate::paste::is_trusted() || crate::autolearn::word_tokens(inserted).len() < 2 {
            return;
        }
        let inserted = inserted.to_string();
        let focused = unsafe { copy_focused_element() };
        if focused.is_null() {
            return;
        }
        let holder = SendPtr(focused);
        std::thread::spawn(move || {
            // Force whole-struct capture (2021 disjoint captures would otherwise grab
            // the raw pointer field and lose the `Send` impl on `SendPtr`).
            let holder = holder;
            let mut baseline: Option<String> = None;
            let mut last: Option<String> = None;
            let mut waited = Duration::ZERO;
            loop {
                let now = unsafe { element_value(holder.0) };
                let Some(now) = now else { break }; // field is gone
                if baseline.is_none() {
                    if super::contains_tokens(&now, &inserted) {
                        baseline = Some(now.clone());
                    }
                } else if Some(&now) != baseline.as_ref() {
                    let stable = last.as_ref() == Some(&now);
                    last = Some(now);
                    if stable {
                        break; // changed, then unchanged for one full interval
                    }
                } else if last.is_some() {
                    // Went back to the baseline (undo): nothing to learn.
                    last = None;
                }
                // Focus moved elsewhere: judge on what we have.
                let still_focused = unsafe {
                    let cur = copy_focused_element();
                    let same = !cur.is_null() && CFEqual(cur, holder.0);
                    if !cur.is_null() {
                        CFRelease(cur);
                    }
                    same
                };
                if !still_focused || waited >= POLL_MAX {
                    break;
                }
                std::thread::sleep(POLL_EVERY);
                waited += POLL_EVERY;
            }
            unsafe { CFRelease(holder.0) };
            let (Some(before), Some(after)) = (baseline, last) else { return };
            if let Some((mishear, correct)) = super::detect_field_correction(&inserted, &before, &after) {
                eprintln!("[whimpr] ✨ auto-learned: \"{mishear}\" -> \"{correct}\"");
                crate::hotkey::dictionary_learn(correct, vec![mishear]);
            }
        });
    }
}

#[cfg(target_os = "macos")]
pub use imp::watch_correction;

#[cfg(not(target_os = "macos"))]
pub fn watch_correction(_inserted: &str) {}

/// Split into alphanumeric word tokens (punctuation stripped), original case kept.
pub fn word_tokens(s: &str) -> Vec<String> {
    s.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_string()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// Detect a single clean correction of 1-3 words by 1-3 words. Both texts are the
/// dictated text as inserted / as it stands now. Returns `(mishear, correct)`.
pub fn detect_correction(inserted: &str, after: &str) -> Option<(String, String)> {
    detect_field_correction(inserted, inserted, after)
}

/// Same, for a whole text field: `before` is the field right after the paste (it
/// contains `inserted`), `after` its current content. The change must be one
/// contiguous substitution, lying inside the dictated text; everything else in the
/// field is unchanged by construction.
pub fn detect_field_correction(inserted: &str, before: &str, after: &str) -> Option<(String, String)> {
    let ins = word_tokens(inserted);
    let bef = word_tokens(before);
    let aft = word_tokens(after);
    if ins.is_empty() || bef.is_empty() || aft.is_empty() {
        return None;
    }
    let eq = |a: &String, b: &String| a.to_lowercase() == b.to_lowercase();
    // Longest common prefix / suffix; the middle is the substitution.
    let mut p = 0;
    while p < bef.len() && p < aft.len() && eq(&bef[p], &aft[p]) {
        p += 1;
    }
    let mut s = 0;
    while s < bef.len() - p && s < aft.len() - p && eq(&bef[bef.len() - 1 - s], &aft[aft.len() - 1 - s]) {
        s += 1;
    }
    let removed = &bef[p..bef.len() - s];
    let added = &aft[p..aft.len() - s];
    if removed.is_empty() || added.is_empty() || removed.len() > 3 || added.len() > 3 {
        return None;
    }
    // The replaced words must come from the dictated text (a contiguous run).
    let in_ins = ins.windows(removed.len()).any(|w| w.iter().zip(removed).all(|(a, b)| eq(a, b)));
    if !in_ins {
        return None;
    }
    let alpha = |ws: &[String]| ws.iter().all(|w| w.chars().all(|c| c.is_alphabetic()));
    if !alpha(removed) || !alpha(added) {
        return None;
    }
    let mishear = removed.join(" ");
    let correct = added.join(" ");
    let (mj, cj) = (removed.concat(), added.concat());
    if mj.chars().count() < 3 || cj.chars().count() < 3 || mj.to_lowercase() == cj.to_lowercase() {
        return None;
    }
    // Never learn ordinary words, nor a replacement made only of ordinary words.
    if is_common(&cj) || removed.iter().all(|w| is_common(w)) || added.iter().all(|w| is_common(w)) {
        return None;
    }
    // The correction should look like a name (Titlecase) and be phonetically close
    // to what it replaced (a real mishear, not an unrelated rewrite).
    let titled = added[0].chars().next().is_some_and(|c| c.is_uppercase());
    let d = norm_levenshtein(&mj, &cj);
    let sounds_alike = whimpr_core::phonetics::cologne(&mj) == whimpr_core::phonetics::cologne(&cj);
    if titled && (d <= 0.6 || sounds_alike) {
        Some((mishear, correct))
    } else {
        None
    }
}

/// True if the words of `needle` appear contiguously (case-insensitive) in `hay`.
pub fn contains_tokens(hay: &str, needle: &str) -> bool {
    let h: Vec<String> = word_tokens(hay).iter().map(|w| w.to_lowercase()).collect();
    let n: Vec<String> = word_tokens(needle).iter().map(|w| w.to_lowercase()).collect();
    !n.is_empty() && h.windows(n.len()).any(|w| w == n.as_slice())
}

fn is_common(w: &str) -> bool {
    whimpr_core::common_words::is_common_word(w)
}

/// Levenshtein distance normalized by the longer length (0 = identical, 1 = totally
/// different).
fn norm_levenshtein(a: &str, b: &str) -> f32 {
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    let m = a.chars().count().max(b.chars().count());
    if m == 0 {
        return 1.0;
    }
    strsim::levenshtein(&a, &b) as f32 / m as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learns_a_name_correction() {
        // We inserted "monvi"; the user fixed it to "Manvi".
        let got = detect_correction("send the deck to monvi please", "send the deck to Manvi please");
        assert_eq!(got, Some(("monvi".to_string(), "Manvi".to_string())));
    }

    #[test]
    fn ignores_common_word_edits() {
        // "there" -> "their" is a common-word edit, never learned.
        assert_eq!(detect_correction("i left there bag", "i left their bag"), None);
    }

    #[test]
    fn ignores_multi_word_changes() {
        // More than one word changed → too ambiguous, skip.
        assert_eq!(detect_correction("meet at noon monvi", "see you later Manvi"), None);
    }

    #[test]
    fn ignores_unrelated_replacement() {
        // Not phonetically close → not a mishear.
        assert_eq!(detect_correction("ping the server foo", "ping the server Xylophone"), None);
    }

    #[test]
    fn learns_split_to_joined_name() {
        let got = detect_correction("Wir bauen die Heinz App weiter", "Wir bauen die HeinsApp weiter");
        assert_eq!(got, Some(("Heinz App".to_string(), "HeinsApp".to_string())));
    }

    #[test]
    fn learns_two_words_to_two_words() {
        let got = detect_correction("Termin mit Teigen Karbon morgen", "Termin mit Teijin Carbon morgen");
        assert_eq!(got, Some(("Teigen Karbon".to_string(), "Teijin Carbon".to_string())));
    }

    #[test]
    fn rest_of_field_must_be_unchanged() {
        // Two separate edits -> ambiguous, skip.
        assert_eq!(
            detect_correction("Heinz App und Teigen Karbon", "HeinsApp und Teijin Carbon"),
            None
        );
    }

    #[test]
    fn four_words_are_too_many() {
        assert_eq!(detect_correction("a b c d Wort", "Wuerzburg Xylon Quarz Olympia Wort"), None);
    }

    #[test]
    fn field_with_surrounding_text() {
        let before = "Hallo Chris, wir bauen die Heinz App weiter. Gruss";
        let after = "Hallo Chris, wir bauen die HeinsApp weiter. Gruss";
        let got = detect_field_correction("wir bauen die Heinz App weiter", before, after);
        assert_eq!(got, Some(("Heinz App".to_string(), "HeinsApp".to_string())));
        // An edit outside the dictated text is not ours to learn from.
        let after2 = "Hallo Chrisse, wir bauen die Heinz App weiter. Gruss";
        assert_eq!(detect_field_correction("wir bauen die Heinz App weiter", before, after2), None);
    }

    #[test]
    fn no_change_learns_nothing() {
        assert_eq!(detect_correction("hello there world", "hello there world"), None);
    }
}
