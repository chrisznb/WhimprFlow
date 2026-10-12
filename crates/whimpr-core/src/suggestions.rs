//! Pattern-based suggestions: after each dictation, compare 1-3-word n-grams of
//! the final text against the whole known vocabulary. An n-gram that sounds like
//! a vocabulary term (Kölner Phonetik, plus string similarity) but is spelled
//! differently is a likely mishear. Candidates are counted (text pairs only, never
//! audio); from two hits on the UI offers "X always means Y?".

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::common_words::is_common_word;
use crate::phonetics::{codes_close, cologne, similarity};

/// Hits needed before a pair is shown to the user.
pub const SUGGEST_THRESHOLD: u32 = 2;
/// Candidates shorter than this (letters, spaces removed) are ignored.
const MIN_LEN: usize = 4;

/// One counted candidate: `heard` was written where `correct` is probably meant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub heard: String,
    pub correct: String,
    #[serde(default)]
    pub count: u32,
}

/// Persisted counters and the never-again list (`suggestions.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SuggestionStore {
    #[serde(default)]
    pub counts: Vec<Candidate>,
    /// Pairs the user declined (or already accepted): never suggested again.
    #[serde(default)]
    pub rejected: Vec<Candidate>,
}

fn same_pair(c: &Candidate, heard: &str, correct: &str) -> bool {
    c.heard.eq_ignore_ascii_case(heard) && c.correct.eq_ignore_ascii_case(correct)
}

impl SuggestionStore {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default())
    }

    /// Count one hit for a pair (ignored if the pair was rejected before).
    pub fn record(&mut self, heard: &str, correct: &str) {
        if self.rejected.iter().any(|c| same_pair(c, heard, correct)) {
            return;
        }
        if let Some(c) = self.counts.iter_mut().find(|c| same_pair(c, heard, correct)) {
            c.count += 1;
        } else {
            self.counts.push(Candidate {
                heard: heard.to_string(),
                correct: correct.to_string(),
                count: 1,
            });
        }
        // Keep the file small: drop the weakest singletons beyond a cap.
        if self.counts.len() > 400 {
            self.counts.sort_by_key(|c| std::cmp::Reverse(c.count));
            self.counts.truncate(300);
        }
    }

    /// Pairs that reached the threshold, most frequent first.
    pub fn pending(&self) -> Vec<Candidate> {
        let mut v: Vec<Candidate> = self
            .counts
            .iter()
            .filter(|c| c.count >= SUGGEST_THRESHOLD)
            .filter(|c| !self.rejected.iter().any(|r| same_pair(r, &c.heard, &c.correct)))
            .cloned()
            .collect();
        v.sort_by_key(|c| std::cmp::Reverse(c.count));
        v
    }

    /// Remove a pair from the counters and remember it so it never returns
    /// (used for both "reject" and "accept": after accepting, the mishear lives in
    /// the dictionary).
    pub fn resolve(&mut self, heard: &str, correct: &str) {
        self.counts.retain(|c| !same_pair(c, heard, correct));
        if !self.rejected.iter().any(|c| same_pair(c, heard, correct)) {
            self.rejected.push(Candidate {
                heard: heard.to_string(),
                correct: correct.to_string(),
                count: 0,
            });
        }
    }
}

/// A vocabulary term with its phonetic code precomputed.
pub struct IndexedTerm {
    pub term: String,
    joined: String,
    code: String,
}

