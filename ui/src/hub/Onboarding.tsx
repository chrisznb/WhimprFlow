import { t } from "../i18n";
import { useEffect } from "react";
import { font, palette } from "../tokens/values";
import { theme } from "./theme";
import {
  requestAccessibility,
  requestMicrophone,
  requestInputMonitoring,
  type Status,
} from "./api";

// A blocking permission gate: the app can't be used until Accessibility and
// Microphone are granted. The three permissions are presented in order (each
// unlocks the next), and their state flips live as macOS applies them.

function Step({
  n,
  title,
  detail,
  done,
  active,
  locked,
  required,
  onGrant,
}: {
  n: number;
  title: string;
  detail: string;
  done: boolean;
  active: boolean;
  locked: boolean;
  required: boolean;
  onGrant: () => void;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 16,
        padding: "16px 18px",
        borderRadius: 14,
        marginBottom: 12,
        background: active ? theme.accentSoft : theme.cardBg,
        border: `1px solid ${active ? theme.accentSoftBorder : theme.border}`,
        boxShadow: theme.shadowSoft,
        opacity: locked ? 0.5 : 1,
      }}
    >
      <div
        style={{
          flex: "0 0 auto",
          width: 30,
          height: 30,
          borderRadius: 9999,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          fontWeight: 700,
          fontSize: 14,
          color: done ? "#fff" : theme.textMuted,
          background: done ? theme.accentDeep : theme.track,
        }}
      >
        {done ? "✓" : n}
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 15, fontWeight: 600, color: theme.textStrong }}>
          {title}{" "}
          <span style={{ fontSize: 12, color: theme.textFaint, fontWeight: 400 }}>
            {required ? t("ob.required") : t("ob.optional")}
          </span>
        </div>
        <div style={{ fontSize: 13, color: theme.textMuted, marginTop: 2 }}>{detail}</div>
      </div>
      {done ? (
        <span style={{ color: theme.accentDeep, fontSize: 13, fontWeight: 600 }}>{t("ob.granted")}</span>
      ) : (
        <button
          onClick={onGrant}
          disabled={locked}
          style={{
            cursor: locked ? "default" : "pointer",
            border: "none",
            borderRadius: 10,
            padding: "9px 16px",
            fontSize: 13,
            fontWeight: 600,
            fontFamily: font.ui,
            color: "#fff",
            background: locked ? theme.textFaint : palette.slate900,
            whiteSpace: "nowrap",
          }}
        >
          {t("ob.grant")}
        </button>
      )}
    </div>
  );
}

export function Onboarding({
  status,
  refresh,
  onEnter,
}: {
  status: Status;
  refresh: () => void;
  onEnter: () => void;
}) {
  // Poll live so the state flips the moment macOS applies each grant.
  useEffect(() => {
    const id = setInterval(refresh, 1200);
    return () => clearInterval(id);
  }, [refresh]);

  const acc = status.accessibility;
  const mic = status.microphone;
  const inp = status.input_monitoring;
  const canEnter = acc && mic;

  return (
    <div
      style={{
        height: "100vh",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: theme.pageBg,
        color: theme.textBody,
        fontFamily: font.ui,
        padding: 24,
      }}
    >
      <div style={{ width: 560, maxWidth: "100%" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 6 }}>
          <div style={{ fontFamily: font.serif, fontSize: 30, fontWeight: 600, color: theme.textStrong }}>
            {t("ob.title")}
          </div>
          <span
            style={{
              fontSize: 10,
              fontWeight: 700,
              letterSpacing: 0.4,
              textTransform: "uppercase",
              color: theme.accentDeep,
              background: theme.accentSoft,
              border: `1px solid ${theme.accentSoftBorder}`,
              borderRadius: 999,
              padding: "2px 7px",
            }}
          >
            {t("ob.local")}
          </span>
        </div>
        <p style={{ color: theme.textMuted, lineHeight: 1.5, margin: "0 0 24px" }}>
          {t("ob.intro")}
        </p>

        <Step
          n={1}
          title={t("ob.acc")}
          detail={t("ob.accDetail")}
          done={acc}
          active={!acc}
          locked={false}
          required
          onGrant={() => requestAccessibility()}
        />
        <Step
          n={2}
          title={t("ob.mic")}
          detail={t("ob.micDetail")}
          done={mic}
          active={acc && !mic}
          locked={!acc}
          required
          onGrant={() => requestMicrophone()}
        />
        <Step
          n={3}
          title={t("ob.input")}
          detail={t("ob.inputDetail")}
          done={inp}
          active={acc && mic && !inp}
          locked={!(acc && mic)}
          required={false}
          onGrant={() => requestInputMonitoring()}
        />

        <button
          onClick={onEnter}
          disabled={!canEnter}
          style={{
            marginTop: 12,
            width: "100%",
            cursor: canEnter ? "pointer" : "default",
            border: "none",
            borderRadius: 12,
            padding: "13px",
            fontSize: 15,
            fontWeight: 700,
            fontFamily: font.ui,
            color: "#fff",
            background: canEnter ? theme.accentDeep : theme.textFaint,
          }}
        >
          {canEnter ? t("ob.enter") : t("ob.enterLocked")}
        </button>

        <p style={{ fontSize: 12, color: theme.textFaint, lineHeight: 1.5, marginTop: 16 }}>
          {t("ob.hint")}
        </p>
      </div>
    </div>
  );
}
