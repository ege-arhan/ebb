import { Check, Hammer, Loader2, PackageOpen, RotateCw, Sparkles, Trash2, Zap } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useMemo, useState } from "react";
import { PageHead, useCountUp } from "../components/Bits";
import { PlanDialog } from "../components/PlanDialog";
import { human } from "../format";
import type { Key } from "../i18n";
import { api } from "../ipc";
import { useApp } from "../state";
import type { Plan } from "../types";

type CatId = "caches" | "installers" | "projects" | "trash";
interface Cat { id: CatId; icon: typeof Zap; body: Key; size: number; count: number; paths: string[]; ids: string[]; pending: boolean; done: boolean; trash?: boolean }

const START: Cat[] = [
  { id: "caches", icon: Zap, body: "smartCachesBody", size: 0, count: 0, paths: [], ids: [], pending: true, done: false },
  { id: "installers", icon: PackageOpen, body: "smartInstallersBody", size: 0, count: 0, paths: [], ids: [], pending: true, done: false },
  { id: "projects", icon: Hammer, body: "smartProjectsBody", size: 0, count: 0, paths: [], ids: [], pending: true, done: false },
  { id: "trash", icon: Trash2, body: "smartTrashBody", size: 0, count: 0, paths: [], ids: [], pending: true, done: false, trash: true },
];

export function Smart() {
  const { t, refresh, findings: cachedFindings } = useApp();
  const [phase, setPhase] = useState<"idle" | "scan" | "done">("idle");
  const [cats, setCats] = useState<Cat[]>(START);
  const [on, setOn] = useState<Set<CatId>>(new Set(["caches", "installers", "projects"]));
  const [plan, setPlan] = useState<Plan | null>(null);
  const [busy, setBusy] = useState(false);
  const { toast, celebrateWith } = useApp();

  const patch = (id: CatId, p: Partial<Cat>) => setCats((cs) => cs.map((c) => (c.id === id ? { ...c, ...p, pending: false, done: true } : c)));

  const run = async () => {
    setPhase("scan"); setCats(START.map((c) => ({ ...c })));
    const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
    // Her kategori kendi hızında çözülür; ekran canlı dolar.
    const jobs = [
      api.findings(false).then((f) => {
        const safe = f.filter((x) => x.safety === "safe");
        patch("caches", { size: safe.reduce((a, x) => a + x.size, 0), count: safe.length, ids: safe.map((x) => x.entry_id), paths: [] });
        return safe;
      }),
      api.oldInstallers().then((i) => patch("installers", { size: i.reduce((a, x) => a + x.size, 0), count: i.length, paths: i.map((x) => x.path) })),
      api.projectArtifacts(30).then((a) => { const s = a.filter((x) => x.safety === "safe"); patch("projects", { size: s.reduce((x, y) => x + y.size, 0), count: s.length, paths: s.map((x) => x.path) }); }),
      api.trashList().then((l) => patch("trash", { size: l.reduce((a, x) => a + x.size, 0), count: l.length })),
    ].map((p) => p.catch(() => undefined));
    await Promise.all([...jobs, wait(1600)]);
    setPhase("done");
    void refresh();
  };

  const total = useMemo(() => cats.filter((c) => on.has(c.id) && !c.trash).reduce((a, c) => a + c.size, 0), [cats, on]);
  const shown = useCountUp(total, 0.8);
  const trashCat = cats.find((c) => c.trash);
  const toggle = (id: CatId) => setOn((s) => { const n = new Set(s); if (n.has(id)) n.delete(id); else n.add(id); return n; });

  const review = async () => {
    setBusy(true);
    try {
      const chosen = cats.filter((c) => on.has(c.id) && !c.trash);
      const findings = (await api.findings(false)).filter((f) => f.safety === "safe" && on.has("caches"));
      const ids = chosen.flatMap((c) => c.ids);
      const paths = chosen.flatMap((c) => c.paths);
      setPlan(await api.makePlan(findings, ids, paths));
    } catch (e) { toast(String(e)); }
    setBusy(false);
  };
  const emptyTrash = async () => {
    setBusy(true);
    try { const b = await api.trashEmpty(); if (b > 0) celebrateWith(b, "freed"); patch("trash", { size: 0, count: 0 }); void refresh(); } catch (e) { toast(String(e)); }
    setBusy(false);
  };
  void cachedFindings;

  return (
    <div className="page">
      <PageHead eyebrow={t("smart")} title={t("smartTitle")} lead={t("smartLead")} />

      <section className={`smart-hero ${phase}`}>
        <div className="orbit" aria-hidden>
          <i className="ring r1" /><i className="ring r2" /><i className="ring r3" />
          {cats.map((c, k) => { const I = c.icon; return <span key={c.id} className={`planet p${k} ${c.done ? "lit" : ""}`}><I size={16} /></span>; })}
          <b className="core"><Sparkles size={26} /></b>
        </div>
        <div className="smart-copy">
          {phase === "idle" && <button className="btn primary huge" onClick={run}><Sparkles size={20} aria-hidden /> {t("smartStart")}</button>}
          {phase === "scan" && <p className="smart-scanning"><Loader2 size={18} className="spin" aria-hidden /> {t("smartScanning")}…</p>}
          {phase === "done" && (
            <>
              <p className="eyebrow">{t("smartFound")}</p>
              <p className="smart-total"><b>{human(shown)}</b></p>
              <div className="smart-actions">
                <button className="btn primary huge" disabled={!total || busy} onClick={review}>{busy ? <Loader2 size={18} className="spin" aria-hidden /> : <Check size={18} aria-hidden />} {t("smartReview")}</button>
                <button className="btn ghost" onClick={run}><RotateCw size={14} aria-hidden /> {t("smartAgain")}</button>
              </div>
              {!total && <p className="muted">{t("smartNothing")}</p>}
            </>
          )}
        </div>
      </section>

      <ul className="smart-cats">
        {cats.map((c, i) => {
          const I = c.icon;
          return (
            <motion.li key={c.id} className={`${c.done ? "done" : ""} ${on.has(c.id) || c.trash ? "" : "off"}`} initial={{ opacity: 0, y: 10 }} animate={{ opacity: phase === "idle" ? 0.55 : 1, y: 0 }} transition={{ delay: i * 0.06 }}>
              <span className="cat-ico"><I size={18} aria-hidden /></span>
              <span className="rowtext"><b>{t(`cat_${c.id}` as Key)}</b><small>{t(c.body)}{c.done && c.count ? ` · ${c.count}` : ""}</small></span>
              <span className="mono size">{c.pending && phase === "scan" ? <Loader2 size={14} className="spin" aria-hidden /> : c.done ? human(c.size) : "—"}</span>
              {c.trash
                ? <button className="btn" disabled={!c.size || busy} onClick={emptyTrash}><Trash2 size={14} aria-hidden /> {t("trashEmptyBtn")}</button>
                : <button role="switch" aria-checked={on.has(c.id)} aria-label={t(`cat_${c.id}` as Key)} className="switch" disabled={!c.done} onClick={() => toggle(c.id)}><i /></button>}
            </motion.li>
          );
        })}
      </ul>
      <p className="muted small">{t("smartSkipNote")}</p>
      <AnimatePresence>{trashCat && phase === "done" && trashCat.size > 0 && <motion.p className="note-inline" initial={{ opacity: 0 }} animate={{ opacity: 1 }}>{t("trashHint")}</motion.p>}</AnimatePresence>
      <PlanDialog plan={plan} onClose={() => setPlan(null)} onDone={() => void run()} />
    </div>
  );
}
