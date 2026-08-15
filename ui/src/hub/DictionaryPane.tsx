import { t } from "../i18n";
import { useEffect, useState } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Button, Card } from "./ui";
import { Icon } from "./icons";
import {
  addDictionaryEntry,
  getDictionary,
  removeDictionaryEntry,
  type DictEntry,
} from "./api";

type Tab = "all" | "personal" | "shared";

const TABS: () => { key: Tab; label: string }[] = () => [
  { key: "all", label: t("dict.all") },
  { key: "personal", label: t("dict.personal") },
  { key: "shared", label: t("dict.shared") },
];

function Tabs({ tab, onChange }: { tab: Tab; onChange: (t: Tab) => void }) {
  return (
    <div style={{ display: "flex", gap: 22, borderBottom: `1px solid ${theme.border}`, marginBottom: 18 }}>
      {TABS().map((it) => {
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
              fontSize: 13.5,
              fontWeight: active ? 600 : 500,
              color: active ? theme.textStrong : theme.textMuted,
              padding: "0 0 11px",
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

function AddForm({ onDone }: { onDone: () => void }) {
  const [correct, setCorrect] = useState("");
  const [heard, setHeard] = useState("");
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

  const submit = async () => {
    const word = correct.trim();
    if (!word) return;
    const mishears = heard
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    await addDictionaryEntry(word, mishears);
    onDone();
  };

  return (
    <Card style={{ marginBottom: 16, borderColor: theme.accentSoftBorder }}>
      <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong, marginBottom: 12 }}>{t("dict.addWord")}</div>
      <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        <div>
          <label style={{ fontSize: 12, color: theme.textMuted, display: "block", marginBottom: 5 }}>{t("dict.word")}</label>
          <input
            autoFocus
            value={correct}
            onChange={(e) => setCorrect(e.target.value)}
            placeholder="e.g. WhimprFlow"
            style={inputStyle}
            onKeyDown={(e) => {
              if (e.key === "Enter") void submit();
            }}
          />
        </div>
        <div>
          <label style={{ fontSize: 12, color: theme.textMuted, display: "block", marginBottom: 5 }}>
            {t("dict.alsoHeard")} <span style={{ color: theme.textFaint }}>{t("dict.optionalComma")}</span>
          </label>
          <input
            value={heard}
            onChange={(e) => setHeard(e.target.value)}
            placeholder="whisper flow, wimper flow"
            style={inputStyle}
            onKeyDown={(e) => {
              if (e.key === "Enter") void submit();
            }}
          />
        </div>
      </div>
      <div style={{ display: "flex", gap: 8, marginTop: 14 }}>
        <Button variant="accent" onClick={() => void submit()}>
          {t("dict.addBtn")}
        </Button>
        <Button variant="ghost" onClick={onDone}>
          {t("dict.cancel")}
        </Button>
      </div>
    </Card>
  );
}

function EntryRow({ entry, onRemove }: { entry: DictEntry; onRemove: () => void }) {
  const [hover, setHover] = useState(false);
  return (
    <div
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 12,
        padding: "12px 6px",
        borderBottom: `1px solid ${theme.border}`,
      }}
    >
      <div style={{ minWidth: 0 }}>
        <span style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>{entry.correct}</span>
        {entry.auto && (
          <span title={t("dict.autoLearned")} style={{ marginLeft: 6, color: theme.accentDeep }}>
            <Icon name="transforms" size={12} />
          </span>
        )}
        {entry.mishears.length > 0 && (
          <span style={{ marginLeft: 10, fontSize: 12.5, color: theme.textMuted }}>
            {t("dict.heardAs", { list: entry.mishears.join(", ") })}
          </span>
        )}
      </div>
      <button
        onClick={onRemove}
        title={t("dict.remove")}
        style={{
          border: "none",
          background: "transparent",
          cursor: "pointer",
          color: theme.textFaint,
          opacity: hover ? 1 : 0,
          transition: "opacity 120ms ease",
          display: "flex",
          alignItems: "center",
          padding: 4,
        }}
      >
        <Icon name="close" size={16} />
      </button>
    </div>
  );
}

function ImportContactsButton({ onDone }: { onDone: () => void }) {
  const [msg, setMsg] = useState<string | null>(null);
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
      <Button
        variant="ghost"
        size="sm"
        onClick={() => {
          setMsg(t("dict.importing"));
          void import("./api").then(({ importContacts }) =>
            importContacts().then((r) => {
              if ("added" in r) {
                setMsg(r.added > 0 ? t("dict.imported", { n: r.added }) : t("dict.noNew"));
                onDone();
              } else {
                setMsg(t("dict.failed", { e: r.error }));
              }
            })
          );
        }}
      >
        {t("dict.import")}
      </Button>
      {msg && <span style={{ fontSize: 12.5, color: theme.textMuted }}>{msg}</span>}
    </div>
  );
}

