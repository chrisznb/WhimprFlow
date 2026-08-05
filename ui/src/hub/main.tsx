import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/figtree/400.css";
import "@fontsource/figtree/500.css";
import "@fontsource/figtree/600.css";
import "@fontsource/figtree/700.css";
import "@fontsource/eb-garamond/400.css";
import "@fontsource/eb-garamond/500.css";
import "@fontsource/eb-garamond/600.css";
import { App } from "./App";

const style = document.createElement("style");
style.textContent = `
  html, body, #root { margin: 0; height: 100%; }
  * { box-sizing: border-box; }

  @keyframes wf-fade-up {
    from { opacity: 0; transform: translateY(6px); }
    to { opacity: 1; transform: translateY(0); }
  }
  @keyframes wf-pop {
    0% { transform: scale(0.6); opacity: 0; }
    70% { transform: scale(1.08); opacity: 1; }
    100% { transform: scale(1); }
  }
  .wf-fade { animation: wf-fade-up 320ms cubic-bezier(0.2, 0.7, 0.3, 1) both; }
  .wf-pop { animation: wf-pop 260ms cubic-bezier(0.34, 1.4, 0.5, 1) both; }

  .wf-press { transition: transform 110ms ease, filter 140ms ease, opacity 140ms ease; }
  .wf-press:hover { filter: brightness(1.06); }
  .wf-press:active { transform: scale(0.965); }

  .wf-nav:hover:not(:disabled) { background: rgba(26,26,26,0.05) !important; }

  .wf-card { transition: box-shadow 180ms ease, transform 180ms ease, border-color 180ms ease; }
  .wf-card-hover:hover {
    transform: translateY(-1px);
    box-shadow: 0 2px 4px rgba(26,26,26,0.05), 0 10px 28px rgba(26,26,26,0.08);
  }

  .wf-row { transition: background 120ms ease; }
  .wf-row:hover { background: #FAF9F7; }
`;
document.head.appendChild(style);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
