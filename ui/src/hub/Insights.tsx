import { useEffect, useState } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Card, PageTitle, Skeleton, useStats } from "./ui";
import { Icon } from "./icons";
import { getVoiceProfile, type StatsSummary, type VoiceProfile } from "./api";
import { fmtCompact, fmtNum, newsArticles } from "./format";

// ── Semicircular gauge ───────────────────────────────────────────────────────
function Gauge({ value, max }: { value: number; max: number }) {
  const frac = Math.max(0, Math.min(1, value / max));
  const r = 58;
  const cx = 80;
  const cy = 72;
  const len = Math.PI * r;
  const d = `M ${cx - r} ${cy} A ${r} ${r} 0 0 1 ${cx + r} ${cy}`;
  return (
    <div style={{ position: "relative", width: 160, height: 88, margin: "0 auto" }}>
      <svg width="160" height="88" viewBox="0 0 160 88">
        <path d={d} fill="none" stroke={theme.track} strokeWidth="12" strokeLinecap="round" />
        <path
          d={d}
          fill="none"
          stroke={theme.accent}
          strokeWidth="12"
          strokeLinecap="round"
          strokeDasharray={len}
          strokeDashoffset={len * (1 - frac)}
        />
      </svg>
      <div
        style={{
          position: "absolute",
          left: 0,
          right: 0,
          bottom: 2,
          textAlign: "center",
        }}
      >
        <div style={{ fontFamily: font.serif, fontSize: 34, fontWeight: 600, color: theme.textStrong, lineHeight: 1 }}>
          {fmtNum(value)}
        </div>
      </div>
    </div>
  );
}

function StatCard({
  label,
  children,
  foot,
}: {
  label: string;
  children: React.ReactNode;
  foot?: React.ReactNode;
}) {
  return (
    <Card style={{ flex: "1 1 200px", minWidth: 0 }}>
      <div
        style={{
          fontSize: 11.5,
          fontWeight: 700,
          letterSpacing: 0.6,
          textTransform: "uppercase",
          color: theme.textFaint,
          marginBottom: 14,
        }}
      >
        {label}
      </div>
      {children}
      {foot && <div style={{ fontSize: 12.5, color: theme.textMuted, marginTop: 12, textAlign: "center" }}>{foot}</div>}
    </Card>
  );
}

function BigNumber({ value, accent }: { value: string; accent?: boolean }) {
  return (
    <div
      style={{
        fontFamily: font.serif,
        fontSize: 44,
        fontWeight: 600,
        lineHeight: 1,
        textAlign: "center",
        color: accent ? theme.accentDeep : theme.textStrong,
      }}
    >
      {value}
    </div>
  );
}

// ── 7-day bar chart ──────────────────────────────────────────────────────────
const DOW = ["S", "M", "T", "W", "T", "F", "S"];

