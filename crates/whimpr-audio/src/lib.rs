//! Microphone capture for WhimprFlow.
//!
//! [`start`] opens the default input device and streams audio. While it runs it
//! downmixes to mono, accumulates the whole utterance, and invokes a throttled
//! callback with a small rolling window of RMS levels (0..1) for the pill's live
//! waveform. [`CaptureHandle::stop`] returns the accumulated mono samples plus the
//! device sample rate, ready for resampling to 16 kHz and handing to ASR.
//!
//! cpal's macOS `Stream` is not `Send`, so the stream is created and owned on a
//! dedicated thread; control flows over channels.

use std::collections::VecDeque;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Number of bars in the rolling waveform window (matches the pill's bar count).
const WAVE_BARS: usize = 6;
/// Emit the waveform at ~30 fps.
const EMIT_INTERVAL: Duration = Duration::from_millis(33);
/// Perceptual gain applied to raw RMS so speech fills the meter without clipping.
const LEVEL_GAIN: f32 = 14.0;

/// The captured audio for one utterance.
pub struct CaptureResult {
    /// Mono samples at `sample_rate` (device-native; resample before ASR).
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl CaptureResult {
    pub fn duration_secs(&self) -> f32 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.samples.len() as f32 / self.sample_rate as f32
        }
    }
}

/// A running capture. Drop or [`stop`](Self::stop) to end it.
pub struct CaptureHandle {
    stop_tx: Sender<()>,
    join: Option<JoinHandle<Option<CaptureResult>>>,
}

impl CaptureHandle {
    /// Stop capture and return the accumulated audio (None if the device failed).
    pub fn stop(mut self) -> Option<CaptureResult> {
        let _ = self.stop_tx.send(());
        self.join.take().and_then(|h| h.join().ok().flatten())
    }
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        // If dropped without an explicit stop, still end the capture thread.
        let _ = self.stop_tx.send(());
        if let Some(h) = self.join.take() {
            let _ = h.join();
        }
    }
}

/// Start capturing from the default input device.
///
/// `on_bars` is called ~30x/second with `WAVE_BARS` RMS levels in `[0, 1]`
/// (oldest→newest), from the audio thread. Returns once the stream is playing (so
/// a microphone-permission failure surfaces here, not silently).
pub fn start<F>(on_bars: F) -> anyhow::Result<CaptureHandle>
where
    F: Fn(&[f32]) + Send + 'static,
{
    let (stop_tx, stop_rx) = channel::<()>();
    let (ready_tx, ready_rx) = channel::<anyhow::Result<()>>();

    let join = std::thread::spawn(move || -> Option<CaptureResult> {
        let host = cpal::default_host();
        let device = match host.default_input_device() {
            Some(d) => d,
            None => {
                let _ = ready_tx.send(Err(anyhow::anyhow!("no default input device")));
                return None;
            }
        };
        let supported = match device.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                let _ = ready_tx.send(Err(anyhow::anyhow!("no default input config: {e}")));
                return None;
            }
        };

        let sample_format = supported.sample_format();
        let sample_rate = supported.sample_rate().0;
        let channels = supported.channels().max(1) as usize;
        let config = supported.config();

        let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
        let buf_cb = buffer.clone();

        let mut ring: VecDeque<f32> = VecDeque::from(vec![0.0f32; WAVE_BARS]);
        let mut last_emit = Instant::now();
        let err_fn = |e| eprintln!("[whimpr-audio] stream error: {e}");

        // Only the common f32 input format is handled for now; other formats error
        // out clearly rather than capturing silence.
        let stream = match sample_format {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config,
                move |data: &[f32], _| {
                    let frames = data.len() / channels;
                    let mut sumsq = 0.0f32;
                    {
                        let mut buf = buf_cb.lock().unwrap();
                        buf.reserve(frames);
                        for f in 0..frames {
                            let mut acc = 0.0f32;
                            for c in 0..channels {
                                acc += data[f * channels + c];
                            }
                            let mono = acc / channels as f32;
                            buf.push(mono);
                            sumsq += mono * mono;
                        }
                    }
                    if last_emit.elapsed() >= EMIT_INTERVAL {
                        last_emit = Instant::now();
                        let rms = if frames > 0 {
                            (sumsq / frames as f32).sqrt()
                        } else {
                            0.0
                        };
                        let level = (rms * LEVEL_GAIN).clamp(0.0, 1.0);
                        ring.pop_front();
                        ring.push_back(level);
                        let bars: Vec<f32> = ring.iter().copied().collect();
                        on_bars(&bars);
                    }
                },
                err_fn,
                None,
            ),
            other => {
                let _ = ready_tx.send(Err(anyhow::anyhow!("unsupported sample format {other:?}")));
                return None;
            }
        };

        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                let _ = ready_tx.send(Err(anyhow::anyhow!("failed to build input stream: {e}")));
                return None;
            }
        };
        if let Err(e) = stream.play() {
            let _ = ready_tx.send(Err(anyhow::anyhow!("failed to start stream: {e}")));
            return None;
        }
        let _ = ready_tx.send(Ok(()));

        // Keep the stream alive on this thread until asked to stop.
        let _ = stop_rx.recv();
        drop(stream);

        let samples = std::mem::take(&mut *buffer.lock().unwrap());
        Some(CaptureResult {
            samples,
            sample_rate,
        })
    });

    match ready_rx.recv() {
        Ok(Ok(())) => Ok(CaptureHandle {
            stop_tx,
            join: Some(join),
        }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(anyhow::anyhow!("capture thread exited before starting")),
    }
}

