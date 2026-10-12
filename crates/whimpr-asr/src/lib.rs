//! Local speech-to-text via whisper.cpp (whisper-rs), implementing
//! [`whimpr_core::AsrEngine`]. Expects 16 kHz mono f32 samples.

use std::path::Path;
use std::sync::Mutex;

/// Spoken-language hint used by every transcription: "auto", "de", "en", ...
static LANGUAGE: Mutex<String> = Mutex::new(String::new());

/// Set the language hint. "auto" (or anything empty) lets Whisper detect it,
/// which costs a second pass through the model.
pub fn set_language(lang: &str) {
    *LANGUAGE.lock().unwrap() = lang.trim().to_lowercase();
}

fn language() -> String {
    let l = LANGUAGE.lock().unwrap().clone();
    if l.is_empty() { "auto".to_string() } else { l }
}

use whimpr_core::asr::{AsrCaps, AsrEngine, AsrEngineId, Transcript};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// A loaded whisper model ready to transcribe utterances.
pub struct WhisperEngine {
    ctx: WhisperContext,
}

impl WhisperEngine {
    /// Load a GGML/GGUF whisper model from `model_path`.
    pub fn load(model_path: &Path) -> anyhow::Result<Self> {
        let path = model_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("model path is not valid UTF-8"))?;
        let mut cparams = WhisperContextParameters::default();
        // Big speedup on Metal for the large encoder; we don't use DTW timestamps.
        cparams.flash_attn(true);
        let ctx = WhisperContext::new_with_params(path, cparams)
            .map_err(|e| anyhow::anyhow!("failed to load whisper model: {e}"))?;
        Ok(Self { ctx })
    }
}

impl AsrEngine for WhisperEngine {
    fn id(&self) -> AsrEngineId {
        AsrEngineId::WhisperCpp
    }

    fn caps(&self) -> AsrCaps {
        AsrCaps {
            supports_streaming: false,
        }
    }

    fn transcribe(&self, pcm16k: &[f32]) -> anyhow::Result<Transcript> {
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| anyhow::anyhow!("whisper create_state: {e}"))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        // "auto" detects per utterance (extra pass); a fixed language is ~2x
        // faster and still keeps English words in German dictation.
        let lang = language();
        params.set_language(Some(&lang));
        params.set_translate(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        // Push-to-talk utterances are always one short clip, not long-form audio.
        // Without this, whisper.cpp can split it into multiple internal segments
        // that repeat the same words — which then get concatenated below,
        // producing the sentence twice. Single-segment mode avoids that.
        params.set_single_segment(true);
        params.set_no_context(true);

        // whisper.cpp silently returns nothing for input under 1 s ("input is
        // too short"), which swallowed quick one-word dictations like "Ja".
        // Pad short clips with silence on both sides up to 1.25 s.
        const MIN_SAMPLES: usize = 20_000; // 1.25 s at 16 kHz
        let padded;
        let pcm16k = if pcm16k.len() < MIN_SAMPLES {
            let pad = MIN_SAMPLES - pcm16k.len();
            let mut v = vec![0.0f32; pad / 2];
            v.extend_from_slice(pcm16k);
            v.resize(MIN_SAMPLES, 0.0);
            padded = v;
            &padded[..]
        } else {
            pcm16k
        };

        state
            .full(params, pcm16k)
            .map_err(|e| anyhow::anyhow!("whisper full: {e}"))?;

        let n = state
            .full_n_segments()
            .map_err(|e| anyhow::anyhow!("whisper n_segments: {e}"))?;
        let mut text = String::new();
        for i in 0..n {
            if let Ok(seg) = state.full_get_segment_text(i) {
                text.push_str(&seg);
            }
        }

        Ok(Transcript {
            text: text.trim().to_string(),
            confidence: None,
        })
    }
}
