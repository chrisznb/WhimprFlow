// Typed wrappers over the Tauri command surface. In a plain browser (vite dev
// without the shell) the invoke import fails and we fall back to defaults so the
// Hub still renders for iteration.

export type CleanupMode = "raw" | "local" | "open_ai" | "anthropic";
export type CleanupLevel = "none" | "light" | "medium" | "high";
export type AsrMode = "local" | "cloud";
export type StyleLevel = "formal" | "casual" | "very_casual";

export interface StylePrefs {
  personal: StyleLevel;
  work: StyleLevel;
  email: StyleLevel;
  other: StyleLevel;
}

export interface Settings {
  cleanup_mode: CleanupMode;
  cleanup_level: CleanupLevel;
  openai_model: string;
  // API root for "OpenAI" mode — leave blank for OpenAI itself, or point at
  // an OpenAI-compatible endpoint like OpenRouter (https://openrouter.ai/api/v1).
  openai_base_url: string;
  anthropic_model: string;
  sound_on_start: boolean;
  context_awareness: boolean;
  mute_music_while_dictating: boolean;
  style: StylePrefs;
  asr_mode: AsrMode;
  asr_base_url: string;
  asr_model: string;
  dictation_hotkey: string;
}

export interface Status {
  accessibility: boolean;
  microphone: boolean;
  input_monitoring: boolean;
  has_openai_key: boolean;
  has_anthropic_key: boolean;
}

export interface StatsSummary {
  total_words: number;
  total_sessions: number;
  total_speaking_secs: number;
  avg_wpm: number;
  best_wpm: number;
  words_today: number;
  wpm_today: number;
  day_streak: number;
  time_saved_secs: number;
  last7_words: number[];
}

export const EMPTY_STATS: StatsSummary = {
  total_words: 0,
  total_sessions: 0,
  total_speaking_secs: 0,
  avg_wpm: 0,
  best_wpm: 0,
  words_today: 0,
  wpm_today: 0,
  day_streak: 0,
  time_saved_secs: 0,
  last7_words: [0, 0, 0, 0, 0, 0, 0],
};

export const DEFAULT_SETTINGS: Settings = {
  cleanup_mode: "open_ai",
  cleanup_level: "light",
  openai_model: "gpt-4o-mini",
  openai_base_url: "",
  anthropic_model: "claude-haiku-4-5",
  sound_on_start: true,
  context_awareness: true,
  mute_music_while_dictating: true,
  style: { personal: "casual", work: "formal", email: "formal", other: "casual" },
  asr_mode: "local",
  asr_base_url: "https://api.mistral.ai/v1",
  asr_model: "voxtral-mini-latest",
  dictation_hotkey: "",
};

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

export async function getSettings(): Promise<Settings> {
  try {
    return await invoke<Settings>("get_settings");
  } catch {
    return DEFAULT_SETTINGS;
  }
}

export async function setSettings(settings: Settings): Promise<void> {
  try {
    await invoke<void>("set_settings", { settings });
  } catch {
    /* browser preview — no-op */
  }
}

export async function getStatus(): Promise<Status> {
  try {
    return await invoke<Status>("get_status");
  } catch {
    // Plain-browser (vite dev) fallback: pretend permissions are granted so the
    // Hub renders past the onboarding gate for UI iteration.
    return {
      accessibility: true,
      microphone: true,
      input_monitoring: true,
      has_openai_key: false,
      has_anthropic_key: false,
    };
  }
}

export async function getStats(): Promise<StatsSummary> {
  try {
    const tz = new Date().getTimezoneOffset(); // minutes to add to local -> UTC
    return await invoke<StatsSummary>("get_stats", { tzOffsetMinutes: tz });
  } catch {
    return EMPTY_STATS;
  }
}

export async function requestMicrophone(): Promise<void> {
  try {
    await invoke<void>("request_microphone");
  } catch {
    /* browser preview */
  }
}

export async function requestAccessibility(): Promise<void> {
  try {
    await invoke<void>("request_accessibility");
  } catch {
    /* browser preview */
  }
}

export async function requestInputMonitoring(): Promise<void> {
  try {
    await invoke<void>("request_input_monitoring");
  } catch {
    /* browser preview */
  }
}

/// Returns null on success, an error message on failure (so the UI can show
/// why a key did NOT get stored instead of pretending it did).
export async function setApiKey(provider: "openai" | "anthropic", key: string): Promise<string | null> {
  let inv: typeof invoke;
  try {
    const mod = await import("@tauri-apps/api/core");
    inv = mod.invoke;
  } catch {
    return null; // browser preview
  }
  try {
    await inv<void>("set_api_key", { provider, key });
    return null;
  } catch (e) {
    return String(e);
  }
}

