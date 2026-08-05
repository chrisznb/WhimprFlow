import { useEffect, useState } from "react";
import type { CSSProperties, ReactNode } from "react";
import { font, palette } from "../tokens/values";
import { theme } from "./theme";
import { getStats, type StatsSummary, EMPTY_STATS } from "./api";

// ── Card ─────────────────────────────────────────────────────────────────────
export function Card({
  children,
  style,
  pad = 22,
}: {
  children: ReactNode;
  style?: CSSProperties;
  pad?: number;
}) {
  return (
    <div
      className="wf-card"
      style={{
        background: theme.cardBg,
        border: `1px solid ${theme.border}`,
        borderRadius: 14,
        padding: pad,
        boxShadow: theme.shadowSoft,
        ...style,
      }}
    >
      {children}
    </div>
  );
}

// ── Status dot ───────────────────────────────────────────────────────────────
export function Dot({ ok, size = 9 }: { ok: boolean; size?: number }) {
  return (
    <span
      style={{
        display: "inline-block",
        width: size,
        height: size,
        borderRadius: 9999,
        background: ok ? palette.success : palette.error,
        boxShadow: ok ? `0 0 0 3px ${theme.accentSoft}` : "none",
        marginRight: 8,
        flex: "0 0 auto",
      }}
    />
  );
}

// ── Button ───────────────────────────────────────────────────────────────────
export function Button({
  children,
  onClick,
  variant = "dark",
  size = "md",
  disabled = false,
  type = "button",
}: {
  children: ReactNode;
  onClick?: () => void;
  variant?: "dark" | "accent" | "ghost";
  size?: "sm" | "md";
  disabled?: boolean;
  type?: "button" | "submit";
}) {
  const pad = size === "sm" ? "6px 12px" : "9px 16px";
  const fontSize = size === "sm" ? 12.5 : 13.5;
  const palettes: Record<string, CSSProperties> = {
    dark: { background: disabled ? theme.textFaint : theme.btnBg, color: theme.btnText, border: "none" },
    accent: { background: disabled ? theme.textFaint : theme.accentDeep, color: "#fff", border: "none" },
    ghost: {
      background: theme.track,
      color: theme.textBody,
      border: "none",
    },
  };
  return (
    <button
      type={type}
      onClick={onClick}
      disabled={disabled}
      className="wf-press"
      style={{
        cursor: disabled ? "default" : "pointer",
        borderRadius: 10,
        padding: pad,
        fontSize,
        fontWeight: 600,
        fontFamily: font.ui,
        display: "inline-flex",
        alignItems: "center",
        gap: 7,
        whiteSpace: "nowrap",
        ...palettes[variant],
      }}
    >
      {children}
    </button>
  );
}

// ── Tabs (Wispr-style: text row with an animated underline) ─────────────────
export function Tabs<T extends string>({
  options,
  value,
  onChange,
}: {
  options: { value: T; label: string }[];
  value: T;
  onChange: (v: T) => void;
}) {
  return (
    <div style={{ display: "flex", gap: 22, borderBottom: `1px solid ${theme.border}` }}>
      {options.map((o) => {
        const active = value === o.value;
        return (
          <button
            key={o.value}
            onClick={() => onChange(o.value)}
            style={{
              border: "none",
              background: "transparent",
              cursor: "pointer",
              padding: "8px 2px 10px",
              fontSize: 14,
              fontFamily: font.ui,
              fontWeight: active ? 650 : 500,
              color: active ? theme.textStrong : theme.textMuted,
              borderBottom: active ? `2px solid ${theme.textStrong}` : "2px solid transparent",
              marginBottom: -1,
              transition: "color 140ms ease, border-color 140ms ease",
            }}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

// ── Segmented control ────────────────────────────────────────────────────────
export function Segmented<T extends string>({
  options,
  value,
  onChange,
  full = false,
}: {
  options: { value: T; label: string }[];
  value: T;
  onChange: (v: T) => void;
  full?: boolean;
}) {
  return (
    <div
      style={{
        display: full ? "grid" : "inline-flex",
        gridTemplateColumns: full ? `repeat(${options.length}, 1fr)` : undefined,
        background: theme.track,
        borderRadius: 11,
        padding: 3,
        gap: 3,
      }}
    >
      {options.map((o) => {
        const active = value === o.value;
        return (
          <button
            key={o.value}
            onClick={() => onChange(o.value)}
            style={{
              border: "none",
              cursor: "pointer",
              borderRadius: 8,
              padding: "7px 14px",
              fontSize: 13,
              fontFamily: font.ui,
              textAlign: "center",
              color: active ? theme.accentDeep : theme.textMuted,
              background: active ? "#fff" : "transparent",
              fontWeight: active ? 600 : 500,
              boxShadow: active ? "0 1px 2px rgba(17,20,25,0.12)" : "none",
              transition: "color 120ms ease",
            }}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

// ── Page heading ─────────────────────────────────────────────────────────────
export function PageTitle({
  children,
  sub,
  serif = false,
}: {
  children: ReactNode;
  sub?: ReactNode;
  serif?: boolean;
}) {
  return (
    <div style={{ marginBottom: 22 }}>
      <h1
        style={{
          fontFamily: serif ? font.serif : font.ui,
          fontSize: serif ? 30 : 26,
          fontWeight: serif ? 600 : 700,
          letterSpacing: serif ? -0.4 : -0.5,
          margin: 0,
          color: theme.textStrong,
        }}
      >
        {children}
      </h1>
      {sub && (
        <p style={{ color: theme.textMuted, fontSize: 14, lineHeight: 1.5, margin: "8px 0 0" }}>{sub}</p>
      )}
    </div>
  );
}

// ── Live stats hook (polls every ~4s so numbers climb while dictating) ───────
export function useStats(): StatsSummary {
  const [stats, setStats] = useState<StatsSummary>(EMPTY_STATS);
  useEffect(() => {
    let alive = true;
    const load = () => getStats().then((s) => alive && setStats(s));
    load();
    const id = setInterval(load, 4000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);
  return stats;
}
