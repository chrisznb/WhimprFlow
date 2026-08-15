import { useState } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Button, Card, Dot, PageTitle, Segmented } from "./ui";
import { ModelDownload, useModelStatus } from "./models";
import {
  requestAccessibility,
  requestInputMonitoring,
  requestMicrophone,
  setApiKey,
  type CleanupLevel,
  type CleanupMode,
  type Settings,
  type Status,
} from "./api";

const MODES: { value: CleanupMode; label: string; hint: string }[] = [
  { value: "raw", label: "Raw", hint: "Paste exactly what you said" },
  { value: "local", label: "Local", hint: "On-device model (offline)" },
  { value: "open_ai", label: "OpenAI", hint: "Cloud cleanup via OpenAI (or an OpenAI-compatible API like OpenRouter, set the base URL below)" },
  { value: "anthropic", label: "Anthropic", hint: "Cloud cleanup via Claude" },
];

const LEVELS: { value: CleanupLevel; label: string; hint: string }[] = [
  { value: "none", label: "None", hint: "Transcribe exactly what you said, including mistakes." },
  { value: "light", label: "Light", hint: "Clean up filler words and grammar. (Recommended)" },
  { value: "medium", label: "Medium", hint: "Edit for clarity and conciseness." },
  { value: "high", label: "High", hint: "Rewrite for brevity and polish." },
];

function SectionTitle({ children, sub }: { children: React.ReactNode; sub?: string }) {
  return (
    <div style={{ marginBottom: 14 }}>
      <div style={{ fontSize: 15, fontWeight: 600, color: theme.textStrong }}>{children}</div>
      {sub && <div style={{ color: theme.textMuted, fontSize: 13, marginTop: 4 }}>{sub}</div>}
    </div>
  );
}

function KeyField({
  label,
  configured,
  onSave,
}: {
  label: string;
  configured: boolean;
  onSave: (key: string) => Promise<string | null>;
}) {
  const [value, setValue] = useState("");
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  return (
    <div style={{ marginTop: 16 }}>
      <div style={{ fontSize: 13, marginBottom: 7, display: "flex", alignItems: "center", color: theme.textBody }}>
        <Dot ok={configured} />
        {label} {configured ? "(configured)" : "(not set)"}
      </div>
      <div style={{ display: "flex", gap: 8 }}>
        <input
          type="password"
          value={value}
          placeholder={configured ? "Enter a new key to replace" : "Paste your API key"}
          onChange={(e) => {
            setValue(e.target.value);
            setSaved(false);
          }}
          style={{
            flex: 1,
            background: theme.cardBgSubtle,
            border: `1px solid ${theme.border}`,
            borderRadius: 10,
            padding: "9px 12px",
            color: theme.textBody,
            fontFamily: font.mono,
            fontSize: 13,
            outline: "none",
          }}
        />
        <Button
          onClick={() => {
            setError(null);
            setSaved(false);
            void onSave(value).then((err) => {
              if (err) {
                setError(err);
              } else {
                setValue("");
                setSaved(true);
              }
            });
          }}
        >
          Save
        </Button>
      </div>
      {saved && <div style={{ fontSize: 12, color: theme.accentDeep, marginTop: 6 }}>Saved to keychain</div>}
      {error && (
        <div style={{ fontSize: 12, color: "#C0392B", marginTop: 6 }}>
          Not saved: {error}
        </div>
      )}
    </div>
  );
}

function PermRow({
  ok,
  label,
  detail,
  onClick,
}: {
  ok: boolean;
  label: string;
  detail: string;
  onClick: () => void;
}) {
  return (
    <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
      <div style={{ display: "flex", alignItems: "center", fontSize: 13 }}>
        <Dot ok={ok} />
        <span style={{ color: theme.textBody }}>
          <b>{label}</b> <span style={{ color: theme.textMuted }}>{detail}</span>
        </span>
      </div>
      {ok ? (
        <span style={{ color: theme.accentDeep, fontSize: 13, fontWeight: 600 }}>Granted</span>
      ) : (
        <Button variant="ghost" size="sm" onClick={onClick}>
          Grant
        </Button>
      )}
    </div>
  );
}

