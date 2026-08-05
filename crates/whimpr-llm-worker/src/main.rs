//! Local-LLM cleanup worker.
//!
//! Loads a GGUF instruction model once, then serves one request per line of stdin:
//! `{"system": "...", "user": "..."}` → `{"text": "..."}` on stdout. The WhimprFlow
//! app spawns this and keeps it warm so cleanup is fast and fully offline.
//!
//! The context is persistent and the KV cache is reused across requests: the
//! shared prompt prefix (system + few-shot demonstrations, ~1.5k tokens) is
//! prefilled once, and each request only decodes the tokens past the longest
//! common prefix with the previous one. That turns per-utterance prefill from
//! seconds into tens of milliseconds.
//!
//! Usage: `whimpr-llm-worker <model.gguf>` (or WHIMPR_LLM_MODEL env var).

use std::io::{BufRead, Write};
use std::num::NonZeroU32;
use std::time::Instant;

use anyhow::Context as _;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use serde::{Deserialize, Serialize};

const N_CTX: u32 = 4096;

#[derive(Deserialize)]
struct Msg {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct Request {
    /// Full multi-turn message list (system + few-shot + user). Preferred.
    #[serde(default)]
    messages: Vec<Msg>,
    /// Back-compat single-turn form, used only when `messages` is empty.
    #[serde(default)]
    system: String,
    #[serde(default)]
    user: String,
    #[serde(default = "default_max")]
    max_tokens: i32,
}
fn default_max() -> i32 {
    400
}

#[derive(Serialize)]
struct Response {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let model_path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("WHIMPR_LLM_MODEL").ok())
        .context("model path required (argv[1] or WHIMPR_LLM_MODEL)")?;

    let backend = LlamaBackend::init()?;
    // Offload everything to the Apple GPU (Metal) — capped by what fits.
    let model_params = LlamaModelParams::default().with_n_gpu_layers(999);
    let model = LlamaModel::load_from_file(&backend, &model_path, &model_params)
        .with_context(|| format!("failed to load model {model_path}"))?;

    let ctx_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(N_CTX));
    let mut ctx = model.new_context(&backend, ctx_params)?;
    // Tokens currently sitting in the KV cache (prompt + generated), positions 0..len.
    let mut cached: Vec<LlamaToken> = Vec::new();
    eprintln!("[llm-worker] model loaded, ready");

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Request>(&line) {
            Ok(req) => match generate(&model, &mut ctx, &mut cached, &req) {
                Ok(text) => Response { text, error: None },
                Err(e) => {
                    // A failed decode leaves the cache state unknown — start clean.
                    ctx.clear_kv_cache();
                    cached.clear();
                    Response {
                        text: String::new(),
                        error: Some(e.to_string()),
                    }
                }
            },
            Err(e) => Response {
                text: String::new(),
                error: Some(format!("bad request: {e}")),
            },
        };
        serde_json::to_writer(&mut stdout, &resp)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
    }
    Ok(())
}

fn generate(
    model: &LlamaModel,
    ctx: &mut LlamaContext,
    cached: &mut Vec<LlamaToken>,
    req: &Request,
) -> anyhow::Result<String> {
    // Qwen2.5 ChatML template. Prefer the full multi-turn message list (few-shot
    // demonstrations drive the newline/list/self-correction behavior); fall back
    // to the legacy single system+user pair.
    let mut prompt = String::new();
    if req.messages.is_empty() {
        prompt.push_str(&format!(
            "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n",
            req.system, req.user
        ));
    } else {
        for m in &req.messages {
            prompt.push_str(&format!("<|im_start|>{}\n{}<|im_end|>\n", m.role, m.content));
        }
    }
    prompt.push_str("<|im_start|>assistant\n");

    let tokens = model.str_to_token(&prompt, AddBos::Always)?;
    let n_prompt = tokens.len() as i32;

    if n_prompt + req.max_tokens >= N_CTX as i32 {
        anyhow::bail!("prompt too long: {n_prompt} tokens");
    }

    // Longest common prefix with what is already in the KV cache. At least the
    // final prompt token must be re-decoded so we have fresh logits to sample from.
    let mut common = 0usize;
    while common < cached.len() && common < tokens.len() && cached[common] == tokens[common] {
        common += 1;
    }
    common = common.min(tokens.len() - 1);
    ctx.clear_kv_cache_seq(Some(0), Some(common as u32), None)
        .map_err(|e| anyhow::anyhow!("kv cache clear: {e}"))?;

    let t0 = Instant::now();
    let mut batch = LlamaBatch::new(N_CTX as usize, 1);
    let last = tokens.len() - 1;
    for (i, tok) in tokens.iter().enumerate().skip(common) {
        batch.add(*tok, i as i32, &[0], i == last)?;
    }
    ctx.decode(&mut batch)?;
    let prefill_ms = t0.elapsed().as_millis();

    let t1 = Instant::now();
    let mut sampler = LlamaSampler::greedy();
    let mut n_cur = n_prompt;
    let mut generated: Vec<LlamaToken> = Vec::new();
    let mut out = String::new();
    let limit = n_prompt + req.max_tokens;

    while n_cur <= limit {
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        sampler.accept(token);
        if model.is_eog_token(token) {
            break;
        }
        out.push_str(&model.token_to_str(token, Special::Tokenize)?);
        generated.push(token);
        batch.clear();
        batch.add(token, n_cur, &[0], true)?;
        n_cur += 1;
        ctx.decode(&mut batch)?;
    }

    // Remember what the KV cache now holds for the next request's prefix match.
    cached.clear();
    cached.extend_from_slice(&tokens);
    cached.extend_from_slice(&generated);

    eprintln!(
        "[llm-worker] reused {common}/{n_prompt} prompt tokens, prefill {prefill_ms}ms, \
         gen {} tok in {}ms",
        generated.len(),
        t1.elapsed().as_millis()
    );
    Ok(out.trim().to_string())
}
