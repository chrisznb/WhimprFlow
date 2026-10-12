//! Auto Cleanup levels — how aggressively the LLM is allowed to edit.
//!
//! `None` bypasses the model entirely (raw ASR is pasted). The others append a
//! modifier to the shared system prompt. Light is WhimprFlow's default: research
//! found Wispr's more-aggressive default was the top "it changed what I said"
//! complaint, so we bias conservative.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CleanupLevel {
    /// Transcribe exactly what was said, including mistakes. No model call.
    None,
    /// Remove fillers and fix grammar only; preserve the speaker's words. (Default.)
    #[default]
    Light,
    /// Also tighten wording for clarity and concision without changing meaning.
    Medium,
    /// Rewrite phrasing for brevity and polish while preserving facts and intent.
    High,
}

impl CleanupLevel {
    /// True when no model should be invoked and the raw transcript is used verbatim.
    pub fn bypasses_llm(self) -> bool {
        matches!(self, CleanupLevel::None)
    }

    /// Text appended to the shared system prompt for this level (empty for `None`).
    pub fn modifier(self) -> &'static str {
        match self {
            CleanupLevel::None => "",
            CleanupLevel::Light => {
                "# LIGHT MODE (overrides the ALLOWED list above)\n\
                 Keep the speaker's exact words, word order, and sentence structure. You may \
                 ONLY: delete hesitations and pure fillers, collapse stutters, add punctuation \
                 and capitalization, convert spoken punctuation and layout cues, resolve a \
                 self-correction that has an explicit cue, format a list only when the speaker \
                 says list cues, and fix spellings from the custom vocabulary. Do NOT fix \
                 grammar, swap words for synonyms, merge, split, shorten, or drop sentences, \
                 and do not make sentences more complete or formal. If a sentence is \
                 ungrammatical as spoken, keep it as spoken. When unsure, return the words \
                 exactly as spoken."
            }
            CleanupLevel::Medium => {
                "You may also tighten wording for clarity and conciseness, but never change the \
                 meaning."
            }
            CleanupLevel::High => {
                "You may rewrite phrasing for brevity and polish while strictly preserving every \
                 fact, name, number, and the speaker's intent."
            }
        }
    }

    /// Ceiling on the *novelty ratio* (fraction of output words that were not
    /// spoken) the deterministic gate tolerates. Filler deletion and punctuation
    /// don't count as novelty; number/spoken-punctuation normalization introduces a
    /// little, so Light leaves headroom for that while still catching full rewrites.
    pub fn max_novelty_ratio(self) -> f32 {
        match self {
            CleanupLevel::None => 0.0,
            CleanupLevel::Light => 0.25,
            CleanupLevel::Medium => 0.55,
            CleanupLevel::High => 0.85,
        }
    }

    /// Ceiling on how much shorter (in characters) the cleaned text may be than
    /// the hesitation-stripped transcript. Light only removes fillers, so a big
    /// drop means the model cut real content; the higher levels may tighten.
    pub fn max_shrink(self) -> f32 {
        match self {
            CleanupLevel::None => 0.0,
            CleanupLevel::Light => 0.30,
            CleanupLevel::Medium => 0.45,
            CleanupLevel::High => 0.55,
        }
    }
}
