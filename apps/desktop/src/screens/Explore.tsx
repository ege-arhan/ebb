import { ChevronRight, FolderOpen, FolderPlus, FolderSearch, HardDrive, Home, RotateCw, Square, Usb } from "lucide-react";
import { motion } from "motion/react";
import { useCountUp } from "../components/Bits";
import { useCallback, useEffect, useState } from "react";
import { PlanDialog } from "../components/PlanDialog";
import { Treemap } from "../components/Treemap";
import { api, inTauri } from "../ipc";
import { human, tilde } from "../format";
import { useApp } from "../state";
import type { Key } from "../i18n";
import type { Explain, Location, Node, Plan, Progress } from "../types";

export function Explore({ hidden, startScan, clearStart }: { hidden: boolean; startScan: boolean; clearStart(): void }) {
  const { t, home, toast, refresh, disk } = useApp();
  const [recent, setRecent] = useState<string[]>([]);
  const [path, setPath] = useState("");
  const [locs, setLocs] = useState<Location[]>([]);
  const [last, setLast] = useState<string[]>(() => { try { return JSON.parse(localStorage.getItem("ebb.recent") ?? "[]"); } catch { return []; } });
  const [busy, setBusy] = useState(false);
  const [prog, setProg] = useState<Progress | null>(null);
  const [stack, setStack] = useState<Node[]>([]);
  const [sel, setSel] = useState<Node | null>(null);
  const [info, setInfo] = useState<Explain | null>(null);
  const [plan, setPlan] = useState<Plan | null>(null);

  useEffect(() => { api.locations().then(setLocs).catch(() => {}); }, []);

  const label = (l: Location) => (t(`loc_${l.id}` as Key) as string | undefined) ?? l.label;
  const remember = (p: string) => setLast((l) => {
    const n = [p, ...l.filter((x) => x !== p)].slice(0, 4);
    try { localStorage.setItem("ebb.recent", JSON.stringify(n)); } catch { /* yok say */ }
    return n;
  });

  const run = useCallback(async (p: string | null) => {
    setBusy(true); setProg(null); setSel(null); setInfo(null); setRecent([]);
    if (p) { setPath(p); remember(p); }
    try {
      const root = await api.scan(p, (pr) => { setProg(pr); setRecent((r) => [pr.current, ...r].slice(0, 5)); });
      setStack([root]);
      setPath(root.path);
    } catch (e) {
      const msg = String(e);
      if (!msg.includes("iptal")) toast(`${t("scanError")}: ${msg}`);
    } finally { setBusy(false); }
  }, [t, toast]);

  useEffect(() => { if (startScan) { clearStart(); void run(null); } }, [startScan, clearStart, run]);

  // Bir klasörü pencereye sürükleyip bırakmak taramayı başlatır (yalnızca masaüstü uygulamasında).
  useEffect(() => {
    if (!inTauri) return;
    let un: (() => void) | undefined;
    void import("@tauri-apps/api/webview").then(({ getCurrentWebview }) =>
      getCurrentWebview().onDragDropEvent((e) => {
        if (e.payload.type === "drop" && e.payload.paths[0]) void run(e.payload.paths[0]);
      }).then((u) => { un = u; }),
    );
    return () => un?.();
  }, [run]);

  const pick = async () => { const p = await api.pickFolder(); if (p) void run(p); };

  const cur = stack[stack.length - 1];
  const choose = async (n: Node) => { setSel(n); setInfo(null); if (!n.name.startsWith("(")) setInfo(await api.explain(n.path)); };
  const open = (n: Node) => { setStack((s) => [...s, n]); setSel(null); setInfo(null); };
  const removable = sel && !sel.name.startsWith("(") && !info?.protected;
  const makePlan = async () => sel && setPlan(await api.makePlan([], [], [sel.path]));

  return (
    <div className="page explore" hidden={hidden}>
      {(busy || cur) && (
        <header className="locbar">
          <div className="locchips" role="group" aria-label={t("explore")}>
            {locs.slice(0, 9).map((l) => (
              <button key={l.id} aria-pressed={path === l.path} disabled={busy} onClick={() => run(l.path)}>{label(l)}</button>
            ))}
          </div>
          <button className="btn" disabled={busy} onClick={pick}><FolderPlus size={14} aria-hidden /> {t("chooseFolder")}</button>
          {busy
            ? <button type="button" className="btn" onClick={() => api.cancelScan()}><Square size={14} aria-hidden /> {t("stop")}</button>
            : <button className="btn" onClick={() => run(path)}><RotateCw size={14} aria-hidden /> {t("rescan")}</button>}
        </header>
      )}

      {busy && <ScanLive prog={prog} recent={recent} used={disk ? disk.total - disk.free : 0} />}

      {!busy && !cur && (
        <section className="where">
          <h1>{t("whereTitle")}</h1>
          <p className="lead">{t("whereBody")}</p>
          <ul className="locgrid">
            {locs.map((l, i) => {
              const Icon = l.kind === "home" ? Home : l.kind === "disk" ? HardDrive : l.kind === "volume" ? Usb : FolderSearch;
              return (
                <li key={l.id} style={{ animationDelay: `${i * 40}ms` }}>
                  <button onClick={() => run(l.path)}>
                    <span className="loc-ico"><Icon size={20} aria-hidden /></span>
                    <span className="rowtext"><b>{label(l)}</b><small className="mono" title={l.path}>{tilde(l.path, home)}</small></span>
                  </button>
                </li>
              );
            })}
            <li><button className="choose" onClick={pick}>
              <span className="loc-ico"><FolderPlus size={20} aria-hidden /></span>
              <span className="rowtext"><b>{t("chooseFolder")}</b><small>{t("dropHint")}</small></span>
            </button></li>
          </ul>
          {last.length > 0 && (
            <div className="recents">
              <p className="eyebrow">{t("recentTitle")}</p>
              <p>{last.map((p) => <button key={p} className="chipbtn" onClick={() => run(p)}>{tilde(p, home)}</button>)}</p>
            </div>
          )}
        </section>
      )}

      {!busy && cur && (
        <>
          <nav className="crumbs" aria-label="Breadcrumb">
            {stack.map((n, i) => (
              <span key={n.path + i}>
                {i > 0 && <ChevronRight size={14} aria-hidden />}
                <button disabled={i === stack.length - 1} onClick={() => { setStack(stack.slice(0, i + 1)); setSel(null); setInfo(null); }}>
                  {i === 0 ? tilde(n.path, home) || n.name : n.name}
                </button>
                <small className="mono">{human(n.size)}</small>
              </span>
            ))}
          </nav>
          <p className="legendbar">
            <span><i className="sw dir" /> {t("legendDir")}</span><span><i className="sw file" /> {t("legendFile")}</span>
            <span className="muted">{t("exploreTip")}</span>
          </p>
          <div className="split">
            <motion.div className="tm-wrap" key={cur.path} initial={{ opacity: 0, scale: 0.955 }} animate={{ opacity: 1, scale: 1 }} transition={{ duration: 0.32, ease: [0.2, 0.8, 0.2, 1] }}>
              <Treemap node={cur} selected={sel?.path ?? null} onSelect={choose} onOpen={open} />
            </motion.div>
            <aside className="side">
              <h2>{sel ? sel.name : t("largest")}</h2>
              {!sel && (
                <ol className="list">
                  {cur.children.slice(0, 40).map((c) => (
                    <li key={c.path + c.name}><button onClick={() => choose(c)} onDoubleClick={() => c.is_dir && c.children.length && open(c)}>
                      <span className="trunc">{c.name}{c.is_dir ? "/" : ""}</span><span className="mono">{human(c.size)}</span>
                    </button></li>
                  ))}
                </ol>
              )}
              {sel && (
                <dl className="facts">
                  <dt>{sel.is_dir ? t("folder") : t("file")}</dt><dd className="mono wrap">{tilde(sel.path, home)}</dd>
                  <dt>{t("onDisk")}</dt>
                  <dd className="mono">{human(sel.size)}{sel.files > 1 ? ` · ${sel.files.toLocaleString()} ${t("files")}` : ""}</dd>
                  <dd className="meter"><i style={{ width: `${cur.size ? Math.max(2, (sel.size / cur.size) * 100) : 0}%` }} /></dd>
                  <dd className="muted">{cur.size ? Math.round((sel.size / cur.size) * 100) : 0}% {t("ofParent")}</dd>
                  {sel.logical !== sel.size && (<><dt>{t("logical")}</dt><dd className="mono">{human(sel.logical)}</dd></>)}
                  {info?.catalog.map(([id, label]) => (<><dt key={id}>Catalog</dt><dd>{label}</dd></>))}
                  {info?.protected && (<><dt>{t("protected")}</dt><dd className="warn">{info.protected}</dd></>)}
                  <div className="actions">
                    {sel.is_dir && sel.children.length > 0 && <button className="btn" onClick={() => open(sel)}>{t("open")}</button>}
                    {!sel.name.startsWith("(") && <button className="btn" onClick={() => api.reveal(sel.path)}><FolderOpen size={14} aria-hidden /> {t("reveal")}</button>}
                    <button className="btn danger" disabled={!removable} onClick={makePlan}>{t("movePlan")}</button>
                    <button className="btn ghost" onClick={() => { setSel(null); setInfo(null); }}>{t("back")}</button>
                  </div>
                </dl>
              )}
            </aside>
          </div>
        </>
      )}
      <PlanDialog plan={plan} onClose={() => setPlan(null)} onDone={() => { void refresh(); void run(stack[0]?.path ?? path); }} />
    </div>
  );
}

function ScanLive({ prog, recent, used }: { prog: Progress | null; recent: string[]; used: number }) {
  const { t, home } = useApp();
  const files = useCountUp(prog?.files ?? 0, 0.35);
  const bytes = prog?.bytes ?? 0;
  const pct = used > 0 ? Math.min(99, (bytes / used) * 100) : 0;
  return (
    <div className="scanlive" role="status">
      <div className="sonar" aria-hidden><i /><i /><i /><b /></div>
      <p className="eyebrow">{t("scanLive")}</p>
      <p className="scan-num"><b>{Math.round(files).toLocaleString()}</b> <span>{t("files")}</span></p>
      <p className="scan-bytes mono">{human(bytes)} <span className="muted">· {pct.toFixed(0)}% {t("ofUsed")}</span></p>
      <div className="progress" aria-hidden><i style={{ width: `${pct}%` }} /></div>
      <ul className="stream">
        {recent.map((p, i) => <li key={p + i} style={{ opacity: 1 - i * 0.2 }} className="mono trunc">{tilde(p, home)}</li>)}
      </ul>
    </div>
  );
}
