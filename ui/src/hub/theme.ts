// Light "Hub" theme, layered on top of the shared design tokens. This governs
// only the desktop Hub window — the floating overlay pill stays on its own dark
// palette. Matched to the real Wispr Flow Hub: near-white warm gray page, white
// cards with hairline borders, black primary buttons, teal data accent, lilac
// badge surfaces, dark plum banner.

import { palette } from "../tokens/values";

export const theme = {
  // Surfaces
  pageBg: "#F7F6F2",
  sidebarBg: "#F7F6F2",
  cardBg: "#FFFFFF",
  cardBgSubtle: "#FAF9F7",
  track: "#EFEDE9", // segmented-control / gauge-track neutral
  hover: "#ECEAE5", // sidebar active pill + row hover

  // Borders
  border: "#E9E6E0",
  borderStrong: "#DAD6CE",

  // Text
  textStrong: "#1A1A1A",
  textBody: "#2B2830",
  textMuted: "#7A7580",
  textFaint: "#9B96A0",

  // Accent (deep teal — data/graphs/links)
  accent: palette.accent500,
  accentDeep: palette.accent600,
  accentBright: palette.accent400,
  accentSoft: "rgba(42,99,88,0.10)",
  accentSoftHover: "rgba(42,99,88,0.16)",
  accentSoftBorder: "rgba(42,99,88,0.28)",

  // Primary button (black, like Wispr's "Add new" / "Upgrade")
  btnBg: "#1A1A1A",
  btnText: "#FFFFFF",

  // Lilac badge surfaces
  lilacBg: palette.lilacBg,
  lilacText: palette.lilacText,
  lilacBorder: palette.lilacBorder,

  // Elevation
  shadow: "0 1px 2px rgba(26,26,26,0.03), 0 4px 16px rgba(26,26,26,0.04)",
  shadowSoft: "0 1px 2px rgba(26,26,26,0.04)",

  // Dark banner gradient (plum, like Wispr's photo banners)
  bannerFrom: palette.slate900,
  bannerVia: palette.slate850,
  bannerTo: palette.slate800,
} as const;
