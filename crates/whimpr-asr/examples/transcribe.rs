//! Transcribe a 16 kHz mono WAV with the local whisper engine.
//! Usage: cargo run -p whimpr-asr --example transcribe -- <model.bin> <audio.wav> [dictionary.json]
//! With a dictionary, the file is transcribed twice: without and with the Whisper prompt.

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
            println!("PROMPT: {n} terms");
            let t0 = std::time::Instant::now();
            let t = engine.transcribe(&pcm)?;
            println!("TRANSCRIPT+PROMPT: {}  ({:?})", t.text, t0.elapsed());
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
