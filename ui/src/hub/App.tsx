import { useEffect, useState } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Onboarding } from "./Onboarding";
import { Sidebar, type Page } from "./Sidebar";
import { Home } from "./Home";
import { Insights } from "./Insights";
import { DictionaryPane } from "./DictionaryPane";
import { SnippetsPane } from "./SnippetsPane";
import { StylePane } from "./StylePane";
import { TransformsPane } from "./TransformsPane";
import { ScratchpadPane } from "./ScratchpadPane";
import { AssistantPane } from "./AssistantPane";
import { SettingsPane } from "./SettingsPane";
import { Help } from "./Help";
import { ComingSoon } from "./ComingSoon";
import type { IconName } from "./icons";
import {
  getSettings,
  setSettings,
  getStatus,
  type Settings,
  type Status,
  DEFAULT_SETTINGS,
} from "./api";

// Placeholder screens that are routed but not yet built.
const SOON: Partial<Record<Page, { icon: IconName; title: string; desc: string }>> = {
};

export function App() {
  const [page, setPage] = useState<Page>("home");
  const [settings, setLocalSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [entered, setEntered] = useState(false);
  const [status, setStatus] = useState<Status>({
    accessibility: false,
    microphone: false,
    input_monitoring: false,
    has_openai_key: false,
    has_anthropic_key: false,
  });

  const refresh = () => getStatus().then(setStatus);

  useEffect(() => {
    getSettings().then(setLocalSettings);
    refresh();
  }, []);

  const update = (s: Settings) => {
    setLocalSettings(s);
    void setSettings(s);
  };

  // Gate the app behind the setup wizard until the required permissions are granted.
  if (!(status.accessibility && status.microphone) && !entered) {
    return <Onboarding status={status} refresh={refresh} onEnter={() => setEntered(true)} />;
  }

  const soon = SOON[page];

  return (
    <div
      style={{
        display: "flex",
        height: "100vh",
        fontFamily: font.ui,
        color: theme.textBody,
        background: theme.pageBg,
      }}
    >
      <Sidebar page={page} setPage={setPage} />
      <main style={{ flex: 1, minWidth: 0, overflowY: "auto" }}>
        <div key={page} className="wf-fade" style={{ padding: "36px 44px", margin: "0 auto", maxWidth: 1120 }}>
          {page === "home" && <Home />}
          {page === "assistant" && <AssistantPane />}
          {page === "insights" && <Insights />}
          {page === "dictionary" && <DictionaryPane />}
          {page === "snippets" && <SnippetsPane />}
          {page === "style" && <StylePane settings={settings} onChange={update} />}
          {page === "transforms" && <TransformsPane />}
          {page === "scratchpad" && <ScratchpadPane />}
          {page === "settings" && (
            <SettingsPane settings={settings} onChange={update} status={status} refresh={refresh} />
          )}
          {page === "help" && <Help />}
          {soon && <ComingSoon icon={soon.icon} title={soon.title} desc={soon.desc} />}
        </div>
      </main>
    </div>
  );
}
