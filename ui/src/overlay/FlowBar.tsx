import { useEffect, useRef, useState } from "react";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { faMicrophone, faStop, faXmark } from "@fortawesome/free-solid-svg-icons";
import { palette, pillFill, geometry, font, motion } from "../tokens/values";

// Visual states, mirroring the Rust `BarState`.
export type BarState =
  | "idle"
  | "recording"
  | "locked"
  | "transcribing"
  | "done"
  | "clipboard"
  | "cancelled"
  | "error";

type StateEvent = { state: BarState; vertical?: boolean };
type WaveformEvent = { bars: number[] };

async function tauriListen<T>(event: string, cb: (payload: T) => void): Promise<() => void> {
  try {
    const { listen } = await import("@tauri-apps/api/event");
    return await listen<T>(event, (e) => cb(e.payload as T));
  } catch {
    return () => {};
  }
}

async function tauriInvoke(cmd: string): Promise<void> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke(cmd);
  } catch {
    // plain-browser dev: no-op
  }
}

// A row of dot-like rounded bars driven by mic RMS — small dots when quiet,
// rising into a waveform when speaking.
function DottedWaveform({ bars, vertical = false }: { bars: number[]; vertical?: boolean }) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const barsRef = useRef<number[]>(bars);
  barsRef.current = bars;

  const vertRef = useRef(vertical);
  vertRef.current = vertical;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    let raf = 0;
    const N = 12;
    // Smoothed per-dot amplitudes so bars ease up/down instead of jumping.
    const smooth = new Array(N).fill(0);
    const draw = () => {
      const dpr = window.devicePixelRatio || 1;
      const w = canvas.clientWidth;
      const h = canvas.clientHeight;
      if (canvas.width !== w * dpr || canvas.height !== h * dpr) {
        canvas.width = w * dpr;
        canvas.height = h * dpr;
      }
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);
      const vert = vertRef.current;
      const span = vert ? h : w;
      const dotW = 2;
      const gap = (span - N * dotW) / (N - 1);
      const t = performance.now();
      ctx.fillStyle = palette.waveBar;
      for (let i = 0; i < N; i++) {
        const real = barsRef.current[barsRef.current.length - 1 - (i % barsRef.current.length)];
        // Idle shimmer so the dotted line reads as "listening" even in near-silence.
        const shimmer = 0.1 + 0.05 * Math.abs(Math.sin(t / 260 + i * 0.7));
        // Boost + soften (sqrt) so quiet speech already moves the bars visibly.
        const boosted = Math.min(1, Math.sqrt(Math.min(1, (real ?? 0) * 3)));
        const target = Math.max(shimmer, boosted);
        smooth[i] += (target - smooth[i]) * (target > smooth[i] ? 0.55 : 0.3);
        const bh = 2.5 + smooth[i] * 15; // 2.5px dot → up to ~17.5px bar
        if (vert) {
          const y = i * (dotW + gap);
          const x = (w - bh) / 2;
          ctx.beginPath();
          ctx.roundRect(x, y, bh, dotW, dotW / 2);
          ctx.fill();
        } else {
          const x = i * (dotW + gap);
          const y = (h - bh) / 2;
          ctx.beginPath();
          ctx.roundRect(x, y, dotW, bh, dotW / 2);
          ctx.fill();
        }
      }
      raf = requestAnimationFrame(draw);
    };
    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, []);

  return (
    <canvas
      ref={canvasRef}
      style={
        vertical
          ? { width: 22, height: "100%", display: "block" }
          : { width: "100%", height: 22, display: "block" }
      }
    />
  );
}

function RoundButton({
  title,
  bg,
  onClick,
  children,
}: {
  title: string;
  bg: string;
  onClick?: () => void;
  children: React.ReactNode;
}) {
  return (
    <div
      title={title}
      className="wf-btn-pop wf-round"
      onClick={onClick}
      style={{
        flex: "0 0 auto",
        width: 18,
        height: 18,
        borderRadius: 9999,
        background: bg,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        cursor: "pointer",
        lineHeight: 0,
      }}
    >
      {children}
    </div>
  );
}

function MicIcon() {
  return (
    <FontAwesomeIcon
      icon={faMicrophone}
      className="wf-in"
      style={{ fontSize: 15, color: palette.pillText, display: "block" }}
    />
  );
}

function Check() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" className="wf-check">
      <path
        d="M4.5 12.5l5 5 10-11"
        stroke={palette.waveBar}
        strokeWidth="3"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function PulseDots() {
  return (
    <span style={{ display: "inline-flex", gap: 4, alignItems: "center" }}>
      {[0, 1, 2].map((i) => (
        <span
          key={i}
          className="wf-dot"
          style={{
            width: 4,
            height: 4,
            borderRadius: 9999,
            background: palette.pillTextMuted,
            animationDelay: `${i * 160}ms`,
          }}
        />
      ))}
    </span>
  );
}

