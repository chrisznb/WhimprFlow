import { availableLangs, t } from "../i18n";
import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Button, Card, Dot, PageTitle, Segmented } from "./ui";
import { ModelDownload, useModelStatus } from "./models";
import { Icon } from "./icons";
import {
  getPillPosition,
  setPillPosition,
  type PillPosition,
  requestAccessibility,
  requestInputMonitoring,
  requestMicrophone,
  setApiKey,
  type CleanupLevel,
  type CleanupMode,
  type Settings,
  type Status,
} from "./api";

const MODES: () => { value: CleanupMode; label: string; hint: string }[] = () => [
  { value: "raw", label: t("set.raw"), hint: t("set.rawHint") },
  { value: "local", label: t("set.local"), hint: t("set.localHint") },
  { value: "open_ai", label: "OpenAI", hint: t("set.openaiHint") },
  { value: "anthropic", label: "Anthropic", hint: t("set.anthropicHint") },
];

const LEVELS: () => { value: CleanupLevel; label: string; hint: string }[] = () => [
  { value: "none", label: t("set.lvlNone"), hint: t("set.lvlNoneHint") },
  { value: "light", label: t("set.lvlLight"), hint: t("set.lvlLightHint") },
  { value: "medium", label: t("set.lvlMedium"), hint: t("set.lvlMediumHint") },
  { value: "high", label: t("set.lvlHigh"), hint: t("set.lvlHighHint") },
];

function SectionTitle({ children, sub }: { children: React.ReactNode; sub?: string }) {
  return (
    <div style={{ marginBottom: 14 }}>
      <div style={{ fontSize: 15, fontWeight: 600, color: theme.textStrong }}>{children}</div>
      {sub && <div style={{ color: theme.textMuted, fontSize: 13, marginTop: 4 }}>{sub}</div>}
    </div>
  );
}

