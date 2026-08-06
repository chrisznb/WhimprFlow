import { useEffect, useState } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Button, Card, PageTitle, Skeleton, WaveLoader } from "./ui";
import { Icon } from "./icons";
import {
  chooseAudioFile,
  getFileTranscripts,
  getScratchpad,
  setScratchpad,
  transcribeFile,
  type FileTranscript,
} from "./api";

export function TranscribePane() {
  const [busy, setBusy] = useState<string | null>(null);
  const [result, setResult] = useState<string>("");
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [past, setPast] = useState<FileTranscript[]>([]);

  const loadPast = () => void getFileTranscripts().then(setPast);
  useEffect(loadPast, []);

  const run = async (path: string) => {
    setBusy(path.split("/").pop() ?? path);
    setError(null);
    setResult("");
    const res = await transcribeFile(path);
    if ("text" in res) {
      setResult(res.text);
      loadPast();
    } else {
      setError(res.error);
    }
    setBusy(null);
  };

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void (async () => {
      try {
        const { getCurrentWebview } = await import("@tauri-apps/api/webview");
        unlisten = await getCurrentWebview().onDragDropEvent((event) => {
          if (event.payload.type === "drop" && event.payload.paths.length > 0) {
            void run(event.payload.paths[0]);
          }
        });
      } catch {
        /* browser dev */
      }
    })();
    return () => unlisten?.();
  }, []);

  const copy = () => {
    void navigator.clipboard.writeText(result).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };

  const toScratchpad = async () => {
    const cur = await getScratchpad();
    const sep = cur && !cur.endsWith("\n") ? "\n\n" : cur ? "\n" : "";
    await setScratchpad(cur + sep + result + "\n");
  };

  return (
    <div style={{ maxWidth: 880 }}>
      <PageTitle sub="Drop a voice memo, recording, or any audio file. WhimprFlow transcribes it with your configured engine.">
        Transcribe a file
      </PageTitle>

      <Card
        style={{
          border: `1.5px dashed ${theme.borderStrong}`,
          background: theme.cardBgSubtle,
          textAlign: "center",
          padding: 34,
          marginBottom: 16,
        }}
      >
        {busy ? (
          <div style={{ display: "flex", flexDirection: "column", gap: 20, alignItems: "center" }}>
            <WaveLoader label={`Transcribing ${busy}`} />
            <div style={{ width: "100%", maxWidth: 560, display: "flex", flexDirection: "column", gap: 10 }}>
              <Skeleton h={12} />
              <Skeleton h={12} w="86%" />
              <Skeleton h={12} w="72%" />
            </div>
          </div>
        ) : (
          <>
            <div style={{ marginBottom: 10 }}>
              <Icon name="mic" size={26} style={{ color: theme.textFaint }} />
            </div>
            <div style={{ fontSize: 14.5, fontWeight: 600, color: theme.textStrong }}>
              Drop an audio file anywhere in this window
            </div>
            <div style={{ fontSize: 12.5, color: theme.textMuted, margin: "6px 0 14px" }}>
              m4a, mp3, wav, aiff, caf. Voice memos from your iPhone work great.
            </div>
            <Button
              onClick={() =>
                void chooseAudioFile().then((p) => {
                  if (p) void run(p);
                })
              }
            >
              Choose file
            </Button>
          </>
        )}
      </Card>

      {error && (
        <Card style={{ marginBottom: 16, borderColor: "#E5484D" }}>
          <div style={{ color: "#C0392B", fontSize: 13.5 }}>{error}</div>
        </Card>
      )}

      {result && (
        <Card>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              marginBottom: 12,
            }}
          >
            <div style={{ fontSize: 14, fontWeight: 650, color: theme.textStrong }}>Transcript</div>
            <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
              {copied && <span style={{ fontSize: 12.5, color: theme.accent }}>Copied</span>}
              <Button size="sm" variant="ghost" onClick={copy}>
                Copy
              </Button>
              <Button size="sm" variant="ghost" onClick={() => void toScratchpad()}>
                Add to Scratchpad
              </Button>
            </div>
          </div>
          <div
            style={{
              fontFamily: font.serif,
              fontSize: 15.5,
              lineHeight: 1.65,
              color: theme.textBody,
              whiteSpace: "pre-wrap",
              overflowWrap: "anywhere",
              maxHeight: 420,
              overflowY: "auto",
            }}
          >
            {result}
          </div>
        </Card>
      )}

      {past.length > 0 && (
        <Card pad={0} style={{ marginTop: 16 }}>
          <div
            style={{
              padding: "14px 18px",
              borderBottom: `1px solid ${theme.border}`,
              fontSize: 11.5,
              fontWeight: 700,
              letterSpacing: 0.7,
              textTransform: "uppercase",
              color: theme.textFaint,
            }}
          >
            Previous transcriptions
          </div>
          {past.map((t, i) => (
            <div
              key={`${t.ts_unix}-${i}`}
              className="wf-row"
              onClick={() => {
                setResult(t.text);
                setError(null);
              }}
              style={{
                display: "flex",
                gap: 14,
                alignItems: "flex-start",
                padding: "12px 18px",
                borderBottom: i < past.length - 1 ? `1px solid ${theme.border}` : "none",
                cursor: "pointer",
              }}
            >
              <div style={{ flex: "0 0 auto", paddingTop: 2 }}>
                <Icon name="fileaudio" size={15} style={{ color: theme.textFaint }} />
              </div>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 13, fontWeight: 600, color: theme.textStrong }}>
                  {t.filename}
                  <span style={{ fontWeight: 400, color: theme.textFaint, marginLeft: 8, fontSize: 11.5 }}>
                    {new Date(t.ts_unix * 1000).toLocaleString()}
                  </span>
                </div>
                <div
                  style={{
                    fontSize: 12.5,
                    color: theme.textMuted,
                    marginTop: 2,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {t.text}
                </div>
              </div>
              <div className="wf-row-actions" style={{ flex: "0 0 auto" }}>
                <button
                  title="Copy"
                  onClick={(e) => {
                    e.stopPropagation();
                    void navigator.clipboard.writeText(t.text);
                  }}
                  className="wf-press"
                  style={{
                    border: "none",
                    background: "transparent",
                    cursor: "pointer",
                    color: theme.textFaint,
                    padding: "2px 4px",
                  }}
                >
                  <Icon name="copy" size={14} />
                </button>
              </div>
            </div>
          ))}
        </Card>
      )}
    </div>
  );
}
