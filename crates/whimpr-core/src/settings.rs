//! User settings, persisted as JSON. Drives the cleanup engine (which provider,
//! how aggressive) and other behavior. Kept dependency-light so it lives in core.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cleanup::CleanupLevel;

/// Which cleanup engine processes transcripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CleanupMode {
    /// Paste the raw transcript (no cleanup).
    Raw,
    /// Local on-device model (default — works offline, no API key).
    #[default]
    Local,
    /// OpenAI cloud.
    OpenAi,
    /// Anthropic cloud.
    Anthropic,
}

/// Where speech is transcribed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AsrMode {
    /// On-device whisper.cpp (default, offline).
    #[default]
    Local,
    /// OpenAI-compatible /audio/transcriptions endpoint (Mistral Voxtral,
    /// Groq Whisper, OpenAI). Uses the OpenAI API key from the keychain.
    Cloud,
}

/// Writing style per target-app category (Wispr-style "Style" feature).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StyleLevel {
    /// Caps + full punctuation.
    Formal,
    /// Caps + lighter punctuation.
    #[default]
    Casual,
    /// no caps + minimal punctuation.
    VeryCasual,
}

/// Per-category style preferences, keyed by what kind of app the text lands in.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StylePrefs {
    pub personal: StyleLevel,
    pub work: StyleLevel,
    pub email: StyleLevel,
    pub other: StyleLevel,
}

impl Default for StylePrefs {
    fn default() -> Self {
        Self {
            personal: StyleLevel::Casual,
            work: StyleLevel::Formal,
            email: StyleLevel::Formal,
            other: StyleLevel::Casual,
        }
    }
}

impl StyleLevel {
    /// The "# Formatting Mode" instruction the cleanup prompt understands.
    pub fn modifier(self) -> &'static str {
        match self {
            StyleLevel::Formal => {
                "Formal: proper sentence capitalization and full punctuation; complete sentences."
            }
            StyleLevel::Casual => {
                "Casual: natural capitalization, lighter punctuation; contractions are fine."
            }
            StyleLevel::VeryCasual => {
                "very casual: lowercase except proper nouns, minimal punctuation, chat-style."
            }
        }
    }
}

/// Persisted user configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub cleanup_mode: CleanupMode,
    pub cleanup_level: CleanupLevel,
    /// Per-app-category writing style.
    #[serde(default)]
    pub style: StylePrefs,
    /// Where transcription runs.
    #[serde(default)]
    pub asr_mode: AsrMode,
    /// Base URL for cloud transcription (OpenAI-compatible).
    #[serde(default = "default_asr_base_url")]
    pub asr_base_url: String,
    /// Model for cloud transcription.
    #[serde(default = "default_asr_model")]
    pub asr_model: String,
    /// Extra dictation hotkey (accelerator syntax, e.g. "F13" or "Ctrl+Space").
    /// Fn always works; this adds a second key. Empty = none.
    #[serde(default)]
    pub dictation_hotkey: String,
    pub openai_model: String,
    /// API root for the "OpenAI" cleanup mode, e.g. `https://openrouter.ai/api/v1`
    /// to route through OpenRouter instead of OpenAI directly (same wire format).
    /// Empty string (the default) means OpenAI's own endpoint.
    #[serde(default)]
    pub openai_base_url: String,
    pub anthropic_model: String,
    /// Play the record-start ping.
    pub sound_on_start: bool,
    /// Read the focused field's text (Accessibility) so cleanup understands the
    /// surroundings — names, thread tone, what the reply refers to.
    #[serde(default = "default_true")]
    pub context_awareness: bool,
    /// Pause Spotify/Music while recording; resume afterwards.
    #[serde(default = "default_true")]
    pub mute_music_while_dictating: bool,
    /// The user's own typing speed (words/min), used for the "time saved vs
    /// typing" stat. 45 matches Wispr Flow's cited typed baseline.
    #[serde(default = "default_typing_wpm")]
    pub typing_wpm: u32,
}

fn default_typing_wpm() -> u32 {
    45
}

fn default_true() -> bool {
    true
}

fn default_asr_base_url() -> String {
    "https://api.mistral.ai/v1".to_string()
}

fn default_asr_model() -> String {
    "voxtral-mini-latest".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            cleanup_mode: CleanupMode::default(),
            cleanup_level: CleanupLevel::Light,
            style: StylePrefs::default(),
            openai_model: "gpt-4o-mini".to_string(),
            openai_base_url: String::new(),
            anthropic_model: "claude-haiku-4-5".to_string(),
            sound_on_start: true,
            context_awareness: true,
            mute_music_while_dictating: true,
            asr_mode: AsrMode::default(),
            asr_base_url: default_asr_base_url(),
            asr_model: default_asr_model(),
            dictation_hotkey: String::new(),
            typing_wpm: default_typing_wpm(),
        }
    }
}

impl Settings {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let s = Settings::default();
        assert_eq!(s.cleanup_mode, CleanupMode::Local);
        assert_eq!(s.cleanup_level, CleanupLevel::Light);
    }

    #[test]
    fn round_trips_json() {
        let s = Settings {
            cleanup_mode: CleanupMode::Local,
            ..Default::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cleanup_mode, CleanupMode::Local);
    }
}