function LangPicker({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const langs = availableLangs();
  const options = [{ value: "system", label: t("set.langSystem") }, ...langs.map((l) => ({ value: l.code, label: l.label }))];
  if (options.length <= 3) {
    return <Segmented options={options} value={value} onChange={onChange} />;
  }
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      style={{
        border: `1px solid ${theme.border}`,
        borderRadius: 8,
        padding: "7px 10px",
        fontSize: 13,
        fontFamily: font.ui,
        color: theme.textStrong,
        background: "#fff",
        cursor: "pointer",
      }}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
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
        {label} {configured ? t("set.configured") : t("set.notSet")}
      </div>
      <div style={{ display: "flex", gap: 8 }}>
        <input
          type="password"
          value={value}
          placeholder={configured ? t("set.keyReplacePh") : t("set.keyPh")}
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
          {t("set.saveKey")}
        </Button>
      </div>
      {saved && <div style={{ fontSize: 12, color: theme.accentDeep, marginTop: 6 }}>{t("set.savedKeychain")}</div>}
      {error && (
        <div style={{ fontSize: 12, color: "#C0392B", marginTop: 6 }}>
          {t("set.notSaved", { e: error })}
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
        <span style={{ color: theme.accentDeep, fontSize: 13, fontWeight: 600 }}>{t("ob.granted")}</span>
      ) : (
        <Button variant="ghost" size="sm" onClick={onClick}>
          {t("ob.grant")}
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
      <PageTitle serif>{t("set.title")}</PageTitle>

      <Card style={{ marginBottom: 16 }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
          <div>
            <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>{t("set.langTitle")}</div>
            <div style={{ fontSize: 12.5, color: theme.textMuted, marginTop: 2 }}>{t("set.langSub")}</div>
          </div>
          <LangPicker
            value={settings.language || "system"}
            onChange={(v) => onChange({ ...settings, language: v })}
          />
        </div>
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle sub={t("set.hotkeySub")}>
          {t("set.hotkeyTitle")}
        </SectionTitle>
        <HotkeyField
          value={settings.dictation_hotkey}
          onPick={(v) => onChange({ ...settings, dictation_hotkey: v })}
        />
      </Card>

      <PillPositionCard />

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle sub={t("set.trSub")}>{t("set.trTitle")}</SectionTitle>
        <Segmented
          options={[
            { value: "local", label: t("set.trLocal") },
            { value: "cloud", label: t("set.trCloud") },
          ]}
          value={settings.asr_mode}
          onChange={(v) => onChange({ ...settings, asr_mode: v })}
        />
        <div style={{ color: theme.textMuted, fontSize: 12.5, marginTop: 10 }}>
          {settings.asr_mode === "local" ? t("set.trLocalHint") : t("set.trCloudHint")}
        </div>
        {settings.asr_mode === "cloud" && (
          <div style={{ marginTop: 12, display: "flex", gap: 8 }}>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 12.5, color: theme.textMuted, marginBottom: 6 }}>{t("set.baseUrl")}</div>
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
              <div style={{ fontSize: 12.5, color: theme.textMuted, marginBottom: 6 }}>{t("set.model")}</div>
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
        <SectionTitle sub={t("set.engineSub")}>{t("set.engineTitle")}</SectionTitle>
        <Segmented
          options={MODES().map((m) => ({ value: m.value, label: m.label }))}
          value={settings.cleanup_mode}
          onChange={(v) => onChange({ ...settings, cleanup_mode: v })}
        />
        <div style={{ color: theme.textMuted, fontSize: 12.5, marginTop: 10 }}>
          {MODES().find((m) => m.value === settings.cleanup_mode)?.hint}
        </div>

        <KeyField
          label={t("set.openaiKey")}
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
              {t("set.baseUrlHint")}
              <div style={{ marginTop: 4, color: theme.textFaint }}>{t("set.localServerHint")}</div>
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
              {t("set.modelHint")}
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
          label={t("set.anthropicKey")}
          configured={status.has_anthropic_key}
          onSave={async (k) => {
            const err = await setApiKey("anthropic", k);
            setTimeout(refresh, 400);
            return err;
          }}
        />
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle>{t("set.autoTitle")}</SectionTitle>
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          {LEVELS().map((l) => {
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

      <ModelsCard settings={settings} />

      <Card style={{ marginBottom: 16 }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
          <div>
            <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>{t("set.typingTitle")}</div>
            <div style={{ fontSize: 12.5, color: theme.textMuted, marginTop: 2 }}>
              {t("set.typingSub")}
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
            {t("set.soundTitle")}
          </div>
          <Segmented
            options={[
              { value: "on", label: t("set.on") },
              { value: "off", label: t("set.off") },
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
              {t("set.ctxTitle")}
            </div>
            <div style={{ fontSize: 12.5, color: theme.textMuted, marginTop: 3 }}>
              {t("set.ctxSub")}
            </div>
          </div>
          <Segmented
            options={[
              { value: "on", label: t("set.on") },
              { value: "off", label: t("set.off") },
            ]}
            value={settings.context_awareness ? "on" : "off"}
            onChange={(v) => onChange({ ...settings, context_awareness: v === "on" })}
          />
        </div>
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
          <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>
            {t("set.muteTitle")}
          </div>
          <Segmented
            options={[
              { value: "on", label: t("set.on") },
              { value: "off", label: t("set.off") },
            ]}
            value={settings.mute_music_while_dictating ? "on" : "off"}
            onChange={(v) => onChange({ ...settings, mute_music_while_dictating: v === "on" })}
          />
        </div>
      </Card>

      <Card>
        <SectionTitle sub={t("set.permsSub")}>
          {t("set.permsTitle")}
        </SectionTitle>
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          <PermRow
            ok={status.accessibility}
            label={t("set.permAcc")}
            detail={t("set.permAccDetail")}
            onClick={() => {
              requestAccessibility();
              setTimeout(refresh, 800);
            }}
          />
          <PermRow
            ok={status.microphone}
            label={t("set.permMic")}
            detail={t("set.permMicDetail")}
            onClick={() => {
              requestMicrophone();
              setTimeout(refresh, 1000);
            }}
          />
          <PermRow
            ok={status.input_monitoring}
            label={t("set.permInput")}
            detail={t("set.permInputDetail")}
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
          <Dot ok /> {t("set.installed")}
        </div>
      ) : (
        <ModelDownload kind={kind} sizeLabel={sizeLabel} onDone={onDone} />
      )}
    </div>
  );
}

function ModelsCard({ settings }: { settings: Settings }) {
  const { status, refresh } = useModelStatus();
  if (status === null) return null;
  return (
    <Card style={{ marginBottom: 16 }}>
      <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong, marginBottom: 14 }}>{t("set.modelsTitle")}</div>
      {((settings.asr_mode === "local" && !status.asr) ||
        (settings.cleanup_mode === "local" && !status.llm)) && (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            background: "rgba(192,57,43,0.08)",
            border: "1px solid rgba(192,57,43,0.28)",
            borderRadius: 10,
            padding: "10px 12px",
            marginBottom: 14,
            fontSize: 12.5,
            color: "#C0392B",
          }}
        >
          <Icon name="close" size={13} />
          {t("set.modelMissing")}
        </div>
      )}
      <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
        <ModelRow
          title={t("set.asrTitle")}
          detail={t("set.asrDetail")}
          installed={status.asr}
          kind="asr"
          sizeLabel="547 MB"
          onDone={refresh}
        />
        <div style={{ borderTop: `1px solid ${theme.border}` }} />
        <ModelRow
          title={t("set.llmTitle")}
          detail={t("set.llmDetail")}
          installed={status.llm}
          kind="llm"
          sizeLabel="2.4 GB"
          onDone={refresh}
        />
      </div>
    </Card>
  );
}

// Turn a KeyboardEvent.code into Tauri accelerator syntax, or null if the key
// isn't something we can register globally.
function codeToAccel(code: string): string | null {
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1];
  const digit = /^Digit(\d)$/.exec(code);
  if (digit) return digit[1];
  if (/^F\d{1,2}$/.test(code)) return code;
  const map: Record<string, string> = {
    Space: "Space",
    Comma: "Comma",
    Period: "Period",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
  };
  return map[code] ?? null;
}

// Fallback when e.code is empty (synthetic events): derive from e.key. Note
// that on macOS Option+letter changes e.key ("∂"), so e.code stays primary.
function keyToAccel(k: string): string | null {
  if (/^[a-zA-Z]$/.test(k)) return k.toUpperCase();
  if (/^[0-9]$/.test(k)) return k;
  if (/^F\d{1,2}$/.test(k)) return k;
  const map: Record<string, string> = {
    " ": "Space",
    ",": "Comma",
    ".": "Period",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
  };
  return map[k] ?? null;
}

// "Ctrl+Alt+D" -> "⌃ ⌥ D" for display.
function prettyAccel(v: string): string {
  return v
    .replace(/CommandOrControl\+|CmdOrCtrl\+/gi, "⌘ ")
    .replace(/Cmd\+|Command\+/gi, "⌘ ")
    .replace(/Ctrl\+|Control\+/gi, "⌃ ")
    .replace(/Alt\+|Option\+/gi, "⌥ ")
    .replace(/Shift\+/gi, "⇧ ");
}

/// Click-to-capture hotkey picker: a field that opens a modal which records
/// the next real key press and stores it as an accelerator.
function HotkeyField({ value, onPick }: { value: string; onPick: (v: string) => void }) {
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const close = () => {
    setOpen(false);
    setError(null);
  };

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        close();
        return;
      }
      // Bare modifiers: keep waiting for the real key.
      if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) return;
      const key = codeToAccel(e.code) ?? keyToAccel(e.key);
      if (!key) {
        setError(t("set.hkUnsupported"));
        return;
      }
      const mods = [
        e.ctrlKey && "Ctrl",
        e.altKey && "Alt",
        e.shiftKey && "Shift",
        e.metaKey && "Cmd",
      ].filter(Boolean) as string[];
      // A bare letter/number would swallow normal typing system-wide; only
      // F-keys may stand alone.
      if (mods.length === 0 && !/^F\d{1,2}$/.test(key)) {
        setError(t("set.hkNeedsMod"));
        return;
      }
      onPick([...mods, key].join("+"));
      close();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [open, onPick]);

  return (
    <>
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <button
          onClick={() => setOpen(true)}
          className="wf-press"
          style={{
            minWidth: 220,
            textAlign: "left",
            background: theme.cardBgSubtle,
            border: `1px solid ${theme.border}`,
            borderRadius: 10,
            padding: "9px 12px",
            color: value ? theme.textStrong : theme.textFaint,
            fontFamily: font.ui,
            fontSize: 13.5,
            fontWeight: value ? 650 : 400,
            cursor: "pointer",
          }}
        >
          {value ? prettyAccel(value) : t("set.hkClick")}
        </button>
        {value && (
          <Button variant="ghost" size="sm" onClick={() => onPick("")}>
            {t("set.hkRemove")}
          </Button>
        )}
      </div>

      {open && createPortal(
        <div
          onClick={close}
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(26,26,26,0.35)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 1000,
          }}
        >
          <div
            className="wf-pop"
            onClick={(e) => e.stopPropagation()}
            style={{
              background: theme.cardBg,
              borderRadius: 18,
              padding: "30px 34px",
              width: 420,
              maxWidth: "90%",
              textAlign: "center",
              boxShadow: "0 8px 40px rgba(26,26,26,0.25)",
            }}
          >
            <div style={{ fontFamily: font.serif, fontSize: 22, fontWeight: 600, color: theme.textStrong }}>
              {t("set.hkModalTitle")}
            </div>
            <div
              style={{
                margin: "18px auto 14px",
                width: 92,
                height: 56,
                borderRadius: 12,
                border: `1.5px dashed ${theme.borderStrong}`,
                background: theme.cardBgSubtle,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                fontFamily: font.serif,
                fontSize: 24,
                color: theme.textFaint,
              }}
            >
              ?
            </div>
            <div style={{ fontSize: 12.5, color: theme.textMuted, lineHeight: 1.55 }}>
              {t("set.hkModalSub")}
            </div>
            {error && (
              <div style={{ fontSize: 12.5, color: "#C0392B", marginTop: 10 }}>{error}</div>
            )}
            <div style={{ display: "flex", gap: 8, justifyContent: "center", marginTop: 18 }}>
              <Button variant="ghost" size="sm" onClick={close}>
                {t("set.hkCancelBtn")}
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  onPick("");
                  close();
                }}
              >
                {t("set.hkRemove")}
              </Button>
            </div>
          </div>
        </div>,
        document.body
      )}
    </>
  );
}


