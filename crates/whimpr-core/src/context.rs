//! Vocabulary from outside the dictionary (external file, contacts, calendar) and
//! the ranked Whisper prompt built from all sources. Nothing here touches the
//! dictionary file: these terms live in memory only.

use std::path::Path;

use serde::Deserialize;

use crate::dictionary::{DictSource, DictionaryStore};

/// Whisper prompt budget used by the app (estimated tokens).
pub const PROMPT_BUDGET: usize = 180;

#[derive(Debug, Clone, Deserialize)]
struct ExtTerm {
    term: String,
    #[allow(dead_code)]
    #[serde(default)]
    kind: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ExtFile {
    #[serde(default)]
    terms: Vec<ExtTerm>,
}

/// Parse the external vocabulary file content (`{"terms":[{"term":..}]}`).
/// Broken input yields `Err`; the caller logs and carries on without it.
pub fn parse_external(json: &str) -> Result<Vec<String>, String> {
    let f: ExtFile = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(f.terms.into_iter().map(|t| t.term.trim().to_string()).filter(|t| !t.is_empty()).collect())
}

/// Read the external vocabulary; missing or broken file -> `Err` (never panics).
pub fn load_external(path: &Path) -> Result<Vec<String>, String> {
    let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse_external(&s)
}

/// A calendar-derived term with the start time of the event it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalTerm {
    pub term: String,
    pub start_unix: i64,
}

/// All in-memory vocabulary that is not part of the dictionary.
#[derive(Debug, Clone, Default)]
pub struct ContextVocab {
    pub external: Vec<String>,
    pub contacts: Vec<String>,
    pub calendar: Vec<CalTerm>,
}

impl ContextVocab {
    /// Every term from every source (for the phonetic comparison).
    pub fn all_terms(&self) -> impl Iterator<Item = &str> {
        self.external
            .iter()
            .map(|s| s.as_str())
            .chain(self.contacts.iter().map(|s| s.as_str()))
            .chain(self.calendar.iter().map(|c| c.term.as_str()))
    }

    pub fn len(&self) -> usize {
        self.external.len() + self.contacts.len() + self.calendar.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Dynamic inputs of the prompt ranking.
pub struct PromptInputs<'a> {
    pub vocab: &'a ContextVocab,
    /// Recently learned / accepted terms, newest first.
    pub learned: &'a [String],
    /// Vocabulary terms that appeared in recent dictations, newest first.
    pub recent: &'a [String],
    pub now: i64,
    /// End of "tomorrow" (unix); calendar terms starting before it rank first.
    pub soon_until: i64,
}

impl DictionaryStore {
    /// Build the Whisper `initial_prompt` from all sources, highest priority first:
    /// appointments today and tomorrow, recently learned terms, entries with known
    /// mishears, recently dictated vocabulary, manual dictionary entries, then
    /// everything else. An entry with mishears is proof that Whisper struggles
    /// with the word, so it ranks high: `apply_spellings` only repairs the exact
    /// mishears it knows, not new variants. Token cost is estimated as in `whisper_prompt`; a term that
    /// does not fit is skipped so shorter ones can still use the budget.
    pub fn whisper_prompt_ranked(&self, inp: &PromptInputs, max_tokens: usize) -> Option<(String, usize)> {
        let mut ranked: Vec<&str> = Vec::new();
        // 1 appointments today and tomorrow (soonest first)
        let mut soon: Vec<&crate::context::CalTerm> = inp
            .vocab
            .calendar
            .iter()
            .filter(|c| c.start_unix >= inp.now - 3 * 3600 && c.start_unix < inp.soon_until)
            .collect();
        soon.sort_by_key(|c| c.start_unix);
        ranked.extend(soon.iter().map(|c| c.term.as_str()));
        // 2 recently learned / accepted
        ranked.extend(inp.learned.iter().map(|s| s.as_str()));
        // 3 entries Whisper is known to get wrong (any source)
        ranked.extend(self.entries.iter().filter(|e| !e.mishears.is_empty()).map(|e| e.correct.as_str()));
        // 4 auto-learned dictionary entries (newest last in file)
        ranked.extend(
            self.entries
                .iter()
                .rev()
                .filter(|e| e.source == DictSource::Auto)
                .map(|e| e.correct.as_str()),
        );
        // 5 recently dictated vocabulary
        ranked.extend(inp.recent.iter().map(|s| s.as_str()));
        // 6 remaining manual entries
        ranked.extend(self.entries.iter().filter(|e| e.source == DictSource::Manual).map(|e| e.correct.as_str()));
        // 7 the rest
        ranked.extend(self.entries.iter().filter(|e| e.source != DictSource::Manual).map(|e| e.correct.as_str()));
        ranked.extend(inp.vocab.calendar.iter().map(|c| c.term.as_str()));
        ranked.extend(inp.vocab.external.iter().map(|s| s.as_str()));
        ranked.extend(inp.vocab.contacts.iter().map(|s| s.as_str()));

        let mut terms: Vec<&str> = Vec::new();
        let mut seen: std::collections::HashSet<String> = Default::default();
        let mut used = 0usize;
        for t in ranked {
            let t = t.trim();
            if t.is_empty() || t.contains('@') || t.chars().count() > 40 {
                continue;
            }
            let key = t.to_lowercase();
            if seen.contains(&key) {
                continue;
            }
            let cost = t.chars().count().div_ceil(3) + 1;
            if used + cost > max_tokens {
                continue;
            }
            seen.insert(key);
            used += cost;
            terms.push(t);
        }
        if terms.is_empty() {
            None
        } else {
            let n = terms.len();
            Some((terms.join(", "), n))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs<'a>(v: &'a ContextVocab, learned: &'a [String], recent: &'a [String]) -> PromptInputs<'a> {
        PromptInputs { vocab: v, learned, recent, now: 1000, soon_until: 1000 + 48 * 3600 }
    }

