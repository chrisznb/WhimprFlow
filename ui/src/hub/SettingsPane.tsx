import { t } from "../i18n";
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
          <Segmented
            options={[
              { value: "system", label: t("set.langSystem") },
              { value: "de", label: "Deutsch" },
              { value: "en", label: "English" },
            ]}
            value={settings.language || "system"}
            onChange={(v) => onChange({ ...settings, language: v })}
          />
        </div>
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle sub={t("set.hotkeySub")}>
          {t("set.hotkeyTitle")}
        </SectionTitle>
        <input
          type="text"
          value={settings.dictation_hotkey}
          placeholder={t("set.hotkeyPh")}
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

      <ModelsCard />

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

function ModelsCard() {
  const { status, refresh } = useModelStatus();
  if (status === null) return null;
  return (
    <Card style={{ marginBottom: 16 }}>
      <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong, marginBottom: 14 }}>{t("set.modelsTitle")}</div>
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