export function FlowBar() {
  const [state, setState] = useState<BarState>("idle");
  const [bars, setBars] = useState<number[]>([]);
  const [hover, setHover] = useState(false);
  const [vertical, setVertical] = useState(false);

  useEffect(() => {
    let un1: (() => void) | undefined;
    let un2: (() => void) | undefined;
    let un3: (() => void) | undefined;
    tauriListen<StateEvent>("whimpr://flowbar/state", (p) => {
      setState(p.state);
      if (typeof p.vertical === "boolean") setVertical(p.vertical);
    }).then((u) => (un1 = u));
    void tauriListen<boolean>("whimpr://flowbar/orient", (v) => setVertical(v));
    // Initial orientation (events only arrive on changes).
    void (async () => {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        setVertical(await invoke<boolean>("get_orientation"));
      } catch {
        /* browser dev */
      }
    })();
    tauriListen<WaveformEvent>("whimpr://audio/waveform", (p) => setBars(p.bars)).then((u) => (un2 = u));
    // Native hover from the Rust mouse watcher — WKWebView hover events don't
    // fire reliably in this borderless panel, so this is the real signal; the
    // DOM handlers below are only a fallback for browser dev.
    tauriListen<boolean>("whimpr://flowbar/hover", (h) => setHover(h)).then((u) => (un3 = u));
    return () => {
      un1?.();
      un2?.();
      un3?.();
    };
  }, []);

  const recording = state === "recording" || state === "locked";
  const isIdle = state === "idle";
  const processing = state === "transcribing";
  const done = state === "done";
  const clipboard = state === "clipboard";

  // Idle + hover: the nub morphs into a round mic button that starts a
  // hands-free dictation on click (same as tapping Fn).
  const micMode = isIdle && hover;

  // Pill dimensions per state — matched to the real Flow Bar: slim nub at rest,
  // compact waveform bar while recording.
  const base = isIdle
    ? micMode
      ? { w: 34, h: 34 }
      : { w: geometry.idleBar.w, h: geometry.idleBar.h }
    : recording
      ? { w: 175, h: 30 }
      : processing
        ? { w: vertical ? 26 : 140, h: 26 }
        : clipboard
          ? { w: vertical ? 26 : 210, h: 26 }
          : { w: vertical ? 26 : 108, h: 24 };
  // Vertical pill (snapped to a side edge): swap the axes.
  const dims = vertical && !micMode ? { w: base.h, h: base.w } : base;

  return (
    <div
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        position: "fixed",
        inset: 0,
        display: "flex",
        alignItems: "flex-end",
        justifyContent: "center",
        paddingBottom: 10,
        fontFamily: font.ui,
        userSelect: "none",
      }}
    >
      <div
        aria-label={`WhimprFlow ${state}`}
        onClick={micMode ? () => void tauriInvoke("start_dictation") : undefined}
        style={{
          cursor: micMode ? "pointer" : "default",
          display: "flex",
          flexDirection: vertical ? "column" : "row",
          alignItems: "center",
          justifyContent: recording ? "space-between" : "center",
          gap: 8,
          height: dims.h,
          width: dims.w,
          padding: recording ? (vertical ? "7px 0" : "0 7px") : 0,
          background: pillFill.base,
          border: `1px solid ${pillFill.border}`,
          borderRadius: 9999,
          boxShadow: pillFill.shadow,
          color: palette.pillText,
          opacity: isIdle ? 0.92 : 1,
          transition:
            `width ${geometry.morphMs}ms ${motion.ease}, ` +
            `height ${geometry.morphMs}ms ${motion.ease}, ` +
            `opacity 240ms ease`,
          overflow: "hidden",
          fontSize: 12,
        }}
      >
        {isIdle ? (
          micMode ? <MicIcon /> : null
        ) : recording ? (
          <>
            <RoundButton
              title="Cancel (Esc)"
              bg="rgba(255,255,255,0.14)"
              onClick={() => void tauriInvoke("cancel_dictation")}
            >
              <FontAwesomeIcon
                icon={faXmark}
                style={{ fontSize: 9, color: "#fff", display: "block" }}
              />
            </RoundButton>
            <div
              style={{
                flex: 1,
                minWidth: 0,
                minHeight: 0,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                alignSelf: "stretch",
              }}
              className="wf-in"
            >
              <DottedWaveform bars={bars} vertical={vertical} />
            </div>
            <RoundButton
              title="Stop"
              bg={palette.error}
              onClick={() => void tauriInvoke("stop_dictation")}
            >
              <FontAwesomeIcon
                icon={faStop}
                style={{ fontSize: 7, color: "#fff", display: "block" }}
              />
            </RoundButton>
          </>
        ) : processing ? (
          <span className="wf-in" style={{ display: "inline-flex", alignItems: "center", gap: 7, color: palette.pillTextMuted }}>
            <PulseDots />
            {!vertical && "Cleaning up"}
          </span>
        ) : clipboard ? (
          <span className="wf-in" style={{ display: "inline-flex", alignItems: "center", gap: 6, color: palette.pillText }}>
            <FontAwesomeIcon icon={faStop} style={{ display: "none" }} />
            {vertical ? "!" : "Not pasted. Copied to clipboard"}
          </span>
        ) : done ? (
          <span className="wf-in" style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
            <Check />
            {!vertical && <span style={{ color: palette.pillTextMuted }}>Done</span>}
          </span>
        ) : (
          <span className="wf-in" style={{ color: palette.pillTextMuted, fontSize: vertical ? 10 : 12 }}>
            {vertical ? "!" : state === "error" ? "Something's off" : "Discarded"}
          </span>
        )}
      </div>
    </div>
  );
}
