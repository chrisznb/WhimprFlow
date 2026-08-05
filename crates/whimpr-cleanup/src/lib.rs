//! Cloud cleanup providers. The OpenAI provider (default cloud) sends the shared
//! WhimprFlow system prompt plus the assembled context and returns cleaned text.
//! On any failure the caller falls back to the raw transcript — cleanup is an
//! enhancement, never a gate.

use std::time::Duration;

use whimpr_core::cleanup::{build_messages, CleanupContext, CleanupProvider, ProviderId};

/// Default OpenAI Chat Completions endpoint.
const OPENAI_DEFAULT_URL: &str = "https://api.openai.com/v1/chat/completions";

/// Cleanup via the OpenAI Chat Completions API — or any OpenAI-compatible
/// endpoint (OpenRouter, a local server, etc.) when `base_url` is set.
/// OpenRouter in particular speaks this exact wire format at
/// `https://openrouter.ai/api/v1/chat/completions`.
pub struct OpenAiProvider {
    client: reqwest::blocking::Client,
    api_key: String,
    model: String,
    /// Full chat-completions URL. Defaults to OpenAI's when empty.
    url: String,
}

impl OpenAiProvider {
    pub fn new(api_key: String, model: impl Into<String>) -> Self {
        Self::with_base_url(api_key, model, None)
    }

    /// `base_url` is the API root (e.g. `https://openrouter.ai/api/v1`), without
    /// the `/chat/completions` suffix. `None` or empty uses OpenAI directly.
    pub fn with_base_url(
        api_key: String,
        model: impl Into<String>,
        base_url: Option<String>,
    ) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("failed to build HTTP client");
        let url = match base_url.map(|s| s.trim().trim_end_matches('/').to_string()) {
            Some(base) if !base.is_empty() => format!("{base}/chat/completions"),
            _ => OPENAI_DEFAULT_URL.to_string(),
        };
        Self {
            client,
            api_key,
            model: model.into(),
            url,
        }
    }
}

impl CleanupProvider for OpenAiProvider {
    fn id(&self) -> ProviderId {
        ProviderId::OpenAi
    }

    fn cleanup(&self, raw: &str, ctx: &CleanupContext) -> anyhow::Result<String> {
        // System prompt + few-shot demonstration turns + the real transcript.
        let messages: Vec<serde_json::Value> = build_messages(raw, ctx)
            .into_iter()
            .map(|m| serde_json::json!({ "role": m.role, "content": m.content }))
            .collect();
        let body = serde_json::json!({
            "model": self.model,
            "temperature": 0.2,
            "max_tokens": 512,
            "messages": messages,
        });

        let resp = self
            .client
            .post(&self.url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()?;

        let status = resp.status();
        if !status.is_success() {
            let detail = resp.text().unwrap_or_default();
            anyhow::bail!("OpenAI HTTP {status}: {detail}");
        }

        let v: serde_json::Value = resp.json()?;
        let text = v["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();
        if text.is_empty() {
            anyhow::bail!("OpenAI returned empty content");
        }
        Ok(text)
    }
}

/// Cleanup via the Anthropic Messages API. Same shared system prompt; the only
/// difference from OpenAI is the wire envelope (top-level `system`, `x-api-key`).
pub struct AnthropicProvider {
    client: reqwest::blocking::Client,
    api_key: String,
    model: String,
}

impl AnthropicProvider {
    pub fn new(api_key: String, model: impl Into<String>) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            api_key,
            model: model.into(),
        }
    }
}

impl CleanupProvider for AnthropicProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Anthropic
    }

    fn cleanup(&self, raw: &str, ctx: &CleanupContext) -> anyhow::Result<String> {
        // Anthropic takes the system prompt top-level; the few-shot turns and the
        // real transcript go in `messages` as user/assistant turns.
        let mut system = String::new();
        let mut messages: Vec<serde_json::Value> = Vec::new();
        for m in build_messages(raw, ctx) {
            if m.role == "system" {
                system = m.content;
            } else {
                messages.push(serde_json::json!({ "role": m.role, "content": m.content }));
            }
        }
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 512,
            "temperature": 0.2,
            "system": system,
            "messages": messages,
        });

        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()?;

        let status = resp.status();
        if !status.is_success() {
            let detail = resp.text().unwrap_or_default();
            anyhow::bail!("Anthropic HTTP {status}: {detail}");
        }

        let v: serde_json::Value = resp.json()?;
        let text = v["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();
        if text.is_empty() {
            anyhow::bail!("Anthropic returned empty content");
        }
        Ok(text)
    }
}

/// OpenAI-compatible cloud transcription (`POST {base}/audio/transcriptions`):
/// works with Mistral Voxtral, Groq Whisper, and OpenAI. Sends WAV bytes,
/// returns the transcript text.
pub fn transcribe_cloud(
    base_url: &str,
    api_key: &str,
    model: &str,
    wav: Vec<u8>,
) -> anyhow::Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let url = format!("{}/audio/transcriptions", base_url.trim_end_matches('/'));
    let part = reqwest::blocking::multipart::Part::bytes(wav)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;
    let form = reqwest::blocking::multipart::Form::new()
        .text("model", model.to_string())
        .part("file", part);
    let resp = client.post(&url).bearer_auth(api_key).multipart(form).send()?;
    let status = resp.status();
    let body: serde_json::Value = resp.json()?;
    if !status.is_success() {
        anyhow::bail!("cloud ASR {status}: {body}");
    }
    Ok(body
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .trim()
        .to_string())
}

/// Plain OpenAI-compatible chat completion for auxiliary generation (voice
/// profile text etc.) — separate from the strict cleanup providers.
pub fn chat_completion(
    base_url: &str,
    api_key: &str,
    model: &str,
    system: &str,
    user: &str,
) -> anyhow::Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let base = if base_url.trim().is_empty() {
        "https://api.openai.com/v1"
    } else {
        base_url.trim_end_matches('/')
    };
    let resp = client
        .post(format!("{base}/chat/completions"))
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "max_tokens": 400,
        }))
        .send()?;
    let status = resp.status();
    let body: serde_json::Value = resp.json()?;
    if !status.is_success() {
        anyhow::bail!("chat {status}: {body}");
    }
    Ok(body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string())
}

/// Multi-turn variant of [`chat_completion`] for the in-app assistant.
pub fn chat_completion_messages(
    base_url: &str,
    api_key: &str,
    model: &str,
    messages: &[(String, String)],
) -> anyhow::Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()?;
    let base = if base_url.trim().is_empty() {
        "https://api.openai.com/v1"
    } else {
        base_url.trim_end_matches('/')
    };
    let msgs: Vec<serde_json::Value> = messages
        .iter()
        .map(|(role, content)| serde_json::json!({"role": role, "content": content}))
        .collect();
    let resp = client
        .post(format!("{base}/chat/completions"))
        .bearer_auth(api_key)
        .json(&serde_json::json!({ "model": model, "messages": msgs, "max_tokens": 700 }))
        .send()?;
    let status = resp.status();
    let body: serde_json::Value = resp.json()?;
    if !status.is_success() {
        anyhow::bail!("chat {status}: {body}");
    }
    Ok(body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string())
}
