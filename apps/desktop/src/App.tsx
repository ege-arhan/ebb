import { Bot, Command, Gauge, History, Layers, Plug, ScanSearch, Settings as Cog, ShieldCheck, Sparkles, Trash2, Undo2, Waves } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useState } from "react";
import { Celebrate } from "./components/Celebrate";
import { Modal, Palette, Toasts, type Cmd } from "./components/Overlay";
import { api, inTauri } from "./ipc";
import { human } from "./format";
import { Agents } from "./screens/Agents";
import { Clean } from "./screens/Clean";
import { Connect } from "./screens/Connect";
import { Explore } from "./screens/Explore";
import { Journal } from "./screens/Journal";
import { Optimize } from "./screens/Optimize";
import { Speed } from "./screens/Speed";
import { Settings } from "./screens/Settings";
import { Tide } from "./screens/Tide";
import { Trash } from "./screens/Trash";
import { useApp } from "./state";

export type Screen = "tide" | "explore" | "clean" | "optimize" | "speed" | "agents" | "connect" | "journal" | "trash" | "settings";
const NAV = [
  { id: "tide", icon: Waves }, { id: "explore", icon: ScanSearch }, { id: "clean", icon: Sparkles }, { id: "optimize", icon: Layers },
  { id: "speed", icon: Gauge }, { id: "agents", icon: Bot },
  { id: "connect", icon: Plug }, { id: "journal", icon: History }, { id: "trash", icon: Trash2 }, { id: "settings", icon: Cog },
] as const;

const isMac = navigator.platform.toLowerCase().includes("mac");
const seen = () => { try { return location.search.includes("nowelcome") || localStorage.getItem("ebb.welcome") === "1"; } catch { return true; } };
const fromHash = (): Screen => { const h = location.hash.slice(1); return (NAV.some((n) => n.id === h) ? h : "tide") as Screen; };
const markSeen = () => { try { localStorage.setItem("ebb.welcome", "1"); } catch { /* yok say */ } };

