import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import pkg from "../package.json";
import { api, inTauri } from "./ipc";
import { dict, type Key, type Lang } from "./i18n";
import type { AiStore, ClientInfo, DiskUsage, Finding, PlanSummary, Report } from "./types";

type Theme = "system" | "light" | "dark";
interface Toast { id: number; text: string; action?: { label: string; run: () => void } }

interface Ctx {
  lang: Lang; setLang(l: Lang): void;
  theme: Theme; setTheme(t: Theme): void;
  t(k: Key): string;
  home: string;
  disk: DiskUsage | null;
  diskAt: number;
  celebrate: number;
  celebrateKind: "trash" | "optimize" | "compress" | "freed";
  celebrateWith(bytes: number, kind: "trash" | "optimize" | "compress" | "freed"): void;
  clearCelebrate(): void;
  findings: Finding[] | null;
  ai: AiStore[] | null;
  journal: PlanSummary[];
  clients: ClientInfo[];
  refreshClients(): Promise<void>;
  reclaimable: number;
  refresh(): Promise<void>;
  apply(planId: string, label: string): Promise<Report>;
  undo(planId: string): Promise<void>;
  trashSize: number;
  update: { version: string; url: string } | null;
  checkUpdates: boolean;
  setCheckUpdates(v: boolean): void;
  toasts: Toast[];
  toast(text: string, action?: Toast["action"]): void;
}

const C = createContext<Ctx>(null!);

/** a > b (basit semver: sayısal major.minor.patch). */
function newer(a: string, b: string) {
  const pa = a.split(".").map(Number), pb = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) if ((pa[i] ?? 0) !== (pb[i] ?? 0)) return (pa[i] ?? 0) > (pb[i] ?? 0);
  return false;
}
export const useApp = () => useContext(C);

const read = (k: string) => { try { return localStorage.getItem(k); } catch { return null; } };
const write = (k: string, v: string) => { try { localStorage.setItem(k, v); } catch { /* özel pencere vb. */ } };

