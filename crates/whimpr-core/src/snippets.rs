//! Spoken snippets: say a trigger phrase, get the stored replacement text.
//! Applied AFTER cleanup, as a whole-phrase, case-insensitive replacement, so
//! "my email address" anywhere in the cleaned text expands to the stored value.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snippet {
    pub trigger: String,
    pub replacement: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SnippetStore {
    pub entries: Vec<Snippet>,
}

impl SnippetStore {
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
        let json = serde_json::to_string_pretty(self).unwrap_or_default();
        std::fs::write(path, json)
    }

    pub fn add(&mut self, trigger: impl Into<String>, replacement: impl Into<String>) {
        let trigger = trigger.into();
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.trigger.eq_ignore_ascii_case(&trigger))
        {
            e.replacement = replacement.into();
        } else {
            self.entries.push(Snippet {
                trigger,
                replacement: replacement.into(),
            });
        }
    }

    pub fn remove(&mut self, trigger: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| !e.trigger.eq_ignore_ascii_case(trigger));
        self.entries.len() != before
    }

    /// Replace every whole-phrase occurrence of a trigger (case-insensitive,
    /// word-boundary checked) with its replacement.
    pub fn apply(&self, text: &str) -> String {
        let mut out = text.to_string();
        for s in &self.entries {
            out = replace_phrase(&out, &s.trigger, &s.replacement);
        }
        out
    }
}

/// Boundary-checked, case-insensitive whole-phrase replacement.
pub(crate) fn replace_phrase(input: &str, phrase: &str, replacement: &str) -> String {
    if phrase.is_empty() {
        return input.to_string();
    }
    let lower_input: Vec<char> = input.to_lowercase().chars().collect();
    let chars: Vec<char> = input.chars().collect();
    let needle: Vec<char> = phrase.to_lowercase().chars().collect();
    // to_lowercase can change char counts in exotic cases; bail to a plain copy.
    if lower_input.len() != chars.len() {
        return input.to_string();
    }
    let n = chars.len();
    let plen = needle.len();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < n {
        let boundary_before = i == 0 || !chars[i - 1].is_alphanumeric();
        if boundary_before
            && i + plen <= n
            && (0..plen).all(|k| lower_input[i + k] == needle[k])
            && (i + plen == n || !chars[i + plen].is_alphanumeric())
        {
            out.push_str(replacement);
            i += plen;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_whole_phrase_case_insensitively() {
        let mut s = SnippetStore::default();
        s.add("my email address", "chris@example.com");
        assert_eq!(
            s.apply("Schick es an My Email Address."),
            "Schick es an chris@example.com."
        );
    }

    #[test]
    fn does_not_touch_partial_words() {
        let mut s = SnippetStore::default();
        s.add("mail", "X");
        assert_eq!(s.apply("Gmail is not mail-like"), "Gmail is not X-like");
    }
}
