import { t } from "../i18n";
import { useEffect, useMemo, useState } from "react";
import { font, palette } from "../tokens/values";
import { theme } from "./theme";
import { Card, SkeletonRows, useStats } from "./ui";
import { Icon } from "./icons";
import { getHistory, type HistoryItem, type StatsSummary } from "./api";
import { dayKey, dayLabel, fmtCompact, fmtDuration, fmtNum, fmtTimeOfDay, prettyApp, wordsReference } from "./format";
import { ModelDownload, useModelStatus } from "./models";

const UNLOCK_WORDS = 500;

function countWords(t: string): number {
  const s = t.trim();
  return s ? s.split(/\s+/).length : 0;
}

function greeting(): string {
  const h = new Date().getHours();
  if (h < 5) return t("greet.late");
  if (h < 12) return t("greet.morning");
  if (h < 18) return t("greet.afternoon");
  return t("greet.evening");
}

// Small keyboard-key chip, light and dark variants.
function KeyCap({ label, dark }: { label: string; dark?: boolean }) {
  return (
    <span
      style={{
        display: "inline-block",
        fontFamily: font.ui,
        fontSize: 11,
        fontWeight: 600,
        lineHeight: 1,
        padding: "4px 8px 5px",
        borderRadius: 6,
        border: `1px solid ${dark ? "rgba(255,255,255,0.30)" : theme.borderStrong}`,
        background: dark ? "rgba(255,255,255,0.10)" : theme.cardBgSubtle,
        color: dark ? palette.slate050 : theme.textBody,
        boxShadow: dark ? "none" : "0 1px 0 rgba(26,26,26,0.06)",
        verticalAlign: "middle",
      }}
    >
      {label}
    </span>
  );
}

// ── First-launch setup ───────────────────────────────────────────────────────
function SetupCard({ onDone }: { onDone: () => void }) {
  return (
    <Card>
      <div style={{ display: "flex", gap: 16, alignItems: "flex-start", flexWrap: "wrap" }}>
        <div
          style={{
            width: 42,
            height: 42,
            borderRadius: "50%",
            background: theme.accentSoft,
            color: theme.accentDeep,
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
            flex: "0 0 auto",
          }}
        >
          <Icon name="mic" size={17} />
        </div>
        <div style={{ flex: "1 1 260px", minWidth: 0 }}>
          <div style={{ fontFamily: font.serif, fontSize: 20, fontWeight: 600, color: theme.textStrong }}>
            {t("setup.title")}
          </div>
          <p style={{ fontSize: 13.5, color: theme.textMuted, lineHeight: 1.6, margin: "6px 0 12px" }}>
            {t("setup.body")}
          </p>
          <ModelDownload kind="asr" sizeLabel="547 MB" onDone={onDone} />
        </div>
      </div>
    </Card>
  );
}

// ── Banner ───────────────────────────────────────────────────────────────────

// Frozen waveform decoration; fixed heights so it stays calm.
const EQ_HEIGHTS = [10, 22, 14, 30, 18, 38, 26, 44, 20, 34, 14, 26, 10];

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
      <div
        className="wf-banner-eq"
        style={{
          position: "absolute",
          right: 30,
          top: "50%",
          transform: "translateY(-50%)",
          display: "flex",
          alignItems: "center",
          gap: 5,
          pointerEvents: "none",
        }}
      >
        {EQ_HEIGHTS.map((h, i) => (
          <span
            key={i}
            style={{
              width: 4,
              height: h,
              borderRadius: 999,
              background:
                i === Math.floor(EQ_HEIGHTS.length / 2)
                  ? "rgba(255,255,255,0.55)"
                  : "rgba(255,255,255,0.22)",
            }}
          />
        ))}
      </div>
      <div style={{ position: "relative", maxWidth: 430 }}>
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
          {t("banner.title")}
        </div>
        <p style={{ color: palette.slate300, fontSize: 14, lineHeight: 1.55, margin: "10px 0 0" }}>
          {t("banner.body")}
        </p>
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginTop: 14 }}>
          <KeyCap label="fn" dark />
          <span style={{ fontSize: 12.5, color: palette.slate300 }}>{t("banner.hint")}</span>
        </div>
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

const CLAMP_STYLE = {
  display: "-webkit-box",
  WebkitLineClamp: 3,
  WebkitBoxOrient: "vertical",
  overflow: "hidden",
} as const;

