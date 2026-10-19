import { AppWindow, Bot, Menu as MenuIcon, PanelLeftClose, PanelLeftOpen, X, CalendarClock, Command, Gauge, Hammer, History, Layers, MessageCircle, Plug, ScanSearch, Settings as Cog, ShieldCheck, Sparkles, Trash2, Undo2, Waves } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { Fragment, Suspense, lazy, useEffect, useMemo, useState } from "react";
import type { Key } from "./i18n";
import { Celebrate } from "./components/Celebrate";
import { Modal, Palette, Toasts, type Cmd } from "./components/Overlay";
import { api, inTauri } from "./ipc";
import { human } from "./format";
import { Explore } from "./screens/Explore";
import { Tide } from "./screens/Tide";
import { useApp } from "./state";

// Ekranlar ihtiyaç olunca yüklenir: ilk açılış hafif kalır.
const Agents = lazy(() => import("./screens/Agents").then((m) => ({ default: m.Agents })));
const Apps = lazy(() => import("./screens/Apps").then((m) => ({ default: m.Apps })));
const Assistant = lazy(() => import("./screens/Assistant").then((m) => ({ default: m.Assistant })));
const Care = lazy(() => import("./screens/Care").then((m) => ({ default: m.Care })));
const Dev = lazy(() => import("./screens/Dev").then((m) => ({ default: m.Dev })));
const Clean = lazy(() => import("./screens/Clean").then((m) => ({ default: m.Clean })));
const Connect = lazy(() => import("./screens/Connect").then((m) => ({ default: m.Connect })));
const Journal = lazy(() => import("./screens/Journal").then((m) => ({ default: m.Journal })));
const Optimize = lazy(() => import("./screens/Optimize").then((m) => ({ default: m.Optimize })));
const Settings = lazy(() => import("./screens/Settings").then((m) => ({ default: m.Settings })));
const Speed = lazy(() => import("./screens/Speed").then((m) => ({ default: m.Speed })));
const Trash = lazy(() => import("./screens/Trash").then((m) => ({ default: m.Trash })));

export type Screen = "tide" | "explore" | "clean" | "optimize" | "speed" | "apps" | "dev" | "care" | "assistant" | "agents" | "connect" | "journal" | "trash" | "settings";
const NAV = [
  { id: "tide", icon: Waves, group: "navOverview" }, { id: "explore", icon: ScanSearch },
  { id: "clean", icon: Sparkles, group: "navClean" }, { id: "optimize", icon: Layers }, { id: "apps", icon: AppWindow }, { id: "dev", icon: Hammer }, { id: "trash", icon: Trash2 },
  { id: "speed", icon: Gauge, group: "navSystem" }, { id: "care", icon: CalendarClock },
  { id: "agents", icon: Bot, group: "navAi" }, { id: "assistant", icon: MessageCircle }, { id: "connect", icon: Plug },
  { id: "journal", icon: History, group: "" }, { id: "settings", icon: Cog },
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
  // Sol menü: geniş pencerede tam, orta boyda simge şeridi, dar pencerede hamburgerle açılan çekmece.
  const [vw, setVw] = useState(window.innerWidth);
  const [collapsed, setCollapsed] = useState(() => { try { return localStorage.getItem("ebb.rail") === "mini"; } catch { return false; } });
  const [drawer, setDrawer] = useState(false);
  useEffect(() => { const f = () => setVw(window.innerWidth); window.addEventListener("resize", f); return () => window.removeEventListener("resize", f); }, []);
  const mode = vw < 760 ? "drawer" : vw < 1060 || collapsed ? "mini" : "full";
  const toggleRail = () => setCollapsed((c) => { try { localStorage.setItem("ebb.rail", c ? "full" : "mini"); } catch { /* yok say */ } return !c; });
  useEffect(() => { if (mode !== "drawer") setDrawer(false); }, [mode]);

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
        if (e.payload === "scan") { setScreen("explore"); setStartScan(true); }
        else if (NAV.some((n) => n.id === e.payload)) setScreen(e.payload as Screen);
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
      if (n >= 1 && n <= 9) { e.preventDefault(); setScreen(NAV[n - 1].id); }
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
    <div className="shell" data-rail={mode}>
      {mode === "drawer" && !drawer && <button className={`menu-btn${isMac ? " mac" : ""}`} aria-label={t("menuToggle")} onClick={() => setDrawer(true)}><MenuIcon size={18} aria-hidden /></button>}
      {mode === "drawer" && drawer && <div className="rail-backdrop" onClick={() => setDrawer(false)} />}
      <aside className={`rail${drawer ? " open" : ""}`}>
        <div className="drag" data-tauri-drag-region />
        {mode === "drawer" ? <button className="rail-toggle" style={{ display: "grid" }} aria-label={t("menuClose")} onClick={() => setDrawer(false)}><X size={16} aria-hidden /></button>
          : vw >= 1060 && <button className="rail-toggle" aria-label={t("menuToggle")} onClick={toggleRail}>{collapsed ? <PanelLeftOpen size={16} aria-hidden /> : <PanelLeftClose size={16} aria-hidden />}</button>}
        <div className="brand"><svg viewBox="0 0 32 32" width="22" height="22" aria-hidden><path d="M3 18c3-4 6-4 9 0s6 4 9 0 4-2 8-2v12H3z" fill="currentColor" /></svg><span>Ebb</span></div>
        <nav aria-label="Main">
          {NAV.map(({ id, icon: Icon, ...rest }) => (
            <Fragment key={id}>
              {"group" in rest && rest.group && <p className="navgroup">{t(rest.group as Key)}</p>}
                <button className={screen === id ? "on" : ""} title={t(id)} aria-label={t(id)} aria-current={screen === id ? "page" : undefined} onClick={() => { setScreen(id); setDrawer(false); }}>
              {screen === id && <motion.span layoutId="navpill" className="navpill" transition={{ type: "spring", stiffness: 520, damping: 40 }} />}
              <Icon size={17} aria-hidden /> <span className="nav-label">{t(id)}</span>
              {id === "connect" && pending > 0 && <span className="dot" aria-label={`${pending}`}>{pending}</span>}
              {id === "trash" && trashSize > 0 && <span className="dot soft">{human(trashSize, 0)}</span>}
            </button>
            </Fragment>
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
              <Suspense fallback={null}>
              {screen === "tide" && <Tide go={setScreen} />}
              {screen === "clean" && <Clean />}
              {screen === "optimize" && <Optimize />}
              {screen === "speed" && <Speed go={setScreen} />}
              {screen === "agents" && <Agents go={setScreen} />}
              {screen === "connect" && <Connect />}
              {screen === "apps" && <Apps />}
              {screen === "dev" && <Dev />}
              {screen === "care" && <Care />}
              {screen === "assistant" && <Assistant />}
              {screen === "journal" && <Journal />}
              {screen === "trash" && <Trash />}
              {screen === "settings" && <Settings />}
              </Suspense>
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
