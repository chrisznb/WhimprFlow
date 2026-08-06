import { useEffect, useState } from "react";
import { font, palette } from "../tokens/values";
import { theme } from "./theme";
import { Card, SkeletonRows, useStats } from "./ui";
import { Icon } from "./icons";
import { getHistory, type HistoryItem, type StatsSummary } from "./api";
import { dayKey, dayLabel, fmtCompact, fmtDuration, fmtNum, fmtTimeOfDay, prettyApp, wordsReference } from "./format";

const UNLOCK_WORDS = 500;

// ── Banner ───────────────────────────────────────────────────────────────────
function Banner() {
  return (
    <div
      style={{
        position: "relative",
        overflow: "hidden",
        borderRadius: 16,
        padding: "26px 28px",
        background: `linear-gradient(135deg, ${theme.bannerFrom} 0%, ${theme.bannerVia} 52%, ${theme.bannerTo} 100%)`,
        boxShadow: theme.shadow,
      }}
    >
      {/* soft accent glow */}
      <div
        style={{
          position: "absolute",
          right: -60,
          top: -60,
          width: 220,
          height: 220,
          borderRadius: "50%",
          background: `radial-gradient(circle, ${palette.accentGlow} 0%, transparent 68%)`,
          opacity: 0.5,
          pointerEvents: "none",
        }}
      />
      <div style={{ position: "relative", maxWidth: 460 }}>
        <div
          style={{
            fontFamily: font.serif,
            fontSize: 23,
            fontWeight: 600,
            letterSpacing: -0.3,
            color: palette.slate050,
            lineHeight: 1.2,
          }}
        >
          Cleanup works anywhere you write.
        </div>
        <p style={{ color: palette.slate300, fontSize: 14, lineHeight: 1.55, margin: "10px 0 0" }}>
          Tap or hold your key, speak, and WhimprFlow types clean text wherever your cursor is.
        </p>
      </div>
    </div>
  );
}

// ── History ──────────────────────────────────────────────────────────────────
type Group = { key: string; label: string; items: HistoryItem[] };

function groupByDay(items: HistoryItem[]): Group[] {
  const now = new Date();
  const groups: Group[] = [];
  const index = new Map<string, Group>();
  for (const it of items) {
    const d = new Date(it.ts_unix * 1000);
    const k = dayKey(d);
    let g = index.get(k);
    if (!g) {
      g = { key: k, label: dayLabel(d, now), items: [] };
      index.set(k, g);
      groups.push(g);
    }
    g.items.push(it);
  }
  return groups;
}

function HistoryRow({ item }: { item: HistoryItem }) {
  const d = new Date(item.ts_unix * 1000);
  const [copied, setCopied] = useState(false);
  const copy = () => {
    void navigator.clipboard.writeText(item.text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    });
  };
  return (
    <div
      className="wf-row"
      style={{ display: "flex", gap: 14, padding: "11px 4px", borderBottom: `1px solid ${theme.border}` }}
    >
      <div
        style={{
          flex: "0 0 74px",
          fontSize: 12,
          color: theme.textFaint,
          fontVariantNumeric: "tabular-nums",
          paddingTop: 1,
        }}
      >
        {fmtTimeOfDay(d)}
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 13.5, lineHeight: 1.5, color: theme.textBody }}>{item.text}</div>
        {item.app && (
          <div style={{ fontSize: 11, color: theme.textFaint, marginTop: 3 }}>{prettyApp(item.app)}</div>
        )}
      </div>
      <div className="wf-row-actions" style={{ flex: "0 0 auto", display: "flex", alignItems: "flex-start" }}>
        <button
          title="Copy"
          onClick={copy}
          className="wf-press"
          style={{
            border: "none",
            background: "transparent",
            cursor: "pointer",
            color: copied ? theme.accent : theme.textFaint,
            padding: "2px 4px",
          }}
        >
          <Icon name="copy" size={15} />
        </button>
      </div>
    </div>
  );
}

function HistorySection({ history }: { history: HistoryItem[] | null }) {
  const [query, setQuery] = useState("");
  const q = query.trim().toLowerCase();
  const items = history ?? [];
  const filtered = q ? items.filter((h) => h.text.toLowerCase().includes(q)) : items;
  const groups = groupByDay(filtered);

  return (
    <Card pad={0}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 12,
          padding: "16px 18px",
          borderBottom: `1px solid ${theme.border}`,
        }}
      >
        <div
          style={{
            fontSize: 11.5,
            fontWeight: 700,
            letterSpacing: 0.7,
            textTransform: "uppercase",
            color: theme.textFaint,
          }}
        >
          Recent dictations
        </div>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 7,
            background: theme.cardBgSubtle,
            border: `1px solid ${theme.border}`,
            borderRadius: 9,
            padding: "6px 10px",
            minWidth: 180,
          }}
        >
          <Icon name="search" size={15} style={{ color: theme.textFaint }} />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search history"
            style={{
              border: "none",
              outline: "none",
              background: "transparent",
              fontFamily: font.ui,
              fontSize: 13,
              color: theme.textBody,
              width: "100%",
            }}
          />
        </div>
      </div>

      <div style={{ padding: "6px 18px 14px" }}>
        {history === null ? (
          <div style={{ padding: "18px 4px" }}>
            <SkeletonRows rows={4} />
          </div>
        ) : history.length === 0 ? (
          <div style={{ padding: "36px 8px", textAlign: "center", color: theme.textFaint, fontSize: 13.5 }}>
            Your dictations will show up here. Tap your key and start speaking.
          </div>
        ) : filtered.length === 0 ? (
          <div style={{ padding: "36px 8px", textAlign: "center", color: theme.textFaint, fontSize: 13.5 }}>
            No dictations match “{query}”.
          </div>
        ) : (
          groups.map((g) => (
            <div key={g.key} style={{ marginTop: 14 }}>
              <div
                style={{
                  fontSize: 11,
                  fontWeight: 700,
                  letterSpacing: 0.6,
                  textTransform: "uppercase",
                  color: theme.accentDeep,
                  marginBottom: 2,
                }}
              >
                {g.label}
              </div>
              {g.items.map((it, i) => (
                <HistoryRow key={`${it.ts_unix}-${i}`} item={it} />
              ))}
            </div>
          ))
        )}
      </div>
    </Card>
  );
}

