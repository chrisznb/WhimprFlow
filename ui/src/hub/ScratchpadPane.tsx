import { t } from "../i18n";
import { useEffect, useRef, useState } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Card, PageTitle } from "./ui";
import { getScratchpad, setScratchpad } from "./api";

export function ScratchpadPane() {
  const [text, setText] = useState("");
  const [saved, setSaved] = useState(true);
  const timer = useRef<number | null>(null);

  useEffect(() => {
    void getScratchpad().then(setText);
    return () => {
      if (timer.current) window.clearTimeout(timer.current);
    };
  }, []);

  const onChange = (v: string) => {
    setText(v);
    setSaved(false);
    if (timer.current) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      void setScratchpad(v).then(() => setSaved(true));
    }, 600);
  };

  return (
    <div style={{ maxWidth: 900, display: "flex", flexDirection: "column", height: "100%" }}>
      <PageTitle sub={t("sp.sub")}>
        {t("sp.title")}
      </PageTitle>
      <Card style={{ flex: 1, display: "flex", minHeight: 480, padding: 0 }} pad={0}>
        <textarea
          value={text}
          onChange={(e) => onChange(e.target.value)}
          placeholder={t("sp.ph")}
          style={{
            flex: 1,
            width: "100%",
            minHeight: 480,
            border: "none",
            outline: "none",
            resize: "none",
            background: "transparent",
            padding: "22px 24px",
            fontFamily: font.serif,
            fontSize: 16.5,
            lineHeight: 1.65,
            color: theme.textBody,
          }}
        />
      </Card>
      <div style={{ marginTop: 8, fontSize: 12, color: theme.textFaint, textAlign: "right" }}>
        {saved ? t("sp.saved") : t("sp.saving")}
      </div>
    </div>
  );
}
