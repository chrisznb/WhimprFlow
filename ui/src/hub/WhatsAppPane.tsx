import { t } from "../i18n";
import { useEffect, useRef, useState } from "react";
import { theme } from "./theme";
import { Button, Card, PageTitle, SkeletonRows } from "./ui";
import { Icon } from "./icons";
import {
  waAccess,
  waChats,
  waMessages,
  waTranscribe,
  type WaAccess,
  type WaChat,
  type WaMessage,
} from "./api";

/// Transcribing all of a chat at once would block the list for minutes, so
/// messages are worked through one at a time, newest first, and each result
/// appears as soon as it lands. Opening another chat abandons the run: the
/// user's attention moved, so the work should follow it.
export function WhatsAppPane() {
  const [access, setAccess] = useState<WaAccess | null>(null);
  const [chats, setChats] = useState<WaChat[]>([]);
  const [active, setActive] = useState<WaChat | null>(null);
  const [messages, setMessages] = useState<WaMessage[]>([]);
  const [loading, setLoading] = useState(false);
  const [working, setWorking] = useState<number | null>(null);
  const [copied, setCopied] = useState<number | null>(null);
  // Bumped on every chat switch so an in-flight run can tell it is stale.
  const runId = useRef(0);

  useEffect(() => {
    void waAccess().then((a) => {
      setAccess(a);
      if (a === "ok") void waChats().then(setChats);
    });
  }, []);

  const openChat = async (c: WaChat) => {
    const run = ++runId.current;
    setActive(c);
    setLoading(true);
    setMessages([]);
    const msgs = await waMessages(c.id);
    if (runId.current !== run) return;
    setMessages(msgs);
    setLoading(false);
    void fillTranscripts(msgs, run);
  };

  const fillTranscripts = async (msgs: WaMessage[], run: number) => {
    for (const m of msgs) {
      if (runId.current !== run) return;
      if (m.text || !m.path) continue;
      setWorking(m.id);
      const text = await waTranscribe(m.id, m.path);
      if (runId.current !== run) return;
      if (text) {
        setMessages((prev) =>
          prev.map((x) => (x.id === m.id ? { ...x, text } : x)),
        );
      }
    }
    if (runId.current === run) setWorking(null);
  };

  const copy = (m: WaMessage) => {
    if (!m.text) return;
    void navigator.clipboard.writeText(m.text);
    setCopied(m.id);
    window.setTimeout(() => setCopied(null), 1400);
  };

  if (access === null) {
    return (
      <>
        <PageTitle sub={t("wa.sub")}>{t("wa.title")}</PageTitle>
        <SkeletonRows rows={4} />
      </>
    );
  }

  if (access !== "ok") {
    return (
      <>
        <PageTitle sub={t("wa.sub")}>{t("wa.title")}</PageTitle>
        <Card>
          <div style={{ display: "flex", gap: 14, alignItems: "flex-start" }}>
            <Icon name="settings" size={20} />
            <div>
              <strong style={{ color: theme.textStrong }}>
                {access === "denied" ? t("wa.denied.title") : t("wa.missing.title")}
              </strong>
              <p style={{ color: theme.textMuted, fontSize: 14, lineHeight: 1.6, margin: "8px 0 0" }}>
                {access === "denied" ? t("wa.denied.body") : t("wa.missing.body")}
              </p>
              {access === "denied" && (
                <div style={{ marginTop: 14 }}>
                <Button
                  onClick={() => {
                    void navigator.clipboard.writeText(
                      "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles",
                    );
                  }}
                >
                  {t("wa.denied.action")}
                </Button>
                </div>
              )}
            </div>
          </div>
        </Card>
      </>
    );
  }

  return (
    <>
      <PageTitle sub={t("wa.sub")}>{t("wa.title")}</PageTitle>
      <div style={{ display: "flex", gap: 20, alignItems: "flex-start" }}>
        <div style={{ width: 260, flexShrink: 0, display: "flex", flexDirection: "column", gap: 6 }}>
          {chats.length === 0 && (
            <p style={{ color: theme.textMuted, fontSize: 14 }}>{t("wa.empty")}</p>
          )}
          {chats.map((c) => (
            <button
              key={c.id}
              onClick={() => void openChat(c)}
              style={{
                textAlign: "left",
                border: "1px solid",
                borderColor: active?.id === c.id ? theme.accentSoftBorder : "transparent",
                background: active?.id === c.id ? theme.accentSoft : "transparent",
                borderRadius: 10,
                padding: "10px 12px",
                cursor: "pointer",
                display: "flex",
                alignItems: "center",
                gap: 10,
              }}
            >
              <Icon name={c.is_group ? "group" : "person"} size={16} />
              <span style={{ flex: 1, minWidth: 0 }}>
                <span
                  style={{
                    display: "block",
                    color: theme.textStrong,
                    fontSize: 14,
                    fontWeight: 600,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {c.name}
                </span>
                <span style={{ color: theme.textMuted, fontSize: 12 }}>
                  {t("wa.count", { n: c.count })}
                </span>
              </span>
            </button>
          ))}
        </div>

        <div style={{ flex: 1, minWidth: 0 }}>
          {!active && <p style={{ color: theme.textMuted, fontSize: 14 }}>{t("wa.pick")}</p>}
          {active && loading && <SkeletonRows rows={3} />}
          {active && !loading && (
            <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
              {messages.map((m) => (
                <Card key={m.id}>
                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "center",
                      gap: 12,
                      marginBottom: 8,
                    }}
                  >
                    <span style={{ color: theme.textMuted, fontSize: 12 }}>
                      {m.from_me ? t("wa.fromMe") : m.sender || active.name}
                      {" · "}
                      {formatWhen(m.sent_ms)}
                      {m.duration_s > 0 && ` · ${formatDuration(m.duration_s)}`}
                    </span>
                    {m.text && (
                      <Button onClick={() => copy(m)}>
                        {copied === m.id ? t("wa.copied") : t("wa.copy")}
                      </Button>
                    )}
                  </div>
                  {m.text ? (
                    <p
                      style={{
                        margin: 0,
                        color: theme.textBody,
                        fontSize: 14,
                        lineHeight: 1.6,
                        whiteSpace: "pre-wrap",
                      }}
                    >
                      {m.text}
                    </p>
                  ) : !m.path ? (
                    <p style={{ margin: 0, color: theme.textFaint, fontSize: 13 }}>
                      {t("wa.notDownloaded")}
                    </p>
                  ) : working === m.id ? (
                    <p style={{ margin: 0, color: theme.textMuted, fontSize: 13 }}>
                      {t("wa.working")}
                    </p>
                  ) : (
                    <p style={{ margin: 0, color: theme.textFaint, fontSize: 13 }}>
                      {t("wa.queued")}
                    </p>
                  )}
                </Card>
              ))}
            </div>
          )}
        </div>
      </div>
    </>
  );
}

/// Dates are shown relative for the last week, because "Tuesday" is easier to
/// place than a date when looking for a message from a few days ago.
function formatWhen(unixSeconds: number): string {
  const d = new Date(unixSeconds * 1000);
  const days = Math.floor((Date.now() - d.getTime()) / 86_400_000);
  const time = d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  if (days === 0) return `${t("wa.today")}, ${time}`;
  if (days === 1) return `${t("wa.yesterday")}, ${time}`;
  if (days < 7) return `${d.toLocaleDateString(undefined, { weekday: "long" })}, ${time}`;
  return `${d.toLocaleDateString()}, ${time}`;
}

function formatDuration(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}