// ── History ────────────────────────────────────────────────────────────────
export interface HistoryItem {
  ts_unix: number;
  text: string;
  app: string | null;
  words: number;
}

export async function getHistory(): Promise<HistoryItem[]> {
  try {
    return await invoke<HistoryItem[]>("get_history");
  } catch {
    return [];
  }
}

// ── Dictionary ───────────────────────────────────────────────────────────────
export interface DictEntry {
  correct: string;
  mishears: string[];
  auto: boolean;
}

export async function getDictionary(): Promise<DictEntry[]> {
  try {
    return await invoke<DictEntry[]>("get_dictionary");
  } catch {
    return [];
  }
}

export async function addDictionaryEntry(correct: string, mishears: string[]): Promise<void> {
  try {
    await invoke<void>("add_dictionary_entry", { correct, mishears });
  } catch {
    /* browser preview — no-op */
  }
}

export async function removeDictionaryEntry(correct: string): Promise<void> {
  try {
    await invoke<void>("remove_dictionary_entry", { correct });
  } catch {
    /* browser preview — no-op */
  }
}


// ── Snippets ────────────────────────────────────────────────────────────────
export interface Snippet {
  trigger: string;
  replacement: string;
}

export async function getSnippets(): Promise<Snippet[]> {
  try {
    return await invoke<Snippet[]>("get_snippets");
  } catch {
    return [
      { trigger: "my email address", replacement: "chris@example.com" },
    ];
  }
}

export async function addSnippet(trigger: string, replacement: string): Promise<void> {
  try {
    await invoke<void>("add_snippet", { trigger, replacement });
  } catch {
    /* browser preview — no-op */
  }
}

export async function removeSnippet(trigger: string): Promise<void> {
  try {
    await invoke<void>("remove_snippet", { trigger });
  } catch {
    /* browser preview — no-op */
  }
}

// ── Scratchpad ──────────────────────────────────────────────────────────────
export async function getScratchpad(): Promise<string> {
  try {
    return await invoke<string>("get_scratchpad");
  } catch {
    return "";
  }
}

export async function setScratchpad(text: string): Promise<void> {
  try {
    await invoke<void>("set_scratchpad", { text });
  } catch {
    /* browser preview — no-op */
  }
}

// ── Transforms ──────────────────────────────────────────────────────────────
export interface Transform {
  name: string;
  shortcut: string;
  prompt: string;
}

export async function getTransforms(): Promise<Transform[]> {
  try {
    return await invoke<Transform[]>("get_transforms");
  } catch {
    return [
      { name: "Polish", shortcut: "Alt+1", prompt: "Improve clarity and conciseness." },
      { name: "Prompt Engineer", shortcut: "Alt+2", prompt: "Rewrite as a structured AI prompt." },
      { name: "Organize", shortcut: "Alt+3", prompt: "Organize these thoughts clearly." },
    ];
  }
}

export async function setTransforms(list: Transform[]): Promise<void> {
  try {
    await invoke<void>("set_transforms", { list });
  } catch {
    /* browser preview — no-op */
  }
}

// ── Voice profile ───────────────────────────────────────────────────────────
export interface VoiceProfile {
  profile_text: string;
  catchphrase: string;
  most_used_word: string;
  most_corrected_word: string;
  peak_time: string;
  generated_at_words: number;
  total_words: number;
  regen_after_words: number;
}

export async function getVoiceProfile(force = false): Promise<VoiceProfile | null> {
  try {
    return await invoke<VoiceProfile>("get_voice_profile", { force });
  } catch {
    return null;
  }
}

// ── Assistant ───────────────────────────────────────────────────────────────
export interface AssistantResult {
  reply: string;
  actions_done: string[];
}

export async function assistantChat(history: [string, string][]): Promise<AssistantResult | null> {
  try {
    return await invoke<AssistantResult>("assistant_chat", { history });
  } catch {
    return null;
  }
}

export async function importContacts(): Promise<{ added: number } | { error: string }> {
  try {
    const added = await invoke<number>("import_contacts");
    return { added };
  } catch (e) {
    return { error: String(e) };
  }
}

export interface SnippetSuggestion {
  phrase: string;
  count: number;
}

export async function getSnippetSuggestions(): Promise<SnippetSuggestion[]> {
  try {
    return await invoke<SnippetSuggestion[]>("get_snippet_suggestions");
  } catch {
    return [];
  }
}

// ── File transcription ──────────────────────────────────────────────────────
export async function transcribeFile(path: string): Promise<{ text: string } | { error: string }> {
  try {
    const text = await invoke<string>("transcribe_file", { path });
    return { text };
  } catch (e) {
    return { error: String(e) };
  }
}

export async function chooseAudioFile(): Promise<string | null> {
  try {
    return await invoke<string | null>("choose_audio_file");
  } catch {
    return null;
  }
}
