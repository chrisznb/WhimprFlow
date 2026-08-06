import { useEffect, useRef, useState } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Card, PageTitle, Skeleton } from "./ui";
import { Icon } from "./icons";
import { assistantChat } from "./api";

type Msg = { role: "user" | "assistant"; content: string; actions?: string[] };

export function AssistantPane() {
  const [msgs, setMsgs] = useState<Msg[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const endRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [msgs, busy]);

  const send = async () => {
    const text = input.trim();
    if (!text || busy) return;
    setInput("");
    const next: Msg[] = [...msgs, { role: "user", content: text }];
    setMsgs(next);
    setBusy(true);
    const history = next.map((m) => [m.role, m.content] as [string, string]);
    const res = await assistantChat(history);
    setMsgs((prev) => [
      ...prev,
      {
        role: "assistant",
        content: res?.reply || "No answer. Check your cleanup engine or API key in Settings.",
        actions: res?.actions_done ?? [],
      },
    ]);
    setBusy(false);
  };

  return (
    <div style={{ maxWidth: 860, display: "flex", flexDirection: "column", height: "calc(100vh - 140px)" }}>
      <PageTitle sub="Chat with your dictation app. It can add snippets and dictionary entries or write to the scratchpad when you ask.">
        Ask Whimpr
      </PageTitle>

      <Card pad={0} style={{ flex: 1, display: "flex", flexDirection: "column", minHeight: 0 }}>
        <div style={{ flex: 1, overflowY: "auto", padding: "18px 18px 6px", minHeight: 0 }}>
          {msgs.length === 0 && (
            <div style={{ padding: "40px 12px", textAlign: "center", color: theme.textFaint, fontSize: 13.5 }}>
              Try: "Leg ein Snippet an: meine Handynummer, plus 49 ..." or "Merk dir, dass Baulyo so
              geschrieben wird."
            </div>
          )}
          {msgs.map((m, i) => (
            <div
              key={i}
              className="wf-fade"
              style={{
                display: "flex",
                justifyContent: m.role === "user" ? "flex-end" : "flex-start",
                marginBottom: 10,
              }}
            >
              <div
                style={{
                  maxWidth: "78%",
                  background: m.role === "user" ? theme.lilacBg : theme.cardBgSubtle,
                  border: `1px solid ${m.role === "user" ? theme.lilacBorder : theme.border}`,
                  borderRadius: 14,
                  padding: "10px 14px",
                  fontSize: 13.5,
                  lineHeight: 1.55,
                  color: theme.textBody,
                  whiteSpace: "pre-wrap",
                  overflowWrap: "anywhere",
                }}
              >
                {m.content}
                {m.actions && m.actions.length > 0 && (
                  <div style={{ display: "flex", flexWrap: "wrap", gap: 6, marginTop: 8 }}>
                    {m.actions.map((a, k) => (
                      <span
                        key={k}
                        style={{
                          fontSize: 11,
                          fontWeight: 650,
                          color: theme.accentDeep,
                          background: theme.accentSoft,
                          borderRadius: 7,
                          padding: "3px 8px",
                        }}
                      >
                        {a}
                      </span>
                    ))}
                  </div>
                )}
              </div>
            </div>
          ))}
          {busy && (
            <div style={{ display: "flex", justifyContent: "flex-start", marginBottom: 10 }}>
              <div
                style={{
                  maxWidth: "60%",
                  minWidth: 220,
                  background: theme.cardBgSubtle,
                  border: `1px solid ${theme.border}`,
                  borderRadius: 14,
                  padding: "12px 14px",
                  display: "flex",
                  flexDirection: "column",
                  gap: 8,
                }}
              >
                <Skeleton h={11} />
                <Skeleton h={11} w="72%" />
              </div>
            </div>
          )}
          <div ref={endRef} />
        </div>
        <div style={{ display: "flex", gap: 8, padding: 14, borderTop: `1px solid ${theme.border}` }}>
          <input
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void send();
            }}
            placeholder="Type or dictate, then Enter"
            style={{
              flex: 1,
              background: theme.cardBgSubtle,
              border: `1px solid ${theme.border}`,
              borderRadius: 12,
              padding: "11px 14px",
              color: theme.textBody,
              fontFamily: font.ui,
              fontSize: 13.5,
              outline: "none",
            }}
          />
          <button
            className="wf-press"
            onClick={() => void send()}
            disabled={busy}
            style={{
              border: "none",
              background: theme.btnBg,
              color: theme.btnText,
              borderRadius: 12,
              padding: "0 16px",
              cursor: "pointer",
              display: "flex",
              alignItems: "center",
            }}
          >
            <Icon name="send" size={15} />
          </button>
        </div>
      </Card>
    </div>
  );
}