function HistoryRow({ item }: { item: HistoryItem }) {
  const d = new Date(item.ts_unix * 1000);
  const [copied, setCopied] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const words = item.words || countWords(item.text);
  const clampable = words > 40 || item.text.length > 220;
  const copy = (e: React.MouseEvent) => {
    e.stopPropagation();
    void navigator.clipboard.writeText(item.text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    });
  };
  return (
    <div
      className="wf-row"
      onClick={() => clampable && setExpanded((v) => !v)}
      style={{
        display: "flex",
        gap: 14,
        padding: "12px 8px",
        margin: "0 -8px",
        borderRadius: 10,
        borderBottom: `1px solid ${theme.border}`,
        cursor: clampable ? "pointer" : "default",
      }}
    >
      <div
        style={{
          flex: "0 0 68px",
          fontSize: 12,
          color: theme.textFaint,
          fontVariantNumeric: "tabular-nums",
          paddingTop: 2,
        }}
      >
        {fmtTimeOfDay(d)}
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div
          style={{
            fontSize: 13.5,
            lineHeight: 1.55,
            color: theme.textBody,
            ...(clampable && !expanded ? CLAMP_STYLE : {}),
          }}
        >
          {item.text}
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginTop: 6 }}>
          {item.app && (
            <span
              style={{
                fontSize: 10.5,
                fontWeight: 600,
                color: theme.textMuted,
                background: theme.track,
                borderRadius: 999,
                padding: "2px 9px 3px",
              }}
            >
              {prettyApp(item.app)}
            </span>
          )}
          <span style={{ fontSize: 11, color: theme.textFaint }}>
            {fmtNum(words)} {words === 1 ? t("row.word") : t("row.words")}
          </span>
          {clampable && (
            <span style={{ fontSize: 11, color: theme.accentDeep, fontWeight: 600 }}>
              {expanded ? t("row.showLess") : t("row.showMore")}
            </span>
          )}
        </div>
      </div>
      <div className="wf-row-actions" style={{ flex: "0 0 auto", display: "flex", alignItems: "flex-start" }}>
        <button
          title={copied ? t("tr.copied") : t("tr.copy")}
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