export function SettingsPane({
  settings,
  onChange,
  status,
  refresh,
}: {
  settings: Settings;
  onChange: (s: Settings) => void;
  status: Status;
  refresh: () => void;
}) {
  return (
    <div style={{ maxWidth: 720 }}>
      <PageTitle serif>Settings</PageTitle>

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle sub="Fn always works: hold to talk, tap to toggle. Add a second key here if you like (accelerator syntax, e.g. F13 or Ctrl+Space).">
          Dictation hotkey
        </SectionTitle>
        <input
          type="text"
          value={settings.dictation_hotkey}
          placeholder="e.g. F13 or Ctrl+Space (empty = Fn only)"
          onChange={(e) => onChange({ ...settings, dictation_hotkey: e.target.value })}
          style={{
            width: "100%",
            maxWidth: 320,
            background: theme.cardBgSubtle,
            border: `1px solid ${theme.border}`,
            borderRadius: 10,
            padding: "9px 12px",
            color: theme.textBody,
            fontFamily: font.mono,
            fontSize: 13,
            outline: "none",
            boxSizing: "border-box",
          }}
        />
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle sub="Where your speech is turned into text.">Transcription</SectionTitle>
        <Segmented
          options={[
            { value: "local", label: "Local (Whisper)" },
            { value: "cloud", label: "Cloud" },
          ]}
          value={settings.asr_mode}
          onChange={(v) => onChange({ ...settings, asr_mode: v })}
        />
        <div style={{ color: theme.textMuted, fontSize: 12.5, marginTop: 10 }}>
          {settings.asr_mode === "local"
            ? "On-device whisper.cpp, fully offline."
            : "OpenAI-compatible audio API (Mistral Voxtral, Groq Whisper, OpenAI). Uses the OpenAI API key below. Falls back to local if the request fails."}
        </div>
        {settings.asr_mode === "cloud" && (
          <div style={{ marginTop: 12, display: "flex", gap: 8 }}>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 12.5, color: theme.textMuted, marginBottom: 6 }}>Base URL</div>
              <input
                type="text"
                value={settings.asr_base_url}
                onChange={(e) => onChange({ ...settings, asr_base_url: e.target.value })}
                style={{
                  width: "100%",
                  background: theme.cardBgSubtle,
                  border: `1px solid ${theme.border}`,
                  borderRadius: 10,
                  padding: "9px 12px",
                  color: theme.textBody,
                  fontFamily: font.mono,
                  fontSize: 13,
                  outline: "none",
                  boxSizing: "border-box",
                }}
              />
            </div>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 12.5, color: theme.textMuted, marginBottom: 6 }}>Model</div>
              <input
                type="text"
                value={settings.asr_model}
                onChange={(e) => onChange({ ...settings, asr_model: e.target.value })}
                placeholder="voxtral-mini-latest"
                style={{
                  width: "100%",
                  background: theme.cardBgSubtle,
                  border: `1px solid ${theme.border}`,
                  borderRadius: 10,
                  padding: "9px 12px",
                  color: theme.textBody,
                  fontFamily: font.mono,
                  fontSize: 13,
                  outline: "none",
                  boxSizing: "border-box",
                }}
              />
            </div>
          </div>
        )}
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle sub="Where your dictation is cleaned up before it's typed.">Cleanup Engine</SectionTitle>
        <Segmented
          options={MODES.map((m) => ({ value: m.value, label: m.label }))}
          value={settings.cleanup_mode}
          onChange={(v) => onChange({ ...settings, cleanup_mode: v })}
        />
        <div style={{ color: theme.textMuted, fontSize: 12.5, marginTop: 10 }}>
          {MODES.find((m) => m.value === settings.cleanup_mode)?.hint}
        </div>

        <KeyField
          label="OpenAI API key"
          configured={status.has_openai_key}
          onSave={async (k) => {
            const err = await setApiKey("openai", k);
            setTimeout(refresh, 400);
            return err;
          }}
        />
        <div style={{ marginTop: 12, display: "flex", gap: 8 }}>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12.5, color: theme.textMuted, marginBottom: 6 }}>
              Base URL (blank = OpenAI; e.g. https://openrouter.ai/api/v1 for OpenRouter)
            </div>
            <input
              type="text"
              value={settings.openai_base_url}
              placeholder="https://openrouter.ai/api/v1"
              onChange={(e) => onChange({ ...settings, openai_base_url: e.target.value })}
              style={{
                width: "100%",
                background: theme.cardBgSubtle,
                border: `1px solid ${theme.border}`,
                borderRadius: 10,
                padding: "9px 12px",
                color: theme.textBody,
                fontFamily: font.mono,
                fontSize: 13,
                outline: "none",
                boxSizing: "border-box",
              }}
            />
          </div>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12.5, color: theme.textMuted, marginBottom: 6 }}>
              Model (e.g. an OpenRouter model slug)
            </div>
            <input
              type="text"
              value={settings.openai_model}
              placeholder="meta-llama/llama-3.3-70b-instruct:free"
              onChange={(e) => onChange({ ...settings, openai_model: e.target.value })}
              style={{
                width: "100%",
                background: theme.cardBgSubtle,
                border: `1px solid ${theme.border}`,
                borderRadius: 10,
                padding: "9px 12px",
                color: theme.textBody,
                fontFamily: font.mono,
                fontSize: 13,
                outline: "none",
                boxSizing: "border-box",
              }}
            />
          </div>
        </div>
        <KeyField
          label="Anthropic API key"
          configured={status.has_anthropic_key}
          onSave={async (k) => {
            const err = await setApiKey("anthropic", k);
            setTimeout(refresh, 400);
            return err;
          }}
        />
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle>Auto Cleanup</SectionTitle>
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          {LEVELS.map((l) => {
            const selected = settings.cleanup_level === l.value;
            return (
              <button
                key={l.value}
                onClick={() => onChange({ ...settings, cleanup_level: l.value })}
                style={{
                  textAlign: "left",
                  cursor: "pointer",
                  borderRadius: 12,
                  padding: "12px 14px",
                  fontFamily: font.ui,
                  background: selected ? theme.accentSoft : theme.cardBgSubtle,
                  border: `1px solid ${selected ? theme.accentSoftBorder : theme.border}`,
                  color: theme.textBody,
                }}
              >
                <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>{l.label}</div>
                <div style={{ fontSize: 12.5, color: theme.textMuted, marginTop: 2 }}>{l.hint}</div>
              </button>
            );
          })}
        </div>
      </Card>

      <ModelsCard />

      <Card style={{ marginBottom: 16 }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
          <div>
            <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>Your typing speed</div>
            <div style={{ fontSize: 12.5, color: theme.textMuted, marginTop: 2 }}>
              Words per minute when you type. Used for the "time saved" stat.
            </div>
          </div>
          <input
            type="number"
            min={10}
            max={200}
            value={settings.typing_wpm || ""}
            onChange={(e) => {
              const n = Math.round(Number(e.target.value));
              onChange({ ...settings, typing_wpm: Number.isFinite(n) && n > 0 ? Math.min(200, n) : 0 });
            }}
            onBlur={() => {
              if (!settings.typing_wpm || settings.typing_wpm < 10) {
                onChange({ ...settings, typing_wpm: 45 });
              }
            }}
            style={{
              width: 74,
              fontFamily: font.ui,
              fontSize: 14,
              color: theme.textBody,
              background: theme.cardBgSubtle,
              border: `1px solid ${theme.border}`,
              borderRadius: 9,
              padding: "8px 10px",
              outline: "none",
              textAlign: "center",
            }}
          />
        </div>
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
          <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>
            Play a sound when recording starts
          </div>
          <Segmented
            options={[
              { value: "on", label: "On" },
              { value: "off", label: "Off" },
            ]}
            value={settings.sound_on_start ? "on" : "off"}
            onChange={(v) => onChange({ ...settings, sound_on_start: v === "on" })}
          />
        </div>
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
          <div>
            <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>
              Context awareness
            </div>
            <div style={{ fontSize: 12.5, color: theme.textMuted, marginTop: 3 }}>
              Reads the text around your cursor so cleanup understands names and what you're replying to. Stays on this Mac.
            </div>
          </div>
          <Segmented
            options={[
              { value: "on", label: "On" },
              { value: "off", label: "Off" },
            ]}
            value={settings.context_awareness ? "on" : "off"}
            onChange={(v) => onChange({ ...settings, context_awareness: v === "on" })}
          />
        </div>
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
          <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>
            Mute music while dictating
          </div>
          <Segmented
            options={[
              { value: "on", label: "On" },
              { value: "off", label: "Off" },
            ]}
            value={settings.mute_music_while_dictating ? "on" : "off"}
            onChange={(v) => onChange({ ...settings, mute_music_while_dictating: v === "on" })}
          />
        </div>
      </Card>

      <Card>
        <SectionTitle sub="Grant these to WhimprFlow, then quit and reopen the app if a dot stays grey.">
          Permissions
        </SectionTitle>
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          <PermRow
            ok={status.accessibility}
            label="Accessibility"
            detail={
              status.accessibility
                ? "granted. Fn works everywhere and types your words"
                : "the key one: makes Fn work in EVERY app AND types your words"
            }
            onClick={() => {
              requestAccessibility();
              setTimeout(refresh, 800);
            }}
          />
          <PermRow
            ok={status.microphone}
            label="Microphone"
            detail={status.microphone ? "granted" : "hears what you say"}
            onClick={() => {
              requestMicrophone();
              setTimeout(refresh, 1000);
            }}
          />
          <PermRow
            ok={status.input_monitoring}
            label="Input Monitoring"
            detail="optional, extra reliability for key detection"
            onClick={() => {
              requestInputMonitoring();
              setTimeout(refresh, 1000);
            }}
          />
        </div>
      </Card>
    </div>
  );
}