// ── Stats card (right column) ────────────────────────────────────────────────
function BigStat({ value, label, accent }: { value: string; label: string; accent?: boolean }) {
  return (
    <div style={{ flex: 1, textAlign: "center" }}>
      <div
        style={{
          fontFamily: font.serif,
          fontSize: 30,
          fontWeight: 600,
          lineHeight: 1.05,
          color: accent ? theme.accentDeep : theme.textStrong,
        }}
      >
        {value}
      </div>
      <div
        style={{
          fontSize: 10.5,
          color: theme.textFaint,
          marginTop: 6,
          textTransform: "uppercase",
          letterSpacing: 0.6,
        }}
      >
        {label}
      </div>
    </div>
  );
}

function StatsCard({ stats }: { stats: StatsSummary }) {
  const unlocked = stats.total_words >= UNLOCK_WORDS;
  return (
    <Card>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 4 }}>
        <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>Your stats</div>
        <div style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 12, color: theme.accentDeep, fontWeight: 600 }}>
          <Icon name="flame" size={13} /> keep it up
        </div>
      </div>

      <div style={{ textAlign: "center", margin: "16px 0 6px" }}>
        <div style={{ fontFamily: font.serif, fontSize: 42, fontWeight: 600, color: theme.textStrong, lineHeight: 1 }}>
          {fmtCompact(stats.total_words)}
        </div>
        <div style={{ fontSize: 11.5, color: theme.textFaint, marginTop: 6, textTransform: "uppercase", letterSpacing: 0.6 }}>
          total words
        </div>
      </div>

      <div style={{ fontSize: 12, color: theme.textMuted, textAlign: "center", marginBottom: 16 }}>
        {wordsReference(stats.total_words)}
      </div>

      <div
        style={{
          display: "flex",
          gap: 8,
          padding: "16px 0 0",
          borderTop: `1px solid ${theme.border}`,
        }}
      >
        <BigStat value={fmtNum(stats.avg_wpm)} label="avg WPM" accent />
        <BigStat value={`${stats.day_streak}`} label="day streak" />
      </div>

      {unlocked ? (
        <div style={{ fontSize: 12, color: theme.textFaint, textAlign: "center", marginTop: 14 }}>
          {fmtNum(stats.best_wpm)} WPM best · saved you {fmtDuration(stats.time_saved_secs)} vs typing
        </div>
      ) : (
        <div style={{ fontSize: 12, color: theme.textFaint, textAlign: "center", marginTop: 14, lineHeight: 1.5 }}>
          Keep dictating to unlock richer stats. {fmtNum(Math.max(0, UNLOCK_WORDS - stats.total_words))} words to go.
        </div>
      )}
    </Card>
  );
}

// ── Page ─────────────────────────────────────────────────────────────────────
export function Home() {
  const stats = useStats();
  const [history, setHistory] = useState<HistoryItem[] | null>(null);

  useEffect(() => {
    let alive = true;
    const load = () => getHistory().then((h) => alive && setHistory(h));
    load();
    const id = setInterval(load, 8000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);

  const today = stats.words_today;

  return (
    <div style={{ maxWidth: 1000 }}>
      <div style={{ marginBottom: 22 }}>
        <h1
          style={{
            fontFamily: font.ui,
            fontSize: 27,
            fontWeight: 700,
            letterSpacing: -0.5,
            margin: 0,
            color: theme.textStrong,
          }}
        >
          Welcome back
        </h1>
        <p style={{ color: theme.textMuted, fontSize: 14, margin: "8px 0 0" }}>
          {today > 0 ? `${fmtNum(today)} words dictated today.` : "Ready when you are. Tap or hold your key and speak."}
        </p>
      </div>

      <div style={{ display: "flex", flexWrap: "wrap", gap: 22, alignItems: "flex-start" }}>
        <div style={{ flex: "1 1 440px", minWidth: 0, display: "flex", flexDirection: "column", gap: 22 }}>
          <Banner />
          <HistorySection history={history} />
        </div>
        <div style={{ flex: "0 0 300px", width: 300, maxWidth: "100%" }}>
          <StatsCard stats={stats} />
        </div>
      </div>
    </div>
  );
}