/// Index the vocabulary once per dictation (cheap: a few thousand terms).
pub fn index_terms<'a>(terms: impl Iterator<Item = &'a str>) -> Vec<IndexedTerm> {
    let mut out: Vec<IndexedTerm> = Vec::new();
    for t in terms {
        let t = t.trim();
        if t.contains('@') || t.chars().any(|c| c.is_ascii_digit()) {
            continue;
        }
        let joined: String = t.chars().filter(|c| c.is_alphabetic()).collect::<String>().to_lowercase();
        if joined.chars().count() < MIN_LEN || is_common_word(&joined) {
            continue;
        }
        out.push(IndexedTerm { term: t.to_string(), code: cologne(t), joined });
    }
    out
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

/// Find `(heard, correct)` candidates in one dictated text. `known_mishears` are
/// spellings the dictionary already corrects (they never need a suggestion).
pub fn find_candidates(text: &str, index: &[IndexedTerm], known_mishears: &[String]) -> Vec<(String, String)> {
    let ws = words(text);
    let mut found: Vec<(String, String, f32)> = Vec::new();
    for n in 1..=3usize {
        for win in ws.windows(n) {
            let heard = win.join(" ");
            let joined: String = heard.chars().filter(|c| c.is_alphabetic()).collect::<String>().to_lowercase();
            if joined.chars().count() < MIN_LEN || joined.chars().count() != heard.chars().filter(|c| !c.is_whitespace()).count() {
                continue; // too short, or contains digits/punctuation inside
            }
            // Common words never count; a multi-word n-gram of only common words neither.
            if win.iter().all(|w| is_common_word(w)) || is_common_word(&joined) {
                continue;
            }
            if known_mishears.iter().any(|m| m.eq_ignore_ascii_case(&heard)) {
                continue;
            }
            let code = cologne(&heard);
            if code.len() < 3 {
                continue;
            }
            // The n-gram is itself a known term: it is spelled right.
            if index.iter().any(|t| t.joined == joined) {
                continue;
            }
            for t in index {
                if !codes_close(&code, &t.code) {
                    continue;
                }
                let sim = similarity(&heard, &t.term);
                let need = if code == t.code { 0.6 } else { 0.75 };
                if sim >= need {
                    found.push((heard.clone(), t.term.clone(), sim));
                }
            }
        }
    }
    // Per target term keep only the best-matching n-gram.
    found.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<(String, String)> = Vec::new();
    for (h, c, _) in found {
        if !out.iter().any(|(_, oc)| oc.eq_ignore_ascii_case(&c))
            && !out.iter().any(|(oh, _)| oh.to_lowercase().contains(&h.to_lowercase()) || h.to_lowercase().contains(&oh.to_lowercase()))
        {
            out.push((h, c));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idx(terms: &[&str]) -> Vec<IndexedTerm> {
        index_terms(terms.iter().copied())
    }

    #[test]
    fn finds_split_mishear() {
        let i = idx(&["HeinsApp", "Teijin Carbon"]);
        let c = find_candidates("Wir bauen die Heinz App weiter", &i, &[]);
        assert_eq!(c, vec![("Heinz App".to_string(), "HeinsApp".to_string())]);
    }

    #[test]
    fn exact_term_is_not_a_candidate() {
        let i = idx(&["HeinsApp"]);
        assert!(find_candidates("Die HeinsApp läuft", &i, &[]).is_empty());
        assert!(find_candidates("Die heinsapp läuft", &i, &[]).is_empty());
    }

    #[test]
    fn common_and_short_words_are_ignored() {
        // "Tisch"/"Termin" are common words; "Tim" is too short.
        let i = idx(&["Tisch", "Termin", "Timo"]);
        assert!(find_candidates("Der Tisch beim Termin mit Tim", &i, &[]).is_empty());
    }

    #[test]
    fn known_mishear_is_skipped() {
        let i = idx(&["HeinsApp"]);
        let known = vec!["Heinz App".to_string()];
        assert!(find_candidates("die Heinz App", &i, &known).is_empty());
    }

    #[test]
    fn unrelated_text_gives_nothing() {
        let i = idx(&["HeinsApp", "Teijin Carbon", "Lugu"]);
        assert!(find_candidates("Ich schreibe morgen eine kurze Mail an den Steuerberater", &i, &[]).is_empty());
    }

    #[test]
    fn threshold_reject_and_never_again() {
        let mut s = SuggestionStore::default();
        s.record("Heinz App", "HeinsApp");
        assert!(s.pending().is_empty(), "one hit is not enough");
        s.record("heinz app", "HeinsApp");
        let p = s.pending();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].count, 2);
        s.resolve("Heinz App", "HeinsApp");
        assert!(s.pending().is_empty());
        s.record("Heinz App", "HeinsApp");
        s.record("Heinz App", "HeinsApp");
        assert!(s.pending().is_empty(), "rejected pairs never return");
    }
}
