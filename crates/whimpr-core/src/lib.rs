//! `whimpr-core` — the platform-agnostic brain of WhimprFlow.
//!
//! Everything here is shared verbatim between macOS and Windows. Native concerns
//! (the hotkey hook, text injection, accessibility reads) live in the sidecar; the
//! ASR and cleanup-LLM implementations live in their own crates and plug in behind
//! the [`asr`] and [`cleanup`] trait seams defined here.
//!
//! What is implemented so far (M0/M1 foundation): the dictation [`state`] machine.
//! Subsequent milestones fill in the audio pipeline, ASR/cleanup traits, dictionary,
//! settings, and storage modules.

pub mod asr;
pub mod cleanup;
pub mod dictionary;
pub mod settings;
pub mod snippets;
pub mod state;
pub mod stats;
pub mod types;

pub use asr::{AsrEngine, AsrEngineId, Transcript};
pub use cleanup::{CleanupContext, CleanupLevel, CleanupProvider, ProviderId, VocabEntry};
pub use dictionary::{DictSource, DictionaryEntry, DictionaryStore};
pub use settings::{AsrMode, CleanupMode, Settings, StyleLevel, StylePrefs};
pub use snippets::{Snippet, SnippetStore};
pub use stats::{HistoryItem, SessionRecord, StatsStore, StatsSummary};
pub use state::{Action, BarState, DictationState, Input, PipelineEvent, StateMachine, TriggerToken};
pub use types::{RecordMode, SessionId};

/// Compare two dotted version strings ("0.8.10" > "0.8.9"). Non-numeric parts
/// are treated as 0, missing parts as 0, so "0.9" == "0.9.0". Returns true when
/// `candidate` is strictly newer than `current` — used so the updater can never
/// "update" a user down to an older release.
pub fn is_newer_version(candidate: &str, current: &str) -> bool {
    let part = |s: &str, i: usize| -> u64 {
        s.trim_start_matches('v')
            .split('.')
            .nth(i)
            .and_then(|p| p.trim().parse::<u64>().ok())
            .unwrap_or(0)
    };
    for i in 0..4 {
        let (a, b) = (part(candidate, i), part(current, i));
        if a != b {
            return a > b;
        }
    }
    false
}

#[cfg(test)]
mod version_tests {
    use super::is_newer_version;

    #[test]
    fn newer_patch_and_minor_win() {
        assert!(is_newer_version("0.8.4", "0.8.3"));
        assert!(is_newer_version("0.9.0", "0.8.99"));
        assert!(is_newer_version("1.0.0", "0.9.9"));
    }

    #[test]
    fn same_version_is_not_newer() {
        assert!(!is_newer_version("0.8.4", "0.8.4"));
        assert!(!is_newer_version("0.9", "0.9.0"));
    }

    #[test]
    fn older_release_never_counts_as_update() {
        // The bug this guards: a locally built 0.8.4 must not be "updated" to 0.8.3.
        assert!(!is_newer_version("0.8.3", "0.8.4"));
        assert!(!is_newer_version("0.8.9", "0.9.0"));
    }

    #[test]
    fn numeric_not_lexicographic() {
        assert!(is_newer_version("0.8.10", "0.8.9"));
        assert!(!is_newer_version("0.8.9", "0.8.10"));
    }

    #[test]
    fn tolerates_v_prefix_and_junk() {
        assert!(is_newer_version("v0.8.5", "0.8.4"));
        assert!(!is_newer_version("garbage", "0.8.4"));
    }
}
