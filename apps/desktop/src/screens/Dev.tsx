import { Hammer, Loader2, Play, Terminal } from "lucide-react";
import { useEffect, useState } from "react";
import { PageHead, Skeleton } from "../components/Bits";
import { Modal } from "../components/Overlay";
import { PlanDialog } from "../components/PlanDialog";
import { human, tilde } from "../format";
import { api } from "../ipc";
import { useApp } from "../state";
import type { Artifact, Plan, PruneAction } from "../types";

export function Dev() {
  const { t, home, toast, refresh, celebrateWith } = useApp();
  const [arts, setArts] = useState<Artifact[] | null>(null);
  const [scanning, setScanning] = useState(false);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [plan, setPlan] = useState<Plan | null>(null);
  const [tools, setTools] = useState<PruneAction[] | null>(null);
  const [confirm, setConfirm] = useState<PruneAction | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => { api.pruneList().then(setTools).catch(() => setTools([])); }, []);
  const find = async () => {
    setScanning(true);
    try {
      const a = await api.projectArtifacts(30);
      setArts(a); setPicked(new Set(a.filter((x) => x.safety === "safe").map((x) => x.path)));
    } catch (e) { toast(String(e)); }
    setScanning(false);
  };
  const toggle = (p: string) => setPicked((s) => { const n = new Set(s); if (n.has(p)) n.delete(p); else n.add(p); return n; });
  const total = (arts ?? []).filter((a) => picked.has(a.path)).reduce((a, x) => a + x.size, 0);
  const review = async () => setPlan(await api.makePlan([], [], [...picked]));
  const run = async (a: PruneAction) => {
    setConfirm(null); setBusy(a.id);
    try {
      const r = await api.pruneRun(a.id);
      const freed = Math.max(0, r.after - r.before);
      if (freed > 0) celebrateWith(freed, "freed"); else toast(`${t("cleanupDone")}: ${a.label}`);
      void refresh();
    } catch (e) { toast(String(e)); }
    setBusy(null);
  };

  return (
    <div className="page">
      <PageHead eyebrow={t("dev")} title={t("devTitle")} lead={t("devLead")} />

      <section className="panel feature">
        <header>
          <div className="feat-head"><span className="feat-ico"><Hammer size={18} aria-hidden /></span><div><h2>{t("artifactsTitle")}</h2><p className="muted">{t("artifactsBody")}</p></div></div>
          <button className="btn primary" disabled={scanning} onClick={find}>{scanning ? <Loader2 size={15} className="spin" aria-hidden /> : <Hammer size={15} aria-hidden />} {t("findArtifacts")}</button>
        </header>
        {scanning && <Skeleton rows={4} />}
        {arts && arts.length === 0 && <p className="muted">{t("nothingFound")}</p>}
        <ul className="rows">
          {(arts ?? []).map((a) => (
            <li key={a.path}>
              <label>
                <input type="checkbox" checked={picked.has(a.path)} onChange={() => toggle(a.path)} />
                <span className="rowtext"><b>{a.kind} <span className="chip">{t("idleDays").replace("{n}", String(a.project_idle_days))}</span></b><small title={a.path}>{tilde(a.path, home)} · {t("rebuildWith")}: {a.rebuild}</small></span>
              </label>
              <span className="mono">{human(a.size)}</span>
            </li>
          ))}
        </ul>
      </section>

      <section className="panel feature">
        <div className="feat-head"><span className="feat-ico amber"><Terminal size={18} aria-hidden /></span><div><h2>{t("toolsTitle")}</h2><p className="muted">{t("toolsBody")}</p></div></div>
        {tools === null && <Skeleton rows={3} />}
        <ul className="rows">
          {(tools ?? []).map((a) => (
            <li key={a.id}>
              <span className="rowtext"><b>{a.label}</b><small>{a.description}</small></span>
              <span className="mono">{a.estimate ? `${t("estimate")} ${human(a.estimate)}` : ""}</span>
              <button className="btn" disabled={busy !== null} onClick={() => setConfirm(a)}>{busy === a.id ? <Loader2 size={14} className="spin" aria-hidden /> : <Play size={14} aria-hidden />} {busy === a.id ? t("running2") : t("runCleanup")}</button>
            </li>
          ))}
        </ul>
      </section>

      <div className={`dock${picked.size ? " show" : ""}`}>
        <span><b className="mono big">{human(total)}</b> <span className="muted">· {picked.size} {t("selected")}</span></span>
        <button className="btn primary" onClick={review} disabled={!picked.size}>{t("movePlan")}</button>
      </div>
      <PlanDialog plan={plan} onClose={() => setPlan(null)} onDone={() => { setArts(null); setPicked(new Set()); }} />
      <Modal open={!!confirm} onClose={() => setConfirm(null)} title={t("toolsTitle")}>
        {confirm && (
          <div className="plan">
            <h2>{confirm.label}</h2>
            <p className="muted">{confirm.description}</p>
            <pre className="promptbox">{confirm.command}</pre>
            <footer className="plan-foot"><span /><button className="btn" onClick={() => setConfirm(null)}>{t("cancel")}</button><button className="btn primary" onClick={() => run(confirm)}><Play size={14} aria-hidden /> {t("runCleanup")}</button></footer>
          </div>
        )}
      </Modal>
    </div>
  );
}
