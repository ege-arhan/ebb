import { ChevronRight, FolderOpen, FolderSearch, Square } from "lucide-react";
import { motion } from "motion/react";
import { useCountUp } from "../components/Bits";
import { useCallback, useEffect, useState } from "react";
import { PlanDialog } from "../components/PlanDialog";
import { Treemap } from "../components/Treemap";
import { api } from "../ipc";
import { human, tilde } from "../format";
import { useApp } from "../state";
import type { Explain, Node, Plan, Progress } from "../types";

export function Explore({ hidden, startScan, clearStart }: { hidden: boolean; startScan: boolean; clearStart(): void }) {
  const { t, home, toast, refresh, disk } = useApp();
  const [recent, setRecent] = useState<string[]>([]);
  const [path, setPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [prog, setProg] = useState<Progress | null>(null);
  const [stack, setStack] = useState<Node[]>([]);
  const [sel, setSel] = useState<Node | null>(null);
  const [info, setInfo] = useState<Explain | null>(null);
  const [plan, setPlan] = useState<Plan | null>(null);

  useEffect(() => { if (!path && home) setPath(home); }, [home, path]);

  const run = useCallback(async (p: string | null) => {
    setBusy(true); setProg(null); setSel(null); setInfo(null); setRecent([]);
    try {
      const root = await api.scan(p, (pr) => { setProg(pr); setRecent((r) => [pr.current, ...r].slice(0, 5)); });
      setStack([root]);
    } catch (e) {
      const msg = String(e);
      if (!msg.includes("iptal")) toast(`${t("scanError")}: ${msg}`);
    } finally { setBusy(false); }
  }, [t, toast]);

  useEffect(() => { if (startScan) { clearStart(); void run(path || null); } }, [startScan, clearStart, run, path]);

  const cur = stack[stack.length - 1];
  const choose = async (n: Node) => { setSel(n); setInfo(null); if (!n.name.startsWith("(")) setInfo(await api.explain(n.path)); };
  const open = (n: Node) => { setStack((s) => [...s, n]); setSel(null); setInfo(null); };
  const removable = sel && !sel.name.startsWith("(") && !info?.protected;
  const makePlan = async () => sel && setPlan(await api.makePlan([], [], [sel.path]));

  return (
    <div className="page explore" hidden={hidden}>
      <header className="topbar">
        <form onSubmit={(e) => { e.preventDefault(); void run(path); }} className="pathform">
          <input value={path} onChange={(e) => setPath(e.target.value)} spellCheck={false} aria-label="Path" />
          {busy
            ? <button type="button" className="btn" onClick={() => api.cancelScan()}><Square size={14} aria-hidden /> {t("stop")}</button>
            : <button className="btn primary"><FolderSearch size={15} aria-hidden /> {t("scan")}</button>}
        </form>
      </header>

      {busy && <ScanLive prog={prog} recent={recent} used={disk ? disk.total - disk.free : 0} />}

      {!busy && !cur && (
        <div className="empty-state big">
          <h2>{t("start")}</h2>
          <p>{t("startBody")}</p>
        </div>
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
