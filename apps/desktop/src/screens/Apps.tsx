import { AppWindow, Loader2, Search, Trash2 } from "lucide-react";
import { motion } from "motion/react";
import { useEffect, useMemo, useState } from "react";
import { PageHead, Skeleton } from "../components/Bits";
import { PlanDialog } from "../components/PlanDialog";
import { human, tilde } from "../format";
import { api } from "../ipc";
import { useApp } from "../state";
import type { InstalledApp, Plan, UninstallPreview } from "../types";

export function Apps() {
  const { t, home, toast } = useApp();
  const [apps, setApps] = useState<InstalledApp[] | null>(null);
  const [q, setQ] = useState("");
  const [sel, setSel] = useState<UninstallPreview | null>(null);
  const [loading, setLoading] = useState(false);
  const [medium, setMedium] = useState(false);
  const [plan, setPlan] = useState<Plan | null>(null);

  useEffect(() => { api.appsList().then((a) => setApps(a.filter((x) => !x.system))).catch(() => setApps([])); }, []);
  const shown = useMemo(() => (apps ?? []).filter((a) => a.name.toLowerCase().includes(q.toLowerCase())), [apps, q]);
  const pick = async (a: InstalledApp) => {
    setLoading(true); setMedium(false);
    try { setSel(await api.appPreview(a.id)); } catch (e) { toast(String(e)); }
    setLoading(false);
  };
  const review = async () => { if (sel) setPlan(await api.appUninstallPlan(sel.app.id, medium)); };
  const used = (d: number | null) => (d === null ? t("never") : d === 0 ? t("today") : `${d}d`);
  const leftSize = sel ? sel.leftovers.filter((l) => medium || l.confidence === "high").reduce((a, l) => a + l.size, 0) : 0;

  return (
    <div className="page">
      <PageHead eyebrow={t("apps")} title={t("appsTitle2")} lead={t("appsLead")} />
      <div className="split-apps">
        <section className="panel">
          <label className="search wide"><Search size={15} aria-hidden /><input value={q} onChange={(e) => setQ(e.target.value)} placeholder={t("searchApps")} /></label>
          {apps === null && <Skeleton rows={6} />}
          <ul className="applist">
            {shown.map((a) => (
              <li key={a.id}>
                <button className={sel?.app.id === a.id ? "on" : ""} onClick={() => pick(a)}>
                  <span className="loc-ico"><AppWindow size={18} aria-hidden /></span>
                  <span className="rowtext"><b>{a.name}</b><small>{t("lastUsed")}: {used(a.last_used_days)}{a.running ? ` · ${t("running")}` : ""}</small></span>
                  <span className="mono">{human(a.size)}</span>
                </button>
              </li>
            ))}
          </ul>
        </section>
        <aside className="panel side-preview">
          {!sel && !loading && <p className="muted">{t("pickApp")}</p>}
          {loading && <Loader2 className="spin" aria-hidden />}
          {sel && !loading && (
            <motion.div key={sel.app.id} initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} className="preview">
              <h2>{sel.app.name} <small className="muted mono">{sel.app.version}</small></h2>
              <p className="total mono">{human(sel.app.size + leftSize)}</p>
              <h3 className="sub">{t("leftoversTitle")}</h3>
              <ul className="rows">
                <li><span className="rowtext"><b>{t("appItself")}</b><small>{tilde(sel.app.path, home)}</small></span><span className="mono">{human(sel.app.size)}</span></li>
                {sel.leftovers.map((l) => (
                  <li key={l.path} className={l.confidence === "medium" && !medium ? "dim" : ""}>
                    <span className="rowtext"><b>{l.kind} <span className={`chip ${l.confidence === "high" ? "applied" : ""}`}>{t(l.confidence)}</span></b><small title={l.path}>{tilde(l.path, home)}</small></span>
                    <span className="mono">{human(l.size)}</span>
                  </li>
                ))}
              </ul>
              {sel.leftovers.some((l) => l.confidence === "medium") && (
                <label className="check"><input type="checkbox" checked={medium} onChange={(e) => setMedium(e.target.checked)} /> {t("includeMedium")}</label>
              )}
              {sel.warnings.map((w) => <p key={w} className="note-inline">{w}</p>)}
              <button className="btn danger-solid big" disabled={sel.app.running} onClick={review}><Trash2 size={15} aria-hidden /> {sel.app.running ? t("quitFirst") : t("reviewUninstall")}</button>
            </motion.div>
          )}
        </aside>
      </div>
      <PlanDialog plan={plan} onClose={() => setPlan(null)} onDone={() => { setSel(null); api.appsList().then((a) => setApps(a.filter((x) => !x.system))); }} />
    </div>
  );
}
