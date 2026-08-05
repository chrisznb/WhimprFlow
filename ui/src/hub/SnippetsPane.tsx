import { useEffect, useState } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Button, Card, PageTitle } from "./ui";
import { Icon } from "./icons";
import { addSnippet, getSnippets, removeSnippet, type Snippet } from "./api";

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
        New snippet
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        <input
          value={trigger}
          onChange={(e) => setTrigger(e.target.value)}
          placeholder='Spoken trigger, e.g. "my email address"'
          style={inputStyle}
        />
        <textarea
          value={replacement}
          onChange={(e) => setReplacement(e.target.value)}
          placeholder="Expands to…"
          rows={3}
          style={{ ...inputStyle, resize: "vertical" }}
        />
        <div>
          <Button onClick={() => void submit()}>
            <Icon name="plus" size={14} /> Add snippet
          </Button>
        </div>
      </div>
    </Card>
  );
}

export function SnippetsPane() {
  const [items, setItems] = useState<Snippet[]>([]);
  const load = () => void getSnippets().then(setItems);
  useEffect(load, []);

  return (
    <div style={{ maxWidth: 860 }}>
      <PageTitle sub="Say the trigger phrase while dictating. WhimprFlow replaces it with the stored text.">
        Snippets
      </PageTitle>
      <AddForm onDone={load} />
      <Card pad={0}>
        {items.length === 0 ? (
          <div style={{ padding: "34px 18px", textAlign: "center", color: theme.textFaint, fontSize: 13.5 }}>
            No snippets yet. Add one above, e.g. “my email address”.
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
                title="Delete"
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
