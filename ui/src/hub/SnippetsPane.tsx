import { t } from "../i18n";
import { useEffect, useState } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Button, Card, PageTitle } from "./ui";
import { Icon } from "./icons";
import {
  addSnippet,
  getSnippets,
  getSnippetSuggestions,
  removeSnippet,
  type Snippet,
  type SnippetSuggestion,
} from "./api";

const inputStyle = {
  width: "100%",
  background: theme.cardBgSubtle,
  border: `1px solid ${theme.border}`,
  borderRadius: 10,
  padding: "9px 12px",
  color: theme.textBody,
  fontFamily: font.ui,
  fontSize: 13.5,
  outline: "none",
} as const;

function AddForm({ onDone }: { onDone: () => void }) {
  const [trigger, setTrigger] = useState("");
  const [replacement, setReplacement] = useState("");

  const submit = async () => {
    const t = trigger.trim();
    const r = replacement.trim();
    if (!t || !r) return;
    await addSnippet(t, r);
    setTrigger("");
    setReplacement("");
    onDone();
  };

  return (
    <Card style={{ marginBottom: 16 }}>
      <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong, marginBottom: 12 }}>
        {t("sn.new")}
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        <input
          value={trigger}
          onChange={(e) => setTrigger(e.target.value)}
          placeholder={t("sn.triggerPh")}
          style={inputStyle}
        />
        <textarea
          value={replacement}
          onChange={(e) => setReplacement(e.target.value)}
          placeholder={t("sn.expandsPh")}
          rows={3}
          style={{ ...inputStyle, resize: "vertical" }}
        />
        <div>
          <Button onClick={() => void submit()}>
            <Icon name="plus" size={14} /> {t("sn.add")}
          </Button>
        </div>
      </div>
    </Card>
  );
}

export function SnippetsPane() {
  const [items, setItems] = useState<Snippet[]>([]);
  const [suggestions, setSuggestions] = useState<SnippetSuggestion[]>([]);
  const load = () => {
    void getSnippets().then(setItems);
    void getSnippetSuggestions().then(setSuggestions);
  };
  useEffect(load, []);

  return (
    <div style={{ maxWidth: 860 }}>
      <PageTitle sub={t("sn.sub")}>
        {t("sn.title")}
      </PageTitle>
      <AddForm onDone={load} />
      {suggestions.length > 0 && (
        <Card style={{ marginBottom: 16, background: theme.lilacBg, borderColor: theme.lilacBorder }}>
          <div style={{ fontSize: 13.5, fontWeight: 650, color: theme.textStrong, marginBottom: 10 }}>
            {t("sn.suggest")}
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            {suggestions.map((sg) => (
              <div key={sg.phrase} style={{ display: "flex", alignItems: "center", gap: 10 }}>
                <div style={{ flex: 1, fontSize: 13, color: theme.textBody, overflowWrap: "anywhere" }}>
                  {"\u201C"}{sg.phrase}{"\u201D"} <span style={{ color: theme.textFaint }}>({sg.count}x)</span>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => void addSnippet(sg.phrase.split(" ").slice(0, 3).join(" "), sg.phrase).then(load)}
                >
                  {t("sn.save")}
                </Button>
              </div>
            ))}
          </div>
        </Card>
      )}
      <Card pad={0}>
        {items.length === 0 ? (
          <div style={{ padding: "34px 18px", textAlign: "center", color: theme.textFaint, fontSize: 13.5 }}>
            {t("sn.empty")}
          </div>
        ) : (
          items.map((s, i) => (
            <div
              key={s.trigger}
              className="wf-row"
              style={{
                display: "flex",
                alignItems: "flex-start",
                gap: 14,
                padding: "13px 18px",
                borderBottom: i < items.length - 1 ? `1px solid ${theme.border}` : "none",
              }}
            >
              <div style={{ flex: "0 0 220px", fontWeight: 600, fontSize: 13.5, color: theme.textStrong }}>
                {s.trigger}
              </div>
              <div
                style={{
                  flex: 1,
                  minWidth: 0,
                  fontSize: 13.5,
                  color: theme.textBody,
                  whiteSpace: "pre-wrap",
                  overflowWrap: "anywhere",
                }}
              >
                {s.replacement}
              </div>
              <button
                title={t("sn.delete")}
                onClick={() => void removeSnippet(s.trigger).then(load)}
                className="wf-press"
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
          ))
        )}
      </Card>
    </div>
  );
}