export function DictionaryPane() {
  const [entries, setEntries] = useState<DictEntry[]>([]);
  const [tab, setTab] = useState<Tab>("all");
  const [query, setQuery] = useState("");
  const [adding, setAdding] = useState(false);

  const load = () => getDictionary().then(setEntries);
  useEffect(() => {
    void load();
  }, []);

  const remove = async (correct: string) => {
    await removeDictionaryEntry(correct);
    await load();
  };

  const q = query.trim().toLowerCase();
  const filtered = q ? entries.filter((e) => e.correct.toLowerCase().includes(q)) : entries;

  return (
    <div style={{ maxWidth: 760 }}>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 18 }}>
        <div>
          <h1
            style={{
              fontFamily: font.serif,
              fontSize: 30,
              fontWeight: 600,
              letterSpacing: -0.4,
              margin: 0,
              color: theme.textStrong,
            }}
          >
            {t("dict.title")}
          </h1>
          <p style={{ color: theme.textMuted, fontSize: 14, margin: "8px 0 0" }}>
            {t("dict.sub")}
          </p>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <ImportContactsButton onDone={load} />
          <Button variant="accent" onClick={() => setAdding((a) => !a)}>
            <Icon name="plus" size={15} style={{ color: "#fff" }} />
            {t("dict.addNew")}
          </Button>
        </div>
      </div>

      <Tabs tab={tab} onChange={setTab} />

      {/* Search + sort */}
      <div style={{ display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 10, marginBottom: 14 }}>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 7,
            background: theme.cardBg,
            border: `1px solid ${theme.border}`,
            borderRadius: 9,
            padding: "6px 10px",
            minWidth: 200,
          }}
        >
          <Icon name="search" size={15} style={{ color: theme.textFaint }} />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("dict.search")}
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
        <button
          title="Sort"
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: 34,
            height: 34,
            border: `1px solid ${theme.border}`,
            borderRadius: 9,
            background: theme.cardBg,
            color: theme.textMuted,
            cursor: "pointer",
          }}
        >
          <Icon name="sort" size={16} />
        </button>
      </div>

      {adding && (
        <AddForm
          onDone={() => {
            setAdding(false);
            void load();
          }}
        />
      )}

      {tab === "shared" ? (
        <Card>
          <div style={{ padding: "36px 8px", textAlign: "center", color: theme.textFaint, fontSize: 13.5 }}>
            {t("dict.teamSoon")}
          </div>
        </Card>
      ) : (
        <Card pad={filtered.length ? 8 : 22}>
          {filtered.length === 0 ? (
            <div style={{ padding: "30px 8px", textAlign: "center", color: theme.textFaint, fontSize: 13.5 }}>
              {entries.length === 0
                ? t("dict.empty")
                : t("dict.noMatch", { q: query })}
            </div>
          ) : (
            <div style={{ padding: "4px 14px" }}>
              {filtered.map((e) => (
                <EntryRow key={e.correct} entry={e} onRemove={() => void remove(e.correct)} />
              ))}
            </div>
          )}
        </Card>
      )}
    </div>
  );
}
