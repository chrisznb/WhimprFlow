import { getLang, t } from "../i18n";
import { useState } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Card, PageTitle, Tabs } from "./ui";
import type { Settings, StyleLevel, StylePrefs } from "./api";

type Category = keyof StylePrefs;

const CATEGORIES: () => { value: Category; label: string }[] = () => [
  { value: "personal", label: t("st.personal") },
  { value: "work", label: t("st.work") },
  { value: "email", label: t("st.email") },
  { value: "other", label: t("st.other") },
];

const SAMPLES_DE: Record<StyleLevel, Record<Category, string>> = {
  formal: {
    personal: "Hey, hast du morgen Zeit für ein Mittagessen? 12 Uhr würde mir passen.",
    work: "Kurzes Update: Der Entwurf ist bereit zum Gegenlesen. Schaffst du das heute?",
    email: "Hallo Alex,\n\nschön, dass wir heute gesprochen haben. Ich freue mich auf das nächste Mal.\n\nViele Grüße\nChris",
    other: "Das Treffen wurde auf Donnerstag 15 Uhr verschoben. Bitte den Kalender aktualisieren.",
  },
  casual: {
    personal: "Hey hast du morgen Zeit für Mittagessen? 12 würde mir passen",
    work: "Kurzes Update: Entwurf ist fertig zum Gegenlesen, schaffst du das heute?",
    email: "Hallo Alex, schön, dass wir heute gesprochen haben. Freu mich aufs nächste Mal.\n\nViele Grüße\nChris",
    other: "Treffen auf Donnerstag 15 Uhr verschoben, Kalender aktualisieren",
  },
  very_casual: {
    personal: "hey hast du morgen zeit für mittagessen? 12 würde passen",
    work: "entwurf ist fertig, schaffst du heute nen blick drauf",
    email: "hallo alex, schön gesprochen zu haben. bis zum nächsten mal\n\nlg\nchris",
    other: "treffen auf donnerstag 15 uhr verschoben",
  },
};

const LEVELS: {
  value: StyleLevel;
  title: string;
  subtitle: string;
  sample: Record<Category, string>;
}[] = [
  {
    value: "formal",
    title: "",
    subtitle: "",
    sample: {
      personal: "Hey, are you free for lunch tomorrow? Let's do 12 if that works for you.",
      work: "Quick update: the draft is ready for review. Could you take a look today?",
      email: "Hi Alex,\n\nIt was great talking with you today. Looking forward to our next chat.\n\nBest,\nChris",
      other: "The meeting moved to Thursday at 3 p.m. Please update your calendar.",
    },
  },
  {
    value: "casual",
    title: "",
    subtitle: "",
    sample: {
      personal: "Hey are you free for lunch tomorrow? Let's do 12 if that works for you",
      work: "Quick update: draft's ready for review, can you take a look today?",
      email: "Hi Alex, it was great talking with you today. Looking forward to our next chat.\n\nBest,\nChris",
      other: "Meeting moved to Thursday 3pm, update your calendar",
    },
  },
  {
    value: "very_casual",
    title: "",
    subtitle: "",
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
      <PageTitle sub={t("st.sub")}>
        {t("st.title")}
      </PageTitle>
      <div style={{ marginBottom: 20 }}>
        <Tabs options={CATEGORIES()} value={cat} onChange={setCat} />
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
                <div style={{ fontSize: 15, fontWeight: 650, color: theme.textStrong }}>
                  {l.value === "formal" ? t("st.formal") : l.value === "casual" ? t("st.casual") : t("st.veryCasual")}
                </div>
                <div style={{ fontSize: 12.5, color: theme.textMuted, marginTop: 2, marginBottom: 14 }}>
                  {l.value === "formal" ? t("st.formalSub") : l.value === "casual" ? t("st.casualSub") : t("st.veryCasualSub")}
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
                  {getLang() === "de" ? SAMPLES_DE[l.value][cat] : l.sample[cat]}
                </div>
              </div>
            </Card>
          );
        })}
      </div>
      <p style={{ color: theme.textFaint, fontSize: 12.5, marginTop: 18, lineHeight: 1.5 }}>
        {t("st.footer")}
      </p>
    </div>
  );
}
