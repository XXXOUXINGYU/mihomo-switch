import React from "react";
import ReactDOM from "react-dom/client";

import { App } from "./ui/App";
import { applyTheme, readStoredTheme } from "./ui/theme/theme";
import "./ui/global.css";
import "./ui/theme/controls.css";

applyTheme(readStoredTheme());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