/// Resample mono `input` from `src_rate` to 16 kHz (what ASR models expect) using
/// linear interpolation. Adequate for speech recognition; a polyphase resampler is
/// a later refinement. Returns `input` unchanged when already at 16 kHz.
pub fn resample_to_16k(input: &[f32], src_rate: u32) -> Vec<f32> {
    const DST: u32 = 16_000;
    if src_rate == DST || src_rate == 0 || input.is_empty() {
        return input.to_vec();
    }
    let ratio = DST as f64 / src_rate as f64;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 / ratio;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;
        let a = input.get(idx).copied().unwrap_or(0.0);
        let b = input.get(idx + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
}

/// Milliseconds of 16 kHz audio that sound like a voice: 30 ms frames whose
/// loudness clearly rises above this recording's own noise floor. A key click
/// or room hum stays below; real speech, even quiet, counts. Used to skip
/// transcription entirely when nothing was said (Whisper invents "Thank you"
/// on silence).
pub fn speech_ms(pcm16k: &[f32]) -> u32 {
    const FRAME: usize = 480; // 30 ms at 16 kHz
    let mut rms: Vec<f32> = pcm16k
        .chunks(FRAME)
        .filter(|c| c.len() == FRAME)
        .map(|c| (c.iter().map(|s| s * s).sum::<f32>() / FRAME as f32).sqrt())
        .collect();
    if rms.is_empty() {
        return 0;
    }
    let loud: Vec<f32> = rms.clone();
    rms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    // Quietest fifth of the recording = background noise.
    let floor = rms[rms.len() / 5];
    // Capped, so a recording that is speech from start to end (no quiet part
    // to measure the floor) never pushes the bar above normal voice levels.
    let threshold = (floor * 4.0).clamp(0.006, 0.02);
    loud.iter().filter(|&&r| r > threshold).count() as u32 * 30
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(ms: usize, amp: f32) -> Vec<f32> {
        (0..ms * 16).map(|i| amp * (i as f32 * 0.07).sin()).collect()
    }

    #[test]
    fn silence_and_a_click_are_not_speech() {
        let mut pcm = vec![0.001f32; 16_000 * 2];
        // A 20 ms key click in the middle.
        for s in &mut pcm[16_000..16_320] {
            *s = 0.3;
        }
        assert!(speech_ms(&pcm) < 300);
    }

    #[test]
    fn continuous_speech_without_pauses_counts() {
        assert!(speech_ms(&tone(1500, 0.08)) >= 1200);
    }

    #[test]
    fn quiet_speech_counts() {
        let mut pcm = vec![0.001f32; 16_000];
        pcm.extend(tone(800, 0.03));
        pcm.extend(vec![0.001f32; 8_000]);
        assert!(speech_ms(&pcm) >= 600);
    }

    #[test]
    fn resample_48k_to_16k_thirds_the_length() {
        let input = vec![0.0f32; 48_000];
        let out = resample_to_16k(&input, 48_000);
        assert!((out.len() as i64 - 16_000).abs() <= 1);
    }

    #[test]
    fn resample_noop_at_16k() {
        let input = vec![0.1f32, 0.2, 0.3];
        assert_eq!(resample_to_16k(&input, 16_000), input);
    }
}
