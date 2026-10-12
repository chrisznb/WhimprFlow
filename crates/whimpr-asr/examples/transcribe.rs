//! Transcribe a 16 kHz mono WAV with the local whisper engine.
//! Usage: cargo run -p whimpr-asr --example transcribe -- <model.bin> <audio.wav> [dictionary.json] [vocab-extern.json]
//! With a dictionary, the file is transcribed three times: without a prompt, with the
//! plain dictionary prompt, and (with an external vocabulary) with the ranked prompt.

use std::path::Path;

use whimpr_core::AsrEngine;
use whimpr_asr::WhisperEngine;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: transcribe <model.bin> <audio.wav>");
        std::process::exit(2);
    }
    let model = &args[1];
    let wav = &args[2];

    let mut reader = hound::WavReader::open(wav)?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    // Read i16 PCM and downmix to mono f32 in [-1, 1].
    let raw: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>()?;
    let mut mono: Vec<f32> = Vec::with_capacity(raw.len() / channels);
    for frame in raw.chunks(channels) {
        let sum: i32 = frame.iter().map(|s| *s as i32).sum();
        mono.push(sum as f32 / channels as f32 / 32768.0);
    }
    let pcm = whimpr_audio_resample(&mono, spec.sample_rate);

    whimpr_asr::set_language("de");
    let engine = WhisperEngine::load(Path::new(model))?;
    let _ = engine.transcribe(&vec![0.0f32; 16_000]); // warm up Metal
    let t0 = std::time::Instant::now();
    let t = engine.transcribe(&pcm)?;
    println!("TRANSCRIPT: {}  ({:?})", t.text, t0.elapsed());
    if let Some(dict) = args.get(3) {
        let store = whimpr_core::DictionaryStore::load(Path::new(dict));
        if let Some((prompt, n)) = store.whisper_prompt(180) {
            whimpr_asr::set_initial_prompt(&prompt);
            println!("PROMPT(old): {n} terms: {prompt}");
            let t0 = std::time::Instant::now();
            let t = engine.transcribe(&pcm)?;
            println!("TRANSCRIPT+PROMPT(old): {}  ({:?})", t.text, t0.elapsed());
        }
        let mut vocab = whimpr_core::ContextVocab::default();
        if let Some(ext) = args.get(4) {
            vocab.external = whimpr_core::context::load_external(Path::new(ext)).unwrap_or_default();
        }
        let now = 0i64;
        // WHIMPR_CAL="A,B" simulates today's appointments, WHIMPR_RECENT="C,D" recently dictated terms.
        let env_list = |k: &str| -> Vec<String> {
            std::env::var(k).unwrap_or_default().split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
        };
        vocab.calendar = env_list("WHIMPR_CAL")
            .into_iter()
            .map(|term| whimpr_core::CalTerm { term, start_unix: 1 })
            .collect();
        let recent = env_list("WHIMPR_RECENT");
        let inp = whimpr_core::PromptInputs { vocab: &vocab, learned: &[], recent: &recent, now, soon_until: 100 };
        if let Some((prompt, n)) = store.whisper_prompt_ranked(&inp, whimpr_core::context::PROMPT_BUDGET) {
            whimpr_asr::set_initial_prompt(&prompt);
            println!("PROMPT(new): {n} terms: {prompt}");
            let t0 = std::time::Instant::now();
            let t = engine.transcribe(&pcm)?;
            println!("TRANSCRIPT+PROMPT(new): {}  ({:?})", t.text, t0.elapsed());
        }
    }
    Ok(())
}

/// Minimal inline 16 kHz resample so this example needn't depend on whimpr-audio.
fn whimpr_audio_resample(input: &[f32], src_rate: u32) -> Vec<f32> {
    const DST: u32 = 16_000;
    if src_rate == DST || src_rate == 0 || input.is_empty() {
        return input.to_vec();
    }
    let ratio = DST as f64 / src_rate as f64;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 / ratio;
            let idx = pos.floor() as usize;
            let frac = (pos - idx as f64) as f32;
            let a = input.get(idx).copied().unwrap_or(0.0);
            let b = input.get(idx + 1).copied().unwrap_or(a);
            a + (b - a) * frac
        })
        .collect()
}
