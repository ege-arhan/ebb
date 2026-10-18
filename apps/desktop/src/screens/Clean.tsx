import { ChevronDown, FolderOpen, Lock, Search, ShieldCheck, TriangleAlert } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useState } from "react";
import { CatIcon, PageHead, Skeleton, useCountUp } from "../components/Bits";
import { PlanDialog } from "../components/PlanDialog";
import { api } from "../ipc";
import { human, tilde } from "../format";
import type { Key } from "../i18n";
import { useApp } from "../state";
import type { Finding, Plan, Safety } from "../types";

const GROUPS: { safety: Safety; title: Key; body: Key; icon: typeof Lock }[] = [
  { safety: "safe", title: "safeSection", body: "safeBody", icon: ShieldCheck },
  { safety: "review", title: "reviewSection", body: "reviewBody", icon: TriangleAlert },
  { safety: "never", title: "neverSection", body: "neverBody", icon: Lock },
];
const key = (f: Finding) => f.entry_id + f.path;

export function Clean() {
  const { t, findings, home } = useApp();
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [plan, setPlan] = useState<Plan | null>(null);
  const [initFor, setInitFor] = useState<Finding[] | null>(null);
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [cat, setCat] = useState("all");
  const [q, setQ] = useState("");

  useEffect(() => {
    if (findings && findings !== initFor) {
      setInitFor(findings);
      setPicked(new Set(findings.filter((f) => f.safety === "safe").map(key)));
    }
  }, [findings, initFor]);

  const cats = useMemo(() => {
    const m = new Map<string, number>();
    (findings ?? []).forEach((f) => m.set(f.category, (m.get(f.category) ?? 0) + 1));
    return [...m.entries()];
  }, [findings]);
  const visible = useMemo(
    () => (findings ?? []).filter((f) => (cat === "all" || f.category === cat) && (!q || (f.label + f.path).toLowerCase().includes(q.toLowerCase()))),
    [findings, cat, q],
  );
  const total = (findings ?? []).filter((f) => picked.has(key(f))).reduce((a, f) => a + f.size, 0);
  const animated = useCountUp(total);
  const toggle = (k: string) => setPicked((s) => { const n = new Set(s); if (n.has(k)) n.delete(k); else n.add(k); return n; });
  const toggleOpen = (k: string) => setOpen((s) => { const n = new Set(s); if (n.has(k)) n.delete(k); else n.add(k); return n; });
  const setGroup = (rows: Finding[], on: boolean) => setPicked((s) => { const n = new Set(s); rows.forEach((f) => (on ? n.add(key(f)) : n.delete(key(f)))); return n; });

  const review = async () => {
    const chosen = (findings ?? []).filter((f) => picked.has(key(f)));
    setPlan(await api.makePlan(chosen, chosen.map((f) => f.entry_id), []));
  };

  return (
    <div className="page">
      <PageHead eyebrow={t("clean")} title={t("clean")} lead={t("cleanLead")} />

      {findings === null && <Skeleton rows={6} />}
      {findings?.length === 0 && <p className="empty-state">{t("emptyFindings")}</p>}

      {findings && findings.length > 0 && (
        <div className="filters">
          <div className="chips" role="group">
            <button aria-pressed={cat === "all"} onClick={() => setCat("all")}>{t("all")} <small>{findings.length}</small></button>
            {cats.map(([c, n]) => (
              <button key={c} aria-pressed={cat === c} onClick={() => setCat(c)}>
                <CatIcon category={c} size={14} /> {t(`cat_${c}` as Key)} <small>{n}</small>
              </button>
            ))}
          </div>
          <label className="search"><Search size={15} aria-hidden /><input value={q} onChange={(e) => setQ(e.target.value)} placeholder={t("search")} /></label>
        </div>
      )}

      {GROUPS.map(({ safety, title, body, icon: Icon }) => {
        const rows = visible.filter((f) => f.safety === safety);
        if (!rows.length) return null;
        const max = rows[0].size || 1;
        const allOn = rows.every((f) => picked.has(key(f)));
        return (
          <section key={safety} className={`group ${safety}`}>
            <header className="group-head">
              <div>
                <h2><Icon size={16} aria-hidden /> {t(title)} <span className="mono muted">{human(rows.reduce((a, f) => a + f.size, 0))}</span></h2>
                <p className="muted">{t(body)}</p>
              </div>
              {safety !== "never" && <button className="btn ghost" onClick={() => setGroup(rows, !allOn)}>{allOn ? t("selectNone") : t("selectAll")}</button>}
            </header>
            <ul className="rows cards">
              {rows.map((f, i) => {
                const k = key(f);
                const isOpen = open.has(k);
                return (
                  <motion.li key={k} className={safety === "never" ? "locked" : ""} initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: Math.min(i * 0.025, 0.2) }}>
                    <div className="row-main">
                      <label>
                        <input type="checkbox" disabled={safety === "never"} checked={picked.has(k)} onChange={() => toggle(k)} />
                        <span className="cat"><CatIcon category={f.category} /></span>
                        <span className="rowtext"><b>{f.label}</b><small title={f.path}>{tilde(f.path, home)}</small></span>
                      </label>
                      <span className="mini"><i style={{ width: `${Math.max(4, (f.size / max) * 100)}%` }} /></span>
                      <span className="mono size">{human(f.size)}</span>
                      <button className="icon-btn" aria-expanded={isOpen} aria-label={t("details")} onClick={() => toggleOpen(k)}><ChevronDown size={16} className={isOpen ? "flip" : ""} /></button>
                    </div>
                    <AnimatePresence initial={false}>
                      {isOpen && (
                        <motion.div className="row-more" initial={{ height: 0, opacity: 0 }} animate={{ height: "auto", opacity: 1 }} exit={{ height: 0, opacity: 0 }}>
                          <div>
                            <p>{f.note ?? t(body)}</p>
                            <dl className="inline">
                              <dt>{t("path")}</dt><dd className="mono wrap">{f.path}</dd>
                              <dt>{t("size")}</dt><dd className="mono">{human(f.size)} · {f.files.toLocaleString()} {t("filesCount")}</dd>
                            </dl>
                            <button className="btn" onClick={() => api.reveal(f.path)}><FolderOpen size={14} aria-hidden /> {t("reveal")}</button>
                          </div>
                        </motion.div>
                      )}
                    </AnimatePresence>
                  </motion.li>
                );
              })}
            </ul>
          </section>
        );
      })}
      <div className={`dock${picked.size ? " show" : ""}`}>
        <span><b className="mono big">{human(animated)}</b> <span className="muted">· {picked.size} {t("selected")}</span></span>
        <button className="btn primary" onClick={review} disabled={!picked.size}>{t("movePlan")}</button>
      </div>
      <PlanDialog plan={plan} onClose={() => setPlan(null)} />
    </div>
  );
}
