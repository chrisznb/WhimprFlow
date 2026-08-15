// Model install state + download UI, shared by the Home setup card and the
// Settings "Models" section. Downloads stream progress via a Tauri event.
import { useCallback, useEffect, useRef, useState } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import {
  downloadModel,
  getModelStatus,
  onModelProgress,
  type ModelProgress,
  type ModelStatus,
} from "./api";

export function useModelStatus() {
  const [status, setStatus] = useState<ModelStatus | null>(null);
  const refresh = useCallback(() => {
    void getModelStatus().then(setStatus);
  }, []);
  useEffect(refresh, [refresh]);
  return { status, refresh };
}

function fmtMb(bytes: number): string {
  return `${Math.round(bytes / 1_000_000)}`;
}

/// Download button that turns into a progress bar while running.
export function ModelDownload({
  kind,
  sizeLabel,
  onDone,
}: {
  kind: "asr" | "llm";
  sizeLabel: string;
  onDone: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<ModelProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const unlistenRef = useRef<(() => void) | null>(null);

  useEffect(() => () => unlistenRef.current?.(), []);

  const start = async () => {
    setBusy(true);
    setError(null);
    unlistenRef.current = await onModelProgress((p) => {
      if (p.kind === kind) setProgress(p);
    });
    const err = await downloadModel(kind);
    unlistenRef.current?.();
    unlistenRef.current = null;
    setBusy(false);
    setProgress(null);
    if (err) {
      setError(err);
    } else {
      onDone();
    }
  };

  if (busy) {
    const pct =
      progress && progress.total > 0
        ? Math.min(100, Math.round((progress.done / progress.total) * 100))
        : null;
    return (
      <div style={{ minWidth: 200 }}>
        <div
          style={{
            height: 8,
            borderRadius: 999,
            background: theme.track,
            overflow: "hidden",
          }}
        >
          <div
            style={{
              height: "100%",
              width: pct === null ? "30%" : `${pct}%`,
              borderRadius: 999,
              background: theme.accent,
              transition: "width 300ms ease",
            }}
          />
        </div>
        <div style={{ fontSize: 11.5, color: theme.textMuted, marginTop: 6, fontVariantNumeric: "tabular-nums" }}>
          {progress && progress.total > 0
            ? `${fmtMb(progress.done)} / ${fmtMb(progress.total)} MB`
            : "Starting download"}
        </div>
      </div>
    );
  }

  return (
    <div>
      <button
        className="wf-press"
        onClick={() => void start()}
        style={{
          fontFamily: font.ui,
          fontSize: 13,
          fontWeight: 600,
          color: theme.btnText,
          background: theme.btnBg,
          border: "none",
          borderRadius: 9,
          padding: "9px 16px",
          cursor: "pointer",
        }}
      >
        Download ({sizeLabel})
      </button>
      {error && (
        <div style={{ fontSize: 12, color: "#B3261E", marginTop: 6, maxWidth: 260 }}>
          {error}
        </div>
      )}
    </div>
  );
}
