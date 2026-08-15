import { t } from "../i18n";
import { useEffect, useState } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Button, Card, PageTitle } from "./ui";
import { Icon } from "./icons";
import { getTransforms, setTransforms, type Transform } from "./api";

const inputStyle = {
  width: "100%",
  background: theme.cardBgSubtle,
  border: `1px solid ${theme.border}`,
  borderRadius: 10,
  padding: "8px 11px",
  color: theme.textBody,
  fontFamily: font.ui,
  fontSize: 13,
  outline: "none",
} as const;

function ShortcutChip({ value }: { value: string }) {
  const pretty = value.replace(/Alt\+/i, "⌥ ").replace(/Cmd\+/i, "⌘ ").replace(/Shift\+/i, "⇧ ");
  return (
    <span
      style={{
        display: "inline-block",
        border: `1px solid ${theme.borderStrong}`,
        borderRadius: 7,
        padding: "2px 8px",
        fontSize: 11.5,
        fontWeight: 650,
        color: theme.textBody,
        background: theme.cardBgSubtle,
      }}
    >
      {pretty}
    </span>
  );
}

export function TransformsPane() {
  const [items, setItems] = useState<Transform[]>([]);
  const [editing, setEditing] = useState<number | null>(null);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    void getTransforms().then(setItems);
  }, []);

  const persist = async (next: Transform[]) => {
    setItems(next);
    await setTransforms(next);
    setSaved(true);
    setTimeout(() => setSaved(false), 1500);
  };

  const update = (i: number, patch: Partial<Transform>) => {
    setItems((prev) => prev.map((t, k) => (k === i ? { ...t, ...patch } : t)));
  };

  const addNew = () => {
    const used = new Set(items.map((t) => t.shortcut));
    let slot = 1;
    while (used.has(`Alt+${slot}`) && slot < 10) slot += 1;
    const next = [
      ...items,
      { name: t("tf.newName"), shortcut: `Alt+${slot}`, prompt: t("tf.newPrompt") },
    ];
    setItems(next);
    setEditing(next.length - 1);
  };

  return (
    <div style={{ maxWidth: 900 }}>
      <PageTitle sub={t("tf.sub")}>
        {t("tf.title")}
      </PageTitle>

      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          justifyContent: "space-between",
          marginBottom: 14,
        }}
      >
        <div style={{ fontFamily: font.serif, fontSize: 24, fontWeight: 600, color: theme.textStrong }}>
          {t("tf.mine")}
        </div>
        <div style={{ display: "flex", gap: 10, alignItems: "center" }}>
          {saved && <span style={{ fontSize: 12.5, color: theme.accent }}>{t("tf.saved")}</span>}
          <Button size="sm" onClick={addNew}>
            <Icon name="plus" size={13} /> {t("tf.create")}
          </Button>
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
        {items.map((tf, i) => (
          <Card key={i}>
            <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 8 }}>
              <ShortcutChip value={tf.shortcut} />
              {editing === i ? (
                <input
                  value={tf.name}
                  onChange={(e) => update(i, { name: e.target.value })}
                  style={{ ...inputStyle, maxWidth: 260 }}
                />
              ) : (
                <div style={{ fontSize: 14.5, fontWeight: 650, color: theme.textStrong }}>{tf.name}</div>
              )}
              <div style={{ flex: 1 }} />
              <button
                className="wf-press"
                onClick={() => (editing === i ? void persist(items).then(() => setEditing(null)) : setEditing(i))}
                style={{
                  border: "none",
                  background: theme.track,
                  borderRadius: 8,
                  padding: "5px 12px",
                  cursor: "pointer",
                  fontFamily: font.ui,
                  fontSize: 12.5,
                  fontWeight: 600,
                  color: theme.textBody,
                }}
              >
                {editing === i ? t("tf.save") : t("tf.edit")}
              </button>
              <button
                title={t("tf.delete")}
                className="wf-press"
                onClick={() => void persist(items.filter((_, k) => k !== i))}
                style={{
                  border: "none",
                  background: "transparent",
                  cursor: "pointer",
                  color: theme.textFaint,
                  padding: 2,
                }}
              >
                <Icon name="close" size={15} />
              </button>
            </div>
            {editing === i ? (
              <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                <input
                  value={tf.shortcut}
                  onChange={(e) => update(i, { shortcut: e.target.value })}
                  placeholder={t("tf.shortcutPh")}
                  style={{ ...inputStyle, maxWidth: 180 }}
                />
                <textarea
                  value={tf.prompt}
                  onChange={(e) => update(i, { prompt: e.target.value })}
                  rows={3}
                  style={{ ...inputStyle, resize: "vertical" }}
                />
              </div>
            ) : (
              <div style={{ fontSize: 13, color: theme.textMuted, lineHeight: 1.5 }}>{tf.prompt}</div>
            )}
          </Card>
        ))}
      </div>
    </div>
  );
}
