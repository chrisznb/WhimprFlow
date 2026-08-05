import { useState } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Card, PageTitle, Tabs } from "./ui";
import type { Settings, StyleLevel, StylePrefs } from "./api";

type Category = keyof StylePrefs;

const CATEGORIES: { value: Category; label: string }[] = [
  { value: "personal", label: "Personal messages" },
  { value: "work", label: "Work messages" },
  { value: "email", label: "Email" },
  { value: "other", label: "Other" },
];

const LEVELS: {
  value: StyleLevel;
  title: string;
  subtitle: string;
  sample: Record<Category, string>;
}[] = [
  {
    value: "formal",
    title: "Formal.",
    subtitle: "Caps + Punctuation",
    sample: {
      personal: "Hey, are you free for lunch tomorrow? Let's do 12 if that works for you.",
      work: "Quick update: the draft is ready for review. Could you take a look today?",
      email: "Hi Alex,\n\nIt was great talking with you today. Looking forward to our next chat.\n\nBest,\nChris",
      other: "The meeting moved to Thursday at 3 p.m. Please update your calendar.",
    },
  },
  {
    value: "casual",
    title: "Casual",
    subtitle: "Caps + Less punctuation",
    sample: {
      personal: "Hey are you free for lunch tomorrow? Let's do 12 if that works for you",
      work: "Quick update: draft's ready for review, can you take a look today?",
      email: "Hi Alex, it was great talking with you today. Looking forward to our next chat.\n\nBest,\nChris",
      other: "Meeting moved to Thursday 3pm, update your calendar",
    },
  },
  {
    value: "very_casual",
    title: "very casual",
    subtitle: "No Caps + Less punctuation",
    sample: {
      personal: "hey are you free for lunch tomorrow? let's do 12 if that works for you",
      work: "draft's ready for review, can you look today",
      email: "hi alex, great talking today. looking forward to the next one\n\nbest\nchris",
      other: "meeting moved to thursday 3pm",
    },
  },
];

export function StylePane({
  settings,
  onChange,
}: {
  settings: Settings;
  onChange: (s: Settings) => void;
}) {
  const [cat, setCat] = useState<Category>("personal");
  const active = settings.style?.[cat] ?? "casual";

  const pick = (level: StyleLevel) => {
    onChange({ ...settings, style: { ...settings.style, [cat]: level } });
  };

  return (
    <div style={{ maxWidth: 980 }}>
      <PageTitle sub="How WhimprFlow formats your words, per app category, detected from where you dictate.">
        Style
      </PageTitle>
      <div style={{ marginBottom: 20 }}>
        <Tabs options={CATEGORIES} value={cat} onChange={setCat} />
      </div>
      <div style={{ display: "flex", gap: 18, flexWrap: "wrap" }}>
        {LEVELS.map((l) => {
          const selected = active === l.value;
          return (
            <Card
              key={l.value}
              style={{
                flex: "1 1 260px",
                minWidth: 240,
                cursor: "pointer",
                border: `1.5px solid ${selected ? theme.lilacBorder : theme.border}`,
                boxShadow: selected ? `0 0 0 3px ${theme.lilacBg}` : undefined,
              }}
            >
              <div onClick={() => pick(l.value)}>
                <div style={{ fontSize: 15, fontWeight: 650, color: theme.textStrong }}>{l.title}</div>
                <div style={{ fontSize: 12.5, color: theme.textMuted, marginTop: 2, marginBottom: 14 }}>
                  {l.subtitle}
                </div>
                <div
                  style={{
                    background: theme.lilacBg,
                    borderRadius: 12,
                    padding: "12px 14px",
                    fontSize: 13,
                    lineHeight: 1.5,
                    color: theme.textBody,
                    fontFamily: font.ui,
                    whiteSpace: "pre-wrap",
                    minHeight: 96,
                  }}
                >
                  {l.sample[cat]}
                </div>
              </div>
            </Card>
          );
        })}
      </div>
      <p style={{ color: theme.textFaint, fontSize: 12.5, marginTop: 18, lineHeight: 1.5 }}>
        Personal: WhatsApp, Telegram, Messages, Discord, Signal. Work: Slack, Teams. Email: Mail,
        Outlook. Everything else uses “Other”.
      </p>
    </div>
  );
}