function ModelRow({
  title,
  detail,
  installed,
  kind,
  sizeLabel,
  onDone,
}: {
  title: string;
  detail: string;
  installed: boolean;
  kind: "asr" | "llm";
  sizeLabel: string;
  onDone: () => void;
}) {
  return (
    <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12, flexWrap: "wrap" }}>
      <div style={{ flex: "1 1 240px", minWidth: 0 }}>
        <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>{title}</div>
        <div style={{ fontSize: 12.5, color: theme.textMuted, marginTop: 2 }}>{detail}</div>
      </div>
      {installed ? (
        <div style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12.5, fontWeight: 600, color: theme.accentDeep }}>
          <Dot ok /> Installed
        </div>
      ) : (
        <ModelDownload kind={kind} sizeLabel={sizeLabel} onDone={onDone} />
      )}
    </div>
  );
}

function ModelsCard() {
  const { status, refresh } = useModelStatus();
  if (status === null) return null;
  return (
    <Card style={{ marginBottom: 16 }}>
      <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong, marginBottom: 14 }}>Models</div>
      <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
        <ModelRow
          title="Speech recognition"
          detail="Whisper large-v3-turbo, runs on your Mac. Required for offline dictation."
          installed={status.asr}
          kind="asr"
          sizeLabel="547 MB"
          onDone={refresh}
        />
        <div style={{ borderTop: `1px solid ${theme.border}` }} />
        <ModelRow
          title="Local cleanup"
          detail="Qwen 3 4B for on-device text cleanup. Optional if cleanup runs in the cloud."
          installed={status.llm}
          kind="llm"
          sizeLabel="2.4 GB"
          onDone={refresh}
        />
      </div>
    </Card>
  );
}
