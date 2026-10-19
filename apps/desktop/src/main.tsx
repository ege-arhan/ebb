import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "@fontsource-variable/bricolage-grotesque";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { AppProvider } from "./state";
import "./styles.css";

// Pencere görünmezken süs animasyonlarını durdur (CPU/GPU yükü sıfıra yakın kalsın).
document.addEventListener("visibilitychange", () => document.documentElement.classList.toggle("idle", document.hidden));

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AppProvider>
      <App />
    </AppProvider>
  </React.StrictMode>,
);
