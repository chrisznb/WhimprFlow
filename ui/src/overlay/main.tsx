import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/figtree/400.css";
import "@fontsource/figtree/500.css";
import { FlowBar } from "./FlowBar";

// The overlay window is transparent; keep the document background clear so only
// the pill paints. (Global reset lives here rather than a CSS file to keep the
// always-resident overlay bundle minimal.)
const style = document.createElement("style");
style.textContent = `
  html, body, #root { margin: 0; height: 100%; background: transparent; }
  * { box-sizing: border-box; }

  @keyframes wf-in {
    from { opacity: 0; transform: translateY(3px) scale(0.96); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }
  @keyframes wf-check-pop {
    0% { transform: scale(0.4); opacity: 0; }
    65% { transform: scale(1.18); opacity: 1; }
    100% { transform: scale(1); }
  }
  @keyframes wf-dot-pulse {
    0%, 80%, 100% { opacity: 0.35; transform: scale(0.85); }
    40% { opacity: 1; transform: scale(1.15); }
  }
  @keyframes wf-btn-in {
    from { transform: scale(0.5); opacity: 0; }
    to { transform: scale(1); opacity: 1; }
  }
  .wf-in { animation: wf-in 220ms cubic-bezier(0.2, 0.7, 0.3, 1) both; }
  .wf-round { transition: transform 120ms ease, filter 120ms ease; }
  .wf-round:hover { transform: scale(1.12); filter: brightness(1.15); }
  .wf-round:active { transform: scale(0.94); }
  .wf-check { animation: wf-check-pop 320ms cubic-bezier(0.34, 1.4, 0.5, 1) both; }
  .wf-dot { display: inline-block; animation: wf-dot-pulse 1.1s ease-in-out infinite; }
  .wf-btn-pop { animation: wf-btn-in 260ms cubic-bezier(0.34, 1.4, 0.5, 1) both; }
`;
document.head.appendChild(style);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <FlowBar />
  </React.StrictMode>,
);