export function AppProvider({ children, light = false }: { children: ReactNode; light?: boolean }) {
  const [lang, setLangS] = useState<Lang>(() => (new URLSearchParams(location.search).get("lang") as Lang) || (read("ebb.lang") as Lang) || (((["tr", "de", "es", "fr"] as const).find((l) => navigator.language.startsWith(l))) ?? "en"));
  const [theme, setThemeS] = useState<Theme>(() => (read("ebb.theme") as Theme) || "system");
  const [home, setHome] = useState("");
  const [disk, setDisk] = useState<DiskUsage | null>(null);
  const [diskAt, setDiskAt] = useState(Date.now());
  const [celebrate, setCelebrate] = useState(0);
  const [celebrateKind, setCelebrateKind] = useState<"trash" | "optimize" | "compress" | "freed">("trash");
  const [findings, setFindings] = useState<Finding[] | null>(null);
  const [ai, setAi] = useState<AiStore[] | null>(null);
  const [journal, setJournal] = useState<PlanSummary[]>([]);
  const [clients, setClients] = useState<ClientInfo[]>([]);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [trashSize, setTrashSize] = useState(0);
  const [update, setUpdate] = useState<{ version: string; url: string } | null>(null);
  const [checkUpdates, setCheckS] = useState(() => read("ebb.updates") !== "off");
  const setCheckUpdates = (v: boolean) => { setCheckS(v); write("ebb.updates", v ? "on" : "off"); };

  // Günde bir kez GitHub'a yeni sürüm var mı diye sorar (kapatılabilir). Başka hiçbir şey gönderilmez.
  useEffect(() => {
    if (!checkUpdates) { setUpdate(null); return; }
    const last = Number(read("ebb.updates.at") ?? 0);
    const cached = read("ebb.updates.latest");
    const apply = (tag: string, url: string) => { if (newer(tag.replace(/^v/, ""), pkg.version)) setUpdate({ version: tag.replace(/^v/, ""), url }); };
    if (Date.now() - last < 86_400_000 && cached) { const [tag, url] = cached.split("|"); apply(tag, url); return; }
    fetch("https://api.github.com/repos/ege-arhan/ebb/releases/latest", { headers: { Accept: "application/vnd.github+json" } })
      .then((r) => (r.ok ? r.json() : null))
      .then((j) => { if (j?.tag_name) { write("ebb.updates.at", String(Date.now())); write("ebb.updates.latest", `${j.tag_name}|${j.html_url}`); apply(j.tag_name, j.html_url); } })
      .catch(() => {});
  }, [checkUpdates]);
  const nextId = useRef(1);

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = theme === "system" ? "" : theme;
    root.lang = lang;
  }, [theme, lang]);

  const setLang = (l: Lang) => { setLangS(l); write("ebb.lang", l); };
  const setTheme = (t: Theme) => { setThemeS(t); write("ebb.theme", t); };
  const t = useCallback((k: Key) => (dict[lang][k] ?? dict.en[k]) as string, [lang]);

  const toast = useCallback((text: string, action?: Toast["action"]) => {
    const id = nextId.current++;
    setToasts((x) => [...x, { id, text, action }]);
    setTimeout(() => setToasts((x) => x.filter((y) => y.id !== id)), 9000);
  }, []);

  const refreshClients = useCallback(async () => {
    if (light) return;
    setClients(await api.connectClients().catch(() => []));
  }, []);

  const refresh = useCallback(async () => {
    void refreshClients();
    api.trashList().then((l) => setTrashSize(l.reduce((a, x) => a + x.size, 0))).catch(() => {});
    api.diskUsage().then((d) => { setDisk(d); setDiskAt(Date.now()); }).catch(() => {});
    api.journalList().then(setJournal).catch(() => {});
    api.findings(lang === "tr").then(setFindings).catch(() => setFindings([]));
    api.aiStorage().then(setAi).catch(() => setAi([]));
  }, [lang, refreshClients]);

  useEffect(() => { if (light) return; api.homeDir().then(setHome); void refresh(); }, [refresh, light]);

  // Otomatik bakım arka planda bir plan hazırlayınca haber ver.
  useEffect(() => {
    if (!inTauri) return;
    let un: (() => void) | undefined;
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen<{ message: string }>("ebb://care", (e) => toast(`${((dict[lang].careReady ?? dict.en.careReady) as string)}: ${e.payload.message}`)).then((u) => { un = u; }),
    );
    return () => un?.();
  }, [lang, toast]);

  // Canlı disk: 20 sn'de bir okunur; değişince havza kendiliğinden hareket eder.
  useEffect(() => {
    const id = setInterval(() => { if (!document.hidden) api.diskUsage().then((d) => { setDisk(d); setDiskAt(Date.now()); }).catch(() => {}); }, 20000);
    return () => clearInterval(id);
  }, []);

  const undo = useCallback(async (planId: string) => {
    const r = await api.undoPlan(planId);
    toast(`${((dict[lang].restoredToast ?? dict.en.restoredToast) as string)} · ${r.items.length - r.failed}`);
    await refresh();
  }, [lang, refresh, toast]);

  const apply = useCallback(async (planId: string, label: string) => {
    const r = await api.applyPlan(planId);
    const ok = r.items.length - r.failed;
    toast(`${((dict[lang].movedToast ?? dict.en.movedToast) as string)} · ${ok}${r.failed ? ` (${r.failed} ✕)` : ""}`, { label: ((dict[lang].undo ?? dict.en.undo) as string), run: () => void undo(planId) });
    void label;
    if (r.bytes > 0) { setCelebrateKind("trash"); setCelebrate(r.bytes); }
    await refresh();
    return r;
  }, [lang, refresh, toast, undo]);

  const reclaimable = useMemo(() => (findings ?? []).filter((f) => f.safety === "safe").reduce((a, f) => a + f.size, 0), [findings]);

  const value: Ctx = { lang, setLang, theme, setTheme, t, home, disk, diskAt, celebrate, celebrateKind, celebrateWith: (b: number, k: "trash" | "optimize" | "compress" | "freed") => { setCelebrateKind(k); setCelebrate(b); }, clearCelebrate: () => setCelebrate(0), findings, ai, journal, clients, refreshClients, reclaimable, refresh, apply, undo, toasts, toast, trashSize, update, checkUpdates, setCheckUpdates };
  return <C.Provider value={value}>{children}</C.Provider>;
}