/// Mini screen with clickable magnet dots. The bottom-most pair sits on the
/// screen's true bottom edge (beside the Dock); the row above it is the
/// regular above-the-Dock line.
function PillPositionCard() {
  const [active, setActive] = useState<string>("");
  useEffect(() => {
    void getPillPosition().then(setActive);
  }, []);

  const pick = (pos: PillPosition) => {
    setActive(pos);
    void setPillPosition(pos);
  };

  const DOTS: { pos: PillPosition; label: string; x: string; y: string; vertical?: boolean }[] = [
    { pos: "top-left", label: t("set.posTopLeft"), x: "8%", y: "12%" },
    { pos: "top-center", label: t("set.posTopCenter"), x: "50%", y: "12%" },
    { pos: "top-right", label: t("set.posTopRight"), x: "92%", y: "12%" },
    { pos: "left", label: t("set.posLeft"), x: "8%", y: "46%", vertical: true },
    { pos: "right", label: t("set.posRight"), x: "92%", y: "46%", vertical: true },
    { pos: "bottom-left", label: t("set.posBottomLeft"), x: "8%", y: "66%" },
    { pos: "bottom-center", label: t("set.posBottomCenter"), x: "50%", y: "66%" },
    { pos: "bottom-right", label: t("set.posBottomRight"), x: "92%", y: "66%" },
    { pos: "screen-bottom-left", label: t("set.posScreenBottomLeft"), x: "8%", y: "88%" },
    { pos: "screen-bottom-right", label: t("set.posScreenBottomRight"), x: "92%", y: "88%" },
  ];

  return (
    <Card style={{ marginBottom: 16 }}>
      <SectionTitle sub={t("set.pillSub")}>{t("set.pillTitle")}</SectionTitle>
      <div
        style={{
          position: "relative",
          width: 300,
          maxWidth: "100%",
          height: 170,
          borderRadius: 14,
          border: `1.5px solid ${theme.borderStrong}`,
          background: theme.cardBgSubtle,
          overflow: "hidden",
        }}
      >
        {/* hinted Dock */}
        <div
          style={{
            position: "absolute",
            left: "50%",
            bottom: 6,
            transform: "translateX(-50%)",
            width: 110,
            height: 12,
            borderRadius: 999,
            background: theme.track,
          }}
        />
        {DOTS.map((d) => {
          const selected = active === d.pos;
          return (
            <button
              key={d.pos}
              title={d.label}
              onClick={() => pick(d.pos)}
              className="wf-press"
              style={{
                position: "absolute",
                left: d.x,
                top: d.y,
                transform: "translate(-50%, -50%)",
                width: d.vertical ? 10 : 22,
                height: d.vertical ? 22 : 10,
                borderRadius: 999,
                border: "none",
                cursor: "pointer",
                background: selected ? theme.accent : theme.borderStrong,
                boxShadow: selected ? `0 0 0 3px ${theme.accentSoft}` : "none",
              }}
            />
          );
        })}
      </div>
    </Card>
  );
}
