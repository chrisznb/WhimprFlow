// WhimprFlow design tokens — matched to the real Wispr Flow look (personal build).
// Extracted from the app's own bundles/screenshots: near-white warm surfaces,
// dark plum pill (#312D37 — Wispr's own overlay color), deep teal data accent,
// lilac badges, black primary buttons, EB Garamond display + Figtree UI type.

export const palette = {
  // Warm plum-charcoal scale (pill + dark banner hue).
  slate950: "#141216",
  slate900: "#312D37", // pill base (Wispr overlay color)
  slate850: "#3A3540",
  slate800: "#49454D", // Wispr's muted dark
  slate700: "#565159",
  slate600: "#6E6870",
  slate500: "#8A858D",
  slate400: "#A7A2AA",
  slate300: "#C9C5CC",
  slate200: "#E2DFE4",
  slate100: "#EFEDF0",
  slate050: "#F9F8F6",

  // Deep teal — Wispr's data/graph accent (gauges, bars, heatmap).
  accent400: "#3E8578",
  accent500: "#2A6358",
  accent600: "#1F4F46",
  accentGlow: "rgba(62,133,120,0.35)",

  // Lilac — badges ("Pro Trial"), selected style cards, trial surfaces.
  lilacBg: "#F3EAFD",
  lilacText: "#7C4DCB",
  lilacBorder: "#C9A8F0",

  // Orange — the focus ring Wispr uses on active settings nav items.
  focusRing: "#E8A33D",

  // Pill text + waveform bars.
  pillText: "#F5F3F0",
  pillTextMuted: "#A7A2AA",
  waveBar: "#F5F3F0",

  // Semantic.
  error: "#E5484D",
  warn: "#E8A33D",
  info: "#5AA9FF",
  success: "#2A6358",
} as const;

// Status ring (teal ramp).
export const ringStops = ["#2A6358", "#3E8578", "#7FB8AC", "#C9E2DC", "#2A6358"] as const;

export const pillFill = {
  base: palette.slate900,
  raised: palette.slate850,
  border: "rgba(255,255,255,0.07)",
  shadow: "none",
} as const;

// Geometry (logical px). Pill sizes match the real Flow Bar: a slim idle nub
// that morphs into a compact waveform bar while recording.
export const geometry = {
  morphMs: 380,
  idleBar: { w: 45, h: 7 },
  restNub: { w: 30, h: 6, r: 6 },
  miniPill: { w: 330, h: 32, r: 22.5 },
  card: { w: 380, h: 130, r: 24 },
  wave: { minBars: 5, maxBars: 7, minH: 8, maxH: 24 },
  border: 2,
  overlayWindow: { w: 440, h: 320 },
} as const;

export const motion = {
  // Slight overshoot for the pill morph — feels springy, not linear.
  ease: "cubic-bezier(0.34, 1.3, 0.5, 1)",
  springDurationS: 0.2,
} as const;

export const font = {
  ui: '"Figtree", "Inter", system-ui, sans-serif',
  serif: '"EB Garamond", "Newsreader", Georgia, serif',
  mono: '"JetBrains Mono", ui-monospace, monospace',
} as const;