export default function App() {
  const { t, setTheme, theme, disk, clients, update, trashSize } = useApp();
  const [screen, setScreen] = useState<Screen>(fromHash);
  const [palette, setPalette] = useState(false);
  const [startScan, setStartScan] = useState(() => location.search.includes("autoscan"));
  const [welcome, setWelcome] = useState(() => !seen());

  const closeWelcome = (to?: "scan" | "connect") => {
    markSeen(); setWelcome(false);
    if (to === "scan") { setScreen("explore"); setStartScan(true); }
    if (to === "connect") setScreen("connect");
  };

  // Menü çubuğu / tepsi menüsünden gelen yönlendirmeler.
  useEffect(() => {
    if (!inTauri) return;
    let un: (() => void) | undefined;
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen<string>("ebb://nav", (e) => {
        if (e.payload === "clean") setScreen("clean");
        if (e.payload === "connect") setScreen("connect");
        if (e.payload === "scan") { setScreen("explore"); setStartScan(true); }
      }).then((u) => { un = u; }),
    );
    return () => un?.();
  }, []);

  // İmleci izleyen ışık: kartların üstünde.
  const glow = (e: React.PointerEvent) => {
    const el = (e.target as HTMLElement).closest<HTMLElement>(".panel, .stat, .client, .steps li, .flow li, .rows.cards li, .stores li, .nudge");
    if (!el) return;
    const r = el.getBoundingClientRect();
    el.style.setProperty("--mx", `${e.clientX - r.left}px`);
    el.style.setProperty("--my", `${e.clientY - r.top}px`);
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      if (e.key.toLowerCase() === "k") { e.preventDefault(); setPalette((p) => !p); return; }
      const n = Number(e.key);
      if (n >= 1 && n <= NAV.length) { e.preventDefault(); setScreen(NAV[n - 1].id); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const cmds: Cmd[] = useMemo(() => [
    ...NAV.map((n, i) => ({ id: n.id, label: t(n.id), hint: `${isMac ? "⌘" : "Ctrl+"}${i + 1}`, run: () => setScreen(n.id) })),
    { id: "scan", label: t("scanHome"), run: () => { setScreen("explore"); setStartScan(true); } },
    { id: "review", label: t("reviewClean"), run: () => setScreen("clean") },
    { id: "theme", label: `${t("theme")}: ${theme === "dark" ? t("light") : t("dark")}`, run: () => setTheme(theme === "dark" ? "light" : "dark") },
  ], [t, theme, setTheme]);

  const usedFrac = disk ? 1 - disk.free / disk.total : 0;
  const pending = clients.filter((c) => c.installed && !c.connected).length;

  return (
    <div className="shell">
      <aside className="rail">
        <div className="drag" data-tauri-drag-region />
        <div className="brand"><svg viewBox="0 0 32 32" width="22" height="22" aria-hidden><path d="M3 18c3-4 6-4 9 0s6 4 9 0 4-2 8-2v12H3z" fill="currentColor" /></svg><span>Ebb</span></div>
        <nav aria-label="Main">
          {NAV.map(({ id, icon: Icon }) => (
            <button key={id} className={screen === id ? "on" : ""} aria-current={screen === id ? "page" : undefined} onClick={() => setScreen(id)}>
              {screen === id && <motion.span layoutId="navpill" className="navpill" transition={{ type: "spring", stiffness: 520, damping: 40 }} />}
              <Icon size={17} aria-hidden /> <span className="nav-label">{t(id)}</span>
              {id === "connect" && pending > 0 && <span className="dot" aria-label={`${pending}`}>{pending}</span>}
              {id === "trash" && trashSize > 0 && <span className="dot soft">{human(trashSize, 0)}</span>}
            </button>
          ))}
        </nav>
        {update && (
          <button className="update-pill" onClick={() => api.openRelease(update.url)}>
            <Sparkles size={14} aria-hidden /> <span>{t("updateAvail").replace("{v}", update.version)}</span> <b>{t("updateGet")}</b>
          </button>
        )}
        {disk && (
          <div className="mini-disk" title={`${human(disk.free)} ${t("free")} ${t("of")} ${human(disk.total)}`}>
            <span><b>{human(disk.free, 0)}</b> {t("free")}</span>
            <i><u style={{ width: `${usedFrac * 100}%` }} className={usedFrac > 0.9 ? "hot" : usedFrac > 0.7 ? "warm" : ""} /></i>
          </div>
        )}
        <button className="kbar" onClick={() => setPalette(true)}><Command size={14} aria-hidden /> <span>{t("palettePlaceholder")}</span> <kbd>{isMac ? "⌘K" : "Ctrl K"}</kbd></button>
      </aside>
      <main onPointerMove={glow}>
        <div className="drag top" data-tauri-drag-region />
        <Explore hidden={screen !== "explore"} startScan={startScan} clearStart={() => setStartScan(false)} />
        <AnimatePresence mode="wait">
          {screen !== "explore" && (
            <motion.div key={screen} initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} transition={{ duration: 0.18 }}>
              {screen === "tide" && <Tide go={setScreen} />}
              {screen === "clean" && <Clean />}
              {screen === "optimize" && <Optimize />}
              {screen === "speed" && <Speed go={setScreen} />}
              {screen === "agents" && <Agents go={setScreen} />}
              {screen === "connect" && <Connect />}
              {screen === "journal" && <Journal />}
              {screen === "trash" && <Trash />}
              {screen === "settings" && <Settings />}
            </motion.div>
          )}
        </AnimatePresence>
      </main>
      <Palette open={palette} onClose={() => setPalette(false)} cmds={cmds} />
      <Modal open={welcome} onClose={() => closeWelcome()} title={t("welcome")}>
        <div className="welcome">
          <svg viewBox="0 0 32 32" width="38" height="38" aria-hidden className="welcome-logo"><path d="M3 18c3-4 6-4 9 0s6 4 9 0 4-2 8-2v12H3z" fill="currentColor" /></svg>
          <h2>{t("welcome")}</h2>
          <p className="muted">{t("welcomeBody")}</p>
          <ul>
            {[[ShieldCheck, t("w1"), t("w1b")], [Undo2, t("w2"), t("w2b")], [Plug, t("w3"), t("w3b")]].map(([I, a, b], i) => {
              const Ico = I as typeof Plug;
              return <li key={i}><span className="flow-ico"><Ico size={18} aria-hidden /></span><span className="rowtext"><b>{a as string}</b><small>{b as string}</small></span></li>;
            })}
          </ul>
          <footer>
            <button className="btn ghost" onClick={() => closeWelcome()}>{t("skip")}</button>
            <button className="btn" onClick={() => closeWelcome("connect")}>{t("later")}</button>
            <button className="btn primary" onClick={() => closeWelcome("scan")}>{t("letsGo")}</button>
          </footer>
        </div>
      </Modal>
      <Celebrate />
      <Toasts />
    </div>
  );
}