    #[test]
    fn external_missing_or_broken_does_not_panic() {
        assert!(load_external(Path::new("/nonexistent/vocab-extern.json")).is_err());
        assert!(parse_external("{ not json").is_err());
        assert_eq!(parse_external("{}").unwrap().len(), 0);
        let ok = parse_external(r#"{"updated":"x","source":"crm","terms":[{"term":"HeinsApp","kind":"firma"},{"term":" "}]}"#).unwrap();
        assert_eq!(ok, vec!["HeinsApp".to_string()]);
    }

    #[test]
    fn prompt_priority_order() {
        let mut d = DictionaryStore::default();
        d.add("ManuellA", vec![], DictSource::Manual);
        d.add("AutoGelernt", vec![], DictSource::Auto);
        d.add("ManuellMitHoer", vec!["manuel mit hör".into()], DictSource::Manual);
        let v = ContextVocab {
            external: vec!["Extern".into()],
            contacts: vec!["Kontakt".into()],
            calendar: vec![
                CalTerm { term: "TerminSpaeter".into(), start_unix: 1000 + 40 * 3600 },
                CalTerm { term: "TerminHeute".into(), start_unix: 1000 + 3600 },
                CalTerm { term: "TerminFern".into(), start_unix: 1000 + 10 * 24 * 3600 },
            ],
        };
        let learned = vec!["Frisch".to_string()];
        let recent = vec!["ZuletztBenutzt".to_string()];
        let (p, _) = d.whisper_prompt_ranked(&inputs(&v, &learned, &recent), 500).unwrap();
        assert_eq!(
            p,
            "TerminHeute, TerminSpaeter, Frisch, ManuellMitHoer, AutoGelernt, ZuletztBenutzt, ManuellA, TerminFern, Extern, Kontakt"
        );
    }

    #[test]
    fn hard_words_survive_a_big_dictionary() {
        // Regression 12.10.2026: with ~200 manual entries the ones with mishears
        // fell out of the budget and "Lugu" came back as "Logo".
        let mut d = DictionaryStore::default();
        for i in 0..200 {
            d.add(&format!("Eintrag{i:03}"), vec![], DictSource::Manual);
        }
        d.add("Lugu", vec!["Lügu".into()], DictSource::Manual);
        let v = ContextVocab::default();
        let (p, _) = d.whisper_prompt_ranked(&inputs(&v, &[], &[]), PROMPT_BUDGET).unwrap();
        assert!(p.starts_with("Lugu, "), "{p}");
    }

    #[test]
    fn prompt_respects_budget_and_dedups() {
        let d = DictionaryStore::default();
        let v = ContextVocab {
            external: (0..100).map(|i| format!("Begriff{i:03}")).collect(),
            contacts: vec!["begriff000".into()],
            calendar: vec![],
        };
        let (p, n) = d.whisper_prompt_ranked(&inputs(&v, &[], &[]), 50).unwrap();
        assert!(n > 0 && n * 4 <= 50);
        assert_eq!(p.split(", ").count(), n);
        assert_eq!(p.to_lowercase().matches("begriff000").count(), 1);
    }

    #[test]
    fn empty_everything_gives_none() {
        let d = DictionaryStore::default();
        let v = ContextVocab::default();
        assert!(d.whisper_prompt_ranked(&inputs(&v, &[], &[]), 180).is_none());
    }
}
