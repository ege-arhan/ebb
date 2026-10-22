import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "@fontsource-variable/bricolage-grotesque";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { Popover } from "./screens/Popover";
import { AppProvider } from "./state";
import "./styles.css";

// Pencere görünmezken süs animasyonlarını durdur (CPU/GPU yükü sıfıra yakın kalsın).
document.addEventListener("visibilitychange", () => document.documentElement.classList.toggle("idle", document.hidden));

const isPopover = location.search.includes("popover");
if (isPopover) document.documentElement.classList.add("popover-mode");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isPopover ? (
      <AppProvider light>
        <Popover />
      </AppProvider>
    ) : (
      <AppProvider>
        <App />
      </AppProvider>
    )}
  </React.StrictMode>,
);
