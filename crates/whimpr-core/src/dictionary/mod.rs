//! Custom dictionary: user vocabulary plus a pre-filter that injects only the
//! entries relevant to a given utterance into the cleanup prompt (fewer distractors
//! → higher LLM precision). Manual entries and auto-learned (✨) entries share the
//! same store; the auto-learn diff engine (needs accessibility reads) layers on top.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cleanup::VocabEntry;

/// How a dictionary entry was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictSource {
    Manual,
    Auto,
}

fn default_source() -> DictSource {
    DictSource::Manual
}

/// One vocabulary entry: the authoritative spelling and known mishears.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictionaryEntry {
    pub correct: String,
    #[serde(default)]
    pub mishears: Vec<String>,
    #[serde(default = "default_source")]
    pub source: DictSource,
}

/// The user's dictionary, persisted as JSON.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DictionaryStore {
    pub entries: Vec<DictionaryEntry>,
}

impl DictionaryStore {
    /// Load from `path`, returning an empty store if missing or unreadable.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Persist to `path` (creating parent dirs).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).unwrap_or_default();
        std::fs::write(path, json)
    }

    /// Add or merge an entry, de-duplicating by spelling (case-insensitive).
    pub fn add(&mut self, correct: impl Into<String>, mishears: Vec<String>, source: DictSource) {
        let correct = correct.into();
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|e| e.correct.eq_ignore_ascii_case(&correct))
        {
            for m in mishears {
                if !existing.mishears.iter().any(|x| x.eq_ignore_ascii_case(&m)) {
                    existing.mishears.push(m);
                }
            }
        } else {
            self.entries.push(DictionaryEntry {
                correct,
                mishears,
                source,
            });
        }
    }

    /// Remove an entry by its spelling (case-insensitive). Returns true if removed.
    pub fn remove(&mut self, correct: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| !e.correct.eq_ignore_ascii_case(correct));
        self.entries.len() != before
    }

    /// Apply the dictionary as a pure spelling pass, with no model involved:
    /// every known mishear becomes the authoritative spelling, and the spelling
    /// itself is normalized to its stored casing ("baulyo" -> "Baulyo"). Matches
    /// are exact, case-insensitive, and word-boundary checked — never fuzzy, so
    /// this can run unattended on a whole file transcript without inventing
    /// corrections. Longer mishears are applied first, so a multi-word mishear
    /// ("charge bee") wins over any shorter one it contains.
    pub fn apply_spellings(&self, text: &str) -> String {
        let mut pairs: Vec<(&str, &str)> = Vec::new();
        for e in &self.entries {
            for m in &e.mishears {
                if !m.trim().is_empty() {
                    pairs.push((m.as_str(), e.correct.as_str()));
                }
            }
        }
        // Longest needle first so "charge bee" is consumed before "bee".
        pairs.sort_by_key(|(m, _)| std::cmp::Reverse(m.chars().count()));
        let mut out = text.to_string();
        for (mishear, correct) in pairs {
            out = crate::snippets::replace_phrase(&out, mishear, correct);
        }
        // Normalize the casing of the authoritative spellings themselves.
        for e in &self.entries {
            out = crate::snippets::replace_phrase(&out, &e.correct, &e.correct);
        }
        out
    }

    /// Select the entries relevant to `utterance` — those whose spelling or a known
    /// mishear is edit-close to a spoken token (or adjacent token pair, to catch
    /// split words like "charge bee" → "ChargeBee") — capped to `max`.
    pub fn prefilter(&self, utterance: &str, max: usize) -> Vec<VocabEntry> {
        let toks: Vec<String> = utterance
            .split_whitespace()
            .map(|t| {
                t.trim_matches(|c: char| c.is_ascii_punctuation())
                    .to_lowercase()
            })
            .filter(|t| !t.is_empty())
            .collect();

        let mut grams: Vec<String> = toks.clone();
        for w in toks.windows(2) {
            grams.push(format!("{}{}", w[0], w[1]));
        }

        let mut out = Vec::new();
        for e in &self.entries {
            let targets: Vec<String> = std::iter::once(e.correct.to_lowercase())
                .chain(e.mishears.iter().map(|m| m.to_lowercase()))
                .collect();
            if grams.iter().any(|g| targets.iter().any(|t| close(g, t))) {
                out.push(VocabEntry {
                    correct: e.correct.clone(),
                    mishears: e.mishears.clone(),
                });
                if out.len() >= max {
                    break;
                }
            }
        }
        out
    }
}

/// Two tokens are "close" if identical or within a normalized edit distance of 0.34.
fn close(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let maxlen = a.chars().count().max(b.chars().count());
    if maxlen == 0 {
        return false;
    }
    (strsim::levenshtein(a, b) as f32 / maxlen as f32) <= 0.34
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> DictionaryStore {
        let mut s = DictionaryStore::default();
        s.add("Manvi", vec!["Monvi".into(), "Manvee".into()], DictSource::Manual);
        s.add("ChargeBee", vec!["charge bee".into()], DictSource::Manual);
        s
    }

    #[test]
    fn prefilter_selects_close_mishear() {
        // "monvi" is an exact mishear of Manvi.
        let v = store().prefilter("send the deck to monvi please", 15);
        assert!(v.iter().any(|e| e.correct == "Manvi"));
        assert!(!v.iter().any(|e| e.correct == "ChargeBee"));
    }

    #[test]
    fn prefilter_catches_split_word_via_bigram() {
        // "charge bee" spoken as two words → bigram "chargebee" matches.
        let v = store().prefilter("we should renew charge bee this month", 15);
        assert!(v.iter().any(|e| e.correct == "ChargeBee"));
    }

    #[test]
    fn prefilter_ignores_unrelated_utterance() {
        let v = store().prefilter("the weather is nice today", 15);
        assert!(v.is_empty());
    }

    #[test]
    fn apply_spellings_fixes_mishears_and_casing() {
        let s = store();
        assert_eq!(
            s.apply_spellings("Send the deck to monvi and renew charge bee, chargebee is due."),
            "Send the deck to Manvi and renew ChargeBee, ChargeBee is due."
        );
    }

    #[test]
    fn apply_spellings_leaves_unrelated_words_alone() {
        let s = store();
        // "Manvis" (inflected) and unrelated text must survive untouched: this
        // pass is exact, never fuzzy.
        assert_eq!(
            s.apply_spellings("Manvis Laptop und ein Monvieee bleiben."),
            "Manvis Laptop und ein Monvieee bleiben."
        );
    }

    #[test]
    fn add_merges_mishears_case_insensitively() {
        let mut s = store();
        s.add("manvi", vec!["Manvie".into()], DictSource::Auto);
        let e = s.entries.iter().find(|e| e.correct == "Manvi").unwrap();
        assert!(e.mishears.iter().any(|m| m == "Manvie"));
        assert_eq!(s.entries.iter().filter(|e| e.correct.eq_ignore_ascii_case("manvi")).count(), 1);
    }
}