function EmptyHistory() {
  return (
    <div style={{ padding: "44px 8px 40px", textAlign: "center" }}>
      <div
        style={{
          width: 46,
          height: 46,
          borderRadius: "50%",
          background: theme.accentSoft,
          color: theme.accentDeep,
          display: "inline-flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Icon name="mic" size={19} />
      </div>
      <div
        style={{
          fontFamily: font.serif,
          fontSize: 19,
          fontWeight: 600,
          color: theme.textStrong,
          marginTop: 14,
        }}
      >
        {t("history.empty.title")}
      </div>
      <div style={{ fontSize: 13, color: theme.textMuted, marginTop: 6, lineHeight: 1.6 }}>
        {t("history.empty.before")} <KeyCap label="fn" /> {t("history.empty.after")}
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
          flexWrap: "wrap",
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
          {t("history.title")}
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
            placeholder={t("history.search")}
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
          <EmptyHistory />
        ) : filtered.length === 0 ? (
          <div style={{ padding: "36px 8px", textAlign: "center", color: theme.textFaint, fontSize: 13.5 }}>
            {t("history.noMatch", { q: query })}
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

// The last 7 calendar days from the stats backend, oldest first (index 6 = today).
function lastSevenDays(last7: number[]): { letter: string; words: number; isToday: boolean }[] {
  const letters = t("week.letters").split(",");
  const now = new Date();
  const out: { letter: string; words: number; isToday: boolean }[] = [];
  for (let i = 6; i >= 0; i--) {
    const d = new Date(now.getFullYear(), now.getMonth(), now.getDate() - i);
    out.push({ letter: letters[d.getDay()], words: last7[6 - i] ?? 0, isToday: i === 0 });
  }
  return out;
}

function WeekChart({ last7 }: { last7: number[] }) {
  const days = useMemo(() => lastSevenDays(last7), [last7]);
  const total = days.reduce((s, d) => s + d.words, 0);
  const max = Math.max(...days.map((d) => d.words), 1);
  const CHART_H = 42;
  return (
    <div style={{ padding: "6px 0 0" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 10 }}>
        <div
          style={{
            fontSize: 10.5,
            fontWeight: 700,
            letterSpacing: 0.6,
            textTransform: "uppercase",
            color: theme.textFaint,
          }}
        >
          {t("stats.thisWeek")}
        </div>
        <div style={{ fontSize: 11.5, color: theme.textMuted, fontVariantNumeric: "tabular-nums" }}>
          {t("stats.weekWords", { n: fmtCompact(total) })}
        </div>
      </div>
      <div style={{ display: "flex", gap: 7, alignItems: "flex-end", height: CHART_H }}>
        {days.map((d, i) => (
          <div
            key={i}
            title={t("in.nWords", { n: fmtNum(d.words) })}
            style={{ flex: 1, display: "flex", flexDirection: "column", justifyContent: "flex-end", height: "100%" }}
          >
            <div
              style={{
                height: d.words > 0 ? Math.max(5, Math.round((d.words / max) * CHART_H)) : 4,
                borderRadius: 4,
                background: d.isToday
                  ? theme.accent
                  : d.words > 0
                    ? "rgba(42,99,88,0.30)"
                    : theme.track,
                transition: "height 240ms cubic-bezier(0.2, 0.7, 0.3, 1)",
              }}
            />
          </div>
        ))}
      </div>
      <div style={{ display: "flex", gap: 7, marginTop: 6 }}>
        {days.map((d, i) => (
          <div
            key={i}
            style={{
              flex: 1,
              textAlign: "center",
              fontSize: 9.5,
              fontWeight: d.isToday ? 700 : 500,
              color: d.isToday ? theme.accentDeep : theme.textFaint,
            }}
          >
            {d.letter}
          </div>
        ))}
      </div>
    </div>
  );
}

function StatsCard({ stats }: { stats: StatsSummary }) {
  const unlocked = stats.total_words >= UNLOCK_WORDS;
  return (
    <Card>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 4 }}>
        <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>{t("stats.title")}</div>
        {stats.day_streak >= 2 && (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 5,
              fontSize: 12,
              color: theme.accentDeep,
              fontWeight: 700,
            }}
          >
            <Icon name="flame" size={13} /> {t("stats.days", { n: stats.day_streak })}
          </div>
        )}
      </div>

      <div style={{ display: "flex", flexWrap: "wrap", gap: "12px 32px", alignItems: "center", marginTop: 8 }}>
        <div style={{ flex: "1 1 170px", textAlign: "center", padding: "8px 0" }}>
          <div style={{ fontFamily: font.serif, fontSize: 42, fontWeight: 600, color: theme.textStrong, lineHeight: 1 }}>
            {fmtCompact(stats.total_words)}
          </div>
          <div style={{ fontSize: 11.5, color: theme.textFaint, marginTop: 6, textTransform: "uppercase", letterSpacing: 0.6 }}>
            {t("stats.totalWords")}
          </div>
          <div style={{ fontSize: 12, color: theme.textMuted, marginTop: 5 }}>
            {wordsReference(stats.total_words)}
          </div>
        </div>
        <div style={{ flex: "1 1 230px", minWidth: 0 }}>
          <WeekChart last7={stats.last7_words} />
        </div>
      </div>

      <div
        style={{
          display: "flex",
          gap: 8,
          padding: "16px 0 0",
          marginTop: 16,
          borderTop: `1px solid ${theme.border}`,
        }}
      >
        <BigStat value={fmtNum(stats.avg_wpm)} label={t("stats.avgWpm")} accent />
        <BigStat value={fmtNum(stats.best_wpm)} label={t("stats.bestWpm")} />
        <BigStat value={`${stats.day_streak}`} label={t("stats.dayStreak")} />
      </div>

      {unlocked ? (
        <div style={{ fontSize: 12, color: theme.textFaint, textAlign: "center", marginTop: 14 }}>
          {t("stats.saved", { t: fmtDuration(stats.time_saved_secs) })}
        </div>
      ) : (
        <div style={{ fontSize: 12, color: theme.textFaint, textAlign: "center", marginTop: 14, lineHeight: 1.5 }}>
          {t("stats.unlock", { n: fmtNum(Math.max(0, UNLOCK_WORDS - stats.total_words)) })}
        </div>
      )}
    </Card>
  );
}

// ── Page ─────────────────────────────────────────────────────────────────────
export function Home() {
  const stats = useStats();
  const [history, setHistory] = useState<HistoryItem[] | null>(null);
  const { status: models, refresh: refreshModels } = useModelStatus();

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
      <div style={{ marginBottom: 24 }}>
        <h1
          style={{
            fontFamily: font.serif,
            fontSize: 34,
            fontWeight: 600,
            letterSpacing: -0.4,
            margin: 0,
            color: theme.textStrong,
            lineHeight: 1.1,
          }}
        >
          {greeting()}.
        </h1>
        <p style={{ color: theme.textMuted, fontSize: 14.5, margin: "9px 0 0" }}>
          {today > 0
            ? t("home.wordsToday", { n: fmtNum(today) })
            : t("home.ready")}
        </p>
      </div>

      <div className="wf-home-grid">
        <div style={{ minWidth: 0, display: "flex", flexDirection: "column", gap: 22 }}>
          {models !== null && !models.asr && <SetupCard onDone={refreshModels} />}
          <Banner />
          <HistorySection history={history} />
        </div>
        <div className="wf-home-side" style={{ position: "sticky", top: 24 }}>
          <StatsCard stats={stats} />
        </div>
      </div>
    </div>
  );
}
