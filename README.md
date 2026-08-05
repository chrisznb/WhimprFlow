# WhimprFlow

A **local-first voice dictation app for macOS**. Tap a key, speak, and clean text lands wherever your cursor is. Speech is transcribed on-device with Whisper and cleaned up by a local LLM (filler removal, self-corrections, spoken punctuation, lists), with optional cloud cleanup.

This is a heavily extended fork of [Blueturboguy07/WhimprFlow](https://github.com/Blueturboguy07/WhimprFlow) (MIT). Not affiliated with, endorsed by, or connected to Wispr Flow or any other product.

## What it does

- **On-device ASR**: Whisper (whisper.cpp, Metal), multilingual with auto language detection. German and English work great, including mixed use.
- **Local LLM cleanup**: Qwen (llama.cpp) removes fillers ("um", "ähm"), resolves spoken self-corrections ("Dienstag, nee warte, Donnerstag"), converts spoken punctuation ("Punkt", "question mark"), formats lists. Persistent KV cache: the prompt prefix is prefilled once, so cleanup is fast.
- **Optional cloud cleanup**: OpenAI, Anthropic, or any OpenAI-compatible API (Mistral, OpenRouter, Groq) via a custom base URL. Keys live in the macOS keychain, never in files. Only the transcript text is sent, never audio.
- **The pill**: a small floating bar above the Dock. Tap your dictation key (Fn) to toggle recording, hold it for push-to-talk, Esc cancels. Hover the pill for a mic button. Drag it anywhere; it snaps to screen edges and corners, and turns vertical on the side edges.
- **Context awareness**: reads the text around your cursor (Accessibility API) so cleanup understands names and what you are replying to. Follow-up dictations within 90 seconds carry the previous one as context. Everything stays on your Mac; toggle in Settings.
- **Snippets**: say a trigger phrase ("my email address"), get the stored replacement.
- **Style**: formal / casual / very casual output per app category (personal chat, work chat, email, other), detected from the app you dictate into.
- **Transforms**: select text anywhere, press Alt+1/2/3, and the selection is rewritten in place (polish, prompt-engineer, organize). Prompts and shortcuts are editable.
- **Scratchpad**: a quiet autosaving page in the hub for long-form dictation.
- **Quality-of-life**: personal dictionary with auto-learn, usage stats and streaks, silence-hallucination filter, smart spacing when dictating mid-text, pauses Spotify/Music while recording, menu-bar-only app with autostart.

## Install

### Option 1: Download

Grab the latest `.zip` from [Releases](../../releases), unzip, and drag `WhimprFlow.app` into `/Applications`.

The app is not notarized, so on first launch macOS will warn you: right-click the app, choose **Open**, then confirm. Grant Accessibility and Microphone when asked.

### Option 2: Build from source

Requires Rust (stable), Node + pnpm, cmake, and the Xcode command-line tools.

```bash
git clone https://github.com/chrisznb/WhimprFlow.git
cd WhimprFlow/ui && pnpm install && cd ..
ui/node_modules/.bin/tauri build --bundles app
cargo build --release -p whimpr-llm-worker
cp target/release/whimpr-llm-worker target/release/bundle/macos/WhimprFlow.app/Contents/MacOS/
cp -R target/release/bundle/macos/WhimprFlow.app /Applications/
```

If you have an Apple Development certificate, sign the app with it so macOS permissions survive rebuilds:

```bash
codesign --force --deep -s "Apple Development: Your Name (TEAMID)" /Applications/WhimprFlow.app
```

### Models

Models are not bundled (multi-GB). Put them in `~/Library/Application Support/WhimprFlow/models/`:

```bash
mkdir -p ~/Library/Application\ Support/WhimprFlow/models
cd ~/Library/Application\ Support/WhimprFlow/models
# Whisper (multilingual, quantized, 547 MB)
curl -LO https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin
# Local cleanup LLM (2.4 GB) — optional if you use a cloud engine
curl -L -o qwen3-4b-instruct-2507-q4_k_m.gguf https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen3-4B-Instruct-2507-Q4_K_M.gguf
```

No local LLM? Set the cleanup engine to OpenAI in Settings and point the base URL at any OpenAI-compatible API.

## Privacy

ASR and default cleanup run on-device. Cloud cleanup is opt-in and sends only the transcript text to the provider you choose. Context awareness reads the focused text field locally and never leaves your Mac. API keys are stored in the macOS keychain.

## License

MIT, see [LICENSE](LICENSE). Based on [Blueturboguy07/WhimprFlow](https://github.com/Blueturboguy07/WhimprFlow).