function ActivityBars({ data }: { data: number[] }) {
  const max = Math.max(1, ...data);
  const todayIdx = new Date().getDay(); // 0..6, last bar = today
  return (
    <div>
      <div style={{ display: "flex", alignItems: "flex-end", gap: 8, height: 120 }}>
        {data.map((v, i) => (
          <div key={i} style={{ flex: 1, display: "flex", flexDirection: "column", justifyContent: "flex-end", height: "100%" }}>
            <div
              title={`${fmtNum(v)} words`}
              style={{
                height: `${v > 0 ? Math.max(6, (v / max) * 100) : 3}%`,
                background: v > 0 ? theme.accent : theme.track,
                borderRadius: 6,
                transition: "height 240ms ease",
              }}
            />
          </div>
        ))}
      </div>
      <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
        {data.map((_, i) => {
          // Map the 7 bars onto weekday initials ending at today.
          const dow = (todayIdx - (data.length - 1 - i) + 7) % 7;
          return (
            <div key={i} style={{ flex: 1, textAlign: "center", fontSize: 10.5, color: theme.textFaint }}>
              {i === data.length - 1 ? "Today" : DOW[dow]}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ── Contribution heatmap (illustrative) ──────────────────────────────────────
const HEAT_WEEKS = 12;

function level(v: number, max: number): number {
  if (v <= 0) return 0;
  const r = v / max;
  if (r < 0.25) return 1;
  if (r < 0.5) return 2;
  if (r < 0.75) return 3;
  return 4;
}

const HEAT_COLORS = [theme.track, "rgba(34,195,182,0.28)", "rgba(34,195,182,0.5)", "rgba(34,195,182,0.72)", theme.accentDeep];

function Heatmap({ last7 }: { last7: number[] }) {
  const max = Math.max(1, ...last7);
  const cols: number[][] = [];
  for (let w = 0; w < HEAT_WEEKS; w++) {
    const col: number[] = [];
    for (let day = 0; day < 7; day++) {
      // Only the most-recent week (rightmost column) carries real data.
      col.push(w === HEAT_WEEKS - 1 ? (last7[day] ?? 0) : 0);
    }
    cols.push(col);
  }
  return (
    <div style={{ display: "flex", gap: 4, overflowX: "auto" }}>
      {cols.map((col, w) => (
        <div key={w} style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          {col.map((v, day) => (
            <div
              key={day}
              title={v > 0 ? `${fmtNum(v)} words` : "no activity"}
              style={{
                width: 13,
                height: 13,
                borderRadius: 3.5,
                background: HEAT_COLORS[level(v, max)],
              }}
            />
          ))}
        </div>
      ))}
    </div>
  );
}

// ── Tabs ─────────────────────────────────────────────────────────────────────
type Tab = "usage" | "voice";

function Tabs({ tab, onChange }: { tab: Tab; onChange: (t: Tab) => void }) {
  const items: { key: Tab; label: string }[] = [
    { key: "usage", label: "Your Usage" },
    { key: "voice", label: "Your Voice" },
  ];
  return (
    <div style={{ display: "flex", gap: 24, borderBottom: `1px solid ${theme.border}`, marginBottom: 22 }}>
      {items.map((it) => {
        const active = tab === it.key;
        return (
          <button
            key={it.key}
            onClick={() => onChange(it.key)}
            style={{
              border: "none",
              background: "transparent",
              cursor: "pointer",
              fontFamily: font.ui,
              fontSize: 14,
              fontWeight: active ? 600 : 500,
              color: active ? theme.textStrong : theme.textMuted,
              padding: "0 0 12px",
              marginBottom: -1,
              borderBottom: `2px solid ${active ? theme.accent : "transparent"}`,
            }}
          >
            {it.label}
          </button>
        );
      })}
    </div>
  );
}

function UsageTab({ stats }: { stats: StatsSummary }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
      {/* Top row — three stat cards */}
      <div style={{ display: "flex", flexWrap: "wrap", gap: 18 }}>
        <StatCard label="Words per minute" foot="Top 5% of dictators">
          <Gauge value={stats.avg_wpm} max={140} />
        </StatCard>

        <StatCard label="Fixes made by WhimprFlow" foot="dictations cleaned">
          <BigNumber value={fmtCompact(stats.total_sessions)} accent />
        </StatCard>

        <StatCard label="Total words dictated" foot={`≈ ${fmtNum(newsArticles(stats.total_words))} news articles`}>
          <BigNumber value={fmtCompact(stats.total_words)} />
        </StatCard>
      </div>

      {/* Bottom row — activity + streak */}
      <div style={{ display: "flex", flexWrap: "wrap", gap: 18 }}>
        <Card style={{ flex: "1 1 340px", minWidth: 0 }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 16 }}>
            <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>7-day activity</div>
            <div style={{ fontSize: 12, color: theme.textFaint }}>{fmtNum(stats.words_today)} today</div>
          </div>
          <ActivityBars data={stats.last7_words} />
        </Card>

        <Card style={{ flex: "1 1 300px", minWidth: 0 }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 16 }}>
            <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>Streak</div>
            <div style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 13, fontWeight: 600, color: theme.accentDeep }}>
              <Icon name="flame" size={14} /> {stats.day_streak} {stats.day_streak === 1 ? "day" : "days"}
            </div>
          </div>
          <Heatmap last7={stats.last7_words} />
          <div style={{ fontSize: 12, color: theme.textFaint, marginTop: 14 }}>
            Each square is a day. Keep the streak alive by dictating something every day.
          </div>
        </Card>
      </div>
    </div>
  );
}

function QuoteCard({ quote, label }: { quote: string; label: string }) {
  return (
    <Card style={{ flex: "1 1 260px", minWidth: 220 }}>
      <div
        style={{
          fontFamily: font.serif,
          fontStyle: "italic",
          fontSize: 24,
          fontWeight: 500,
          color: theme.textStrong,
          marginBottom: 12,
          overflowWrap: "anywhere",
        }}
      >
        {"\u201C"}{quote || "..."}{"\u201D"}
      </div>
      <div
        style={{
          fontSize: 11,
          fontWeight: 700,
          letterSpacing: 0.6,
          textTransform: "uppercase",
          color: theme.textFaint,
        }}
      >
        {label}
      </div>
    </Card>
  );
}

function VoiceTab() {
  const [profile, setProfile] = useState<VoiceProfile | null>(null);
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    void getVoiceProfile().then((p) => {
      setProfile(p);
      setLoading(false);
    });
  }, []);

  if (loading) {
    return (
      <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
        <Skeleton h={6} w="100%" />
        <Card>
          <Skeleton h={22} w={180} style={{ marginBottom: 14 }} />
          <Skeleton h={12} style={{ marginBottom: 8 }} />
          <Skeleton h={12} w="88%" style={{ marginBottom: 8 }} />
          <Skeleton h={12} w="64%" />
        </Card>
        <div style={{ display: "flex", gap: 18 }}>
          <Card style={{ flex: 1 }}>
            <Skeleton h={26} w="70%" style={{ marginBottom: 12 }} />
            <Skeleton h={10} w={90} />
          </Card>
          <Card style={{ flex: 1 }}>
            <Skeleton h={26} w="55%" style={{ marginBottom: 12 }} />
            <Skeleton h={10} w={110} />
          </Card>
        </div>
      </div>
    );
  }
  if (!profile || (!profile.profile_text && !profile.catchphrase)) {
    return (
      <Card>
        <div style={{ padding: "34px 8px", textAlign: "center", color: theme.textFaint, fontSize: 13.5 }}>
          Keep dictating. Your voice profile appears here once there is enough to analyze.
        </div>
      </Card>
    );
  }

  const sinceGen = profile.total_words - profile.generated_at_words;
  const frac = Math.min(1, sinceGen / profile.regen_after_words);
  const remaining = Math.max(0, profile.regen_after_words - sinceGen);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
      <div>
        <div style={{ height: 6, borderRadius: 999, background: theme.track, overflow: "hidden" }}>
          <div
            style={{
              width: `${Math.max(2, frac * 100)}%`,
              height: "100%",
              borderRadius: 999,
              background: theme.lilacBorder,
              transition: "width 600ms ease",
            }}
          />
        </div>
        <div style={{ display: "flex", justifyContent: "space-between", marginTop: 6, fontSize: 12, color: theme.textFaint }}>
          <span>Based on {fmtNum(profile.generated_at_words)} words</span>
          <span>Next update in {fmtNum(remaining)} more words</span>
        </div>
      </div>

      <Card>
        <div style={{ fontFamily: font.serif, fontSize: 26, fontWeight: 600, color: theme.textStrong }}>
          Voice Profile
        </div>
        <div
          style={{
            fontSize: 11,
            fontWeight: 700,
            letterSpacing: 0.6,
            textTransform: "uppercase",
            color: theme.textFaint,
            margin: "4px 0 14px",
          }}
        >
          How you dictate
        </div>
        <p style={{ fontSize: 15, lineHeight: 1.6, color: theme.textBody, margin: 0, maxWidth: 720 }}>
          {profile.profile_text || "Not enough dictations yet for a written profile."}
        </p>
      </Card>

      <div style={{ display: "flex", flexWrap: "wrap", gap: 18 }}>
        <QuoteCard quote={profile.catchphrase} label="Catchphrase" />
        <QuoteCard quote={profile.most_used_word} label="Most used word" />
        {profile.most_corrected_word && (
          <QuoteCard quote={profile.most_corrected_word} label="Most corrected word" />
        )}
      </div>

      {profile.peak_time && (
        <Card>
          <div style={{ fontFamily: font.serif, fontSize: 22, fontWeight: 600, color: theme.textStrong }}>
            {profile.peak_time}
          </div>
          <div
            style={{
              fontSize: 11,
              fontWeight: 700,
              letterSpacing: 0.6,
              textTransform: "uppercase",
              color: theme.textFaint,
              marginTop: 4,
            }}
          >
            Your peak time
          </div>
        </Card>
      )}
    </div>
  );
}

export function Insights() {
  const stats = useStats();
  const [tab, setTab] = useState<Tab>("usage");
  return (
    <div style={{ maxWidth: 1000 }}>
      <PageTitle>Insights</PageTitle>
      <Tabs tab={tab} onChange={setTab} />
      {tab === "usage" ? <UsageTab stats={stats} /> : <VoiceTab />}
    </div>
  );
}
