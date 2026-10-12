//! Phonetic matching for German-sounding names: Kölner Phonetik plus a
//! normalized edit-distance similarity. Used to find "this sounds like a known
//! term but is spelled differently" (Whisper mishears).

/// Kölner Phonetik: map a word (or several, spaces are ignored) to a digit string.
/// Words that sound alike get the same code ("Heinz App" and "HeinsApp" -> "681").
pub fn cologne(input: &str) -> String {
    // Normalize: lowercase letters only, umlauts and ß folded to their base sound.
    let mut chars: Vec<char> = Vec::new();
    for c in input.chars().flat_map(|c| c.to_lowercase()) {
        match c {
            'ä' => chars.push('a'),
            'ö' => chars.push('o'),
            'ü' => chars.push('u'),
            'ß' => chars.push('s'),
            'a'..='z' => chars.push(c),
            _ => {}
        }
    }
    let at = |i: isize| -> Option<char> {
        if i < 0 {
            None
        } else {
            chars.get(i as usize).copied()
        }
    };
    let mut codes: Vec<char> = Vec::new();
    for i in 0..chars.len() {
        let c = chars[i];
        let prev = at(i as isize - 1);
        let next = at(i as isize + 1);
        let code: &str = match c {
            'a' | 'e' | 'i' | 'j' | 'o' | 'u' | 'y' => "0",
            'h' => "",
            'b' => "1",
            'p' => {
                if next == Some('h') {
                    "3"
                } else {
                    "1"
                }
            }
            'd' | 't' => {
                if matches!(next, Some('c') | Some('s') | Some('z')) {
                    "8"
                } else {
                    "2"
                }
            }
            'f' | 'v' | 'w' => "3",
            'g' | 'k' | 'q' => "4",
            'c' => {
                if i == 0 {
                    if matches!(
                        next,
                        Some('a' | 'h' | 'k' | 'l' | 'o' | 'q' | 'r' | 'u' | 'x')
                    ) {
                        "4"
                    } else {
                        "8"
                    }
                } else if matches!(prev, Some('s') | Some('z')) {
                    "8"
                } else if matches!(next, Some('a' | 'h' | 'k' | 'o' | 'q' | 'u' | 'x')) {
                    "4"
                } else {
                    "8"
                }
            }
            'x' => {
                if matches!(prev, Some('c' | 'k' | 'q')) {
                    "8"
                } else {
                    "48"
                }
            }
            'l' => "5",
            'm' | 'n' => "6",
            'r' => "7",
            's' | 'z' => "8",
            _ => "",
        };
        codes.extend(code.chars());
    }
    // Collapse consecutive duplicates, then drop vowels (0) except at the start.
    let mut dedup: Vec<char> = Vec::new();
    for c in codes {
        if dedup.last() != Some(&c) {
            dedup.push(c);
        }
    }
    dedup
        .iter()
        .enumerate()
        .filter(|(i, c)| **c != '0' || *i == 0)
        .map(|(_, c)| *c)
        .collect()
}

/// Similarity of two strings in 0..=1 (1 = identical): 1 - Levenshtein / longer
/// length, case-insensitive and ignoring spaces and hyphens.
pub fn similarity(a: &str, b: &str) -> f32 {
    let norm = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect()
    };
    let (a, b) = (norm(a), norm(b));
    let m = a.chars().count().max(b.chars().count());
    if m == 0 {
        return 0.0;
    }
    1.0 - strsim::levenshtein(&a, &b) as f32 / m as f32
}

/// True if two phonetic codes are equal, or (for longer codes) differ by one edit.
pub fn codes_close(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    a.len().min(b.len()) >= 5 && strsim::levenshtein(a, b) <= 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_example_mueller_luedenscheidt() {
        // Well-known reference value from the Kölner Phonetik description.
        assert_eq!(cologne("Müller-Lüdenscheidt"), "65752682");
    }

    #[test]
    fn reference_example_breschnew() {
        assert_eq!(cologne("Breschnew"), "17863");
    }

    #[test]
    fn heinz_app_sounds_like_heinsapp() {
        assert_eq!(cologne("Heinz App"), cologne("HeinsApp"));
        assert_eq!(cologne("HeinsApp"), "0681");
    }

    #[test]
    fn x_and_c_rules() {
        assert_eq!(cologne("Xaver"), "4837");
        assert_eq!(cologne("Christian"), "47826"); // initial C before H -> 4
    }

    #[test]
    fn similarity_bounds() {
        assert!(similarity("Heinz App", "HeinsApp") > 0.85);
        assert!(similarity("Tisch", "Lugu") < 0.3);
        assert_eq!(similarity("", ""), 0.0);
    }
}
