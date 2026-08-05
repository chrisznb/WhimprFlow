// Small, framework-free formatting helpers used across the Hub.

// 12345 -> "12.3K", 1_200_000 -> "1.2M", 940 -> "940".
export function fmtCompact(n: number): string {
  if (!isFinite(n)) return "0";
  const abs = Math.abs(n);
  if (abs < 1000) return String(Math.round(n));
  if (abs < 1_000_000) return strip(n / 1000) + "K";
  return strip(n / 1_000_000) + "M";
}

function strip(v: number): string {
  const r = Math.round(v * 10) / 10;
  return Number.isInteger(r) ? r.toFixed(0) : r.toFixed(1);
}

// Bundle id -> friendly app name for the history list.
const APP_NAMES: Record<string, string> = {
  "com.anthropic.claudefordesktop": "Claude",
  "com.apple.TextEdit": "TextEdit",
  "com.apple.Safari": "Safari",
  "com.apple.mail": "Mail",
  "com.apple.Notes": "Notes",
  "com.apple.MobileSMS": "Messages",
  "com.apple.finder": "Finder",
  "com.google.Chrome": "Chrome",
  "org.mozilla.firefox": "Firefox",
  "com.tinyspeck.slackmacgap": "Slack",
  "net.whatsapp.WhatsApp": "WhatsApp",
  "com.google.Chrome.app.hnpfjngllnobngcgfapefoaidbinmjnm": "WhatsApp Business",
  "com.microsoft.VSCode": "VS Code",
  "com.microsoft.Outlook": "Outlook",
  "com.spotify.client": "Spotify",
  "notion.id": "Notion",
  "com.figma.Desktop": "Figma",
};

export function prettyApp(bundleId: string): string {
  const known = APP_NAMES[bundleId];
  if (known) return known;
  // Unknown Chrome PWA ids are random letters — don't show them raw.
  if (bundleId.startsWith("com.google.Chrome.app.")) return "Chrome-App";
  if (!bundleId.includes(".")) return bundleId;
  // Fallback: last reverse-domain segment, cleaned up and capitalized.
  let seg = bundleId.split(".").pop() ?? bundleId;
  seg = seg.replace(/fordesktop$|desktop$|macos$|app$/i, "") || seg;
  return seg.charAt(0).toUpperCase() + seg.slice(1);
}

export function fmtNum(n: number): string {
  return Math.round(n).toLocaleString();
}

export function fmtDuration(secs: number): string {
  const m = Math.round(secs / 60);
  if (m < 1) return "under a minute";
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  const rem = m % 60;
  return rem ? `${h}h ${rem}m` : `${h}h`;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

// "9:41 am"
export function fmtTimeOfDay(d: Date): string {
  let h = d.getHours();
  const m = d.getMinutes();
  const ap = h >= 12 ? "pm" : "am";
  h = h % 12;
  if (h === 0) h = 12;
  return `${h}:${m.toString().padStart(2, "0")} ${ap}`;
}

function startOfDay(d: Date): number {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

// Section header for a history group: "Today" / "Yesterday" / "Jul 15".
export function dayLabel(d: Date, now: Date = new Date()): string {
  const diff = Math.round((startOfDay(now) - startOfDay(d)) / 86_400_000);
  if (diff <= 0) return "Today";
  if (diff === 1) return "Yesterday";
  return `${MONTHS[d.getMonth()]} ${d.getDate()}`;
}

export function dayKey(d: Date): string {
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

// Friendly human comparison for a running word count (adds a little delight).
export function wordsReference(n: number): string {
  if (n < 60) return "just getting started";
  if (n < 500) return `about ${Math.max(1, Math.round(n / 55))} thank-you notes`;
  if (n < 4000) return `about ${newsArticles(n)} news articles`;
  if (n < 40000) return `about ${Math.max(1, Math.round(n / 8000))} short film scripts`;
  return `about ${Math.max(1, Math.round(n / 80000))} novels`;
}

// ~800 words per news article.
export function newsArticles(n: number): number {
  return Math.max(1, Math.round(n / 800));
}
