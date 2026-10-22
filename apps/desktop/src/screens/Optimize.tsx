import { Archive, ChevronDown, Copy, FolderPlus, Loader2, PackageOpen, Sparkles, Undo2 } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useState } from "react";
import { PageHead, Skeleton, useCountUp } from "../components/Bits";
import { PlanDialog } from "../components/PlanDialog";
import { human, tilde } from "../format";
import { api } from "../ipc";
import { useApp } from "../state";
import type { CompressAnalysis, CompressReport, DupGroup, DupProgress, FoundFile, Plan } from "../types";
import type { Key } from "../i18n";

// Oturum boyunca saklanan sonuçlar: ekranlar arasında gidip gelince yeniden taranmaz.
const cache: { large?: FoundFile[]; installers?: FoundFile[] } = {};

export function Optimize() {
  const { t, home, toast, celebrateWith, refresh } = useApp();
  const [phase, setPhase] = useState<"idle" | "scan" | "done">("idle");
  const [prog, setProg] = useState<DupProgress | null>(null);
  const [groups, setGroups] = useState<DupGroup[]>([]);
  const [cloneOk, setCloneOk] = useState(true);
  const [openG, setOpenG] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [cmp, setCmp] = useState<CompressReport | null>(null);
  const [ana, setAna] = useState<CompressAnalysis | null>(null);
  const [large, setLarge] = useState<FoundFile[] | null>(cache.large ?? null);
  const [inst, setInst] = useState<FoundFile[] | null>(cache.installers ?? null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [plan, setPlan] = useState<Plan | null>(null);
  const files = useCountUp(prog?.files ?? 0, 0.3);

  useEffect(() => {
    if (!cache.installers) api.oldInstallers().then((x) => { cache.installers = x; setInst(x); }).catch(() => setInst([]));
    if (!cache.large) api.largeOld(null).then((x) => { cache.large = x; setLarge(x); }).catch(() => setLarge([]));
  }, []);

  const pending = groups.filter((g) => !g.optimized && g.reclaimable > 0);
  const total = pending.reduce((a, g) => a + g.reclaimable, 0);

  const find = async () => {
    setPhase("scan"); setProg(null);
    try {
      const r = await api.findDuplicates(null, setProg);
      setGroups(r.groups); setCloneOk(r.cloneSupported);
    } catch (e) { toast(String(e)); }
    setPhase("done");
  };
  const optimizeAll = async () => {
    setBusy("dupes");
    try {
      const r = await api.optimizeClones(pending);
      if (r.saved > 0) celebrateWith(r.saved, "optimize");
      if (r.skipped.length) toast(`${r.skipped.length} skipped: ${r.skipped[0][1]}`);
      const skipped = new Set(r.skipped.map(([p]) => p));
      setGroups((gs) => gs.map((g) => (pending.includes(g) && !g.files.slice(1).some((f) => skipped.has(f)) ? { ...g, optimized: true, reclaimable: 0 } : g)));
    } catch (e) { toast(String(e)); }
    setBusy(null);
  };
  const trashCopies = async () => setPlan(await api.makePlan([], [], pending.flatMap((g) => g.files.slice(1))));

  const finishCompress = (r: CompressReport, undo: boolean) => {
    const pct = r.before ? ((r.before - r.after) / r.before) * 100 : 0;
    if (!undo && pct >= 5) celebrateWith(r.before - r.after, "compress");
    else if (!undo) toast(t("compressLow").replace("{pct}", pct.toFixed(1)));
    setCmp(undo ? null : r);
    void refresh();
  };
  // Klasör seç → türlere göre çözümle → yalnızca değecek dosyaları sıkıştır.
  const analyze = async () => {
    const path = await api.pickFolder();
    if (!path) return;
    setBusy("analyze"); setAna(null); setCmp(null);
    try { setAna(await api.compressAnalyze(path)); } catch (e) { toast(String(e)); }
    setBusy(null);
  };
  const runSmart = async () => {
    if (!ana) return;
    setBusy("compress");
    try {
      const r = await api.compressSmart(ana.path, 0.1);
      finishCompress({ path: r.path, before: r.before, after: r.after, files: r.compressed_files }, false);
      if (r.skipped_files) toast(t("skipFiles").replace("{n}", String(r.skipped_files)));
      setAna(null);
    } catch (e) { toast(String(e)); }
    setBusy(null);
  };
  const compress = async (undo = false, path?: string) => {
    const target = undo ? cmp?.path : path ?? ana?.path;
    if (!target) return;
    setBusy(undo ? "decompress" : "compress");
    try {
      const r = undo ? await api.decompress(target) : await api.compress(target);
      finishCompress(r, undo);
      if (!undo) setAna(null);
    } catch (e) { toast(String(e)); }
    setBusy(null);
  };

  const toggle = (p: string) => setPicked((s) => { const n = new Set(s); if (n.has(p)) n.delete(p); else n.add(p); return n; });
  const review = async () => setPlan(await api.makePlan([], [], [...picked]));
  const pickedSize = [...(large ?? []), ...(inst ?? [])].filter((f) => picked.has(f.path)).reduce((a, f) => a + f.size, 0);

  const list = (rows: FoundFile[] | null) => (
    <>
      {rows === null && <Skeleton rows={3} />}
      {rows?.length === 0 && <p className="muted">{t("nothingFound")}</p>}
      <ul className="rows">
        {(rows ?? []).map((f) => (
          <li key={f.path}>
            <label>
              <input type="checkbox" checked={picked.has(f.path)} onChange={() => toggle(f.path)} />
              <span className="rowtext"><b>{f.path.split(/[\\/]/).pop()}</b><small title={f.path}>{tilde(f.path, home)} · {t("daysAgo").replace("{n}", String(f.age_days))}</small></span>
            </label>
            <span className="mono">{human(f.size)}</span>
          </li>
        ))}
      </ul>
    </>
  );

  return (
    <div className="page">
      <PageHead eyebrow={t("optimize")} title={t("optimizeTitle")} lead={t("optimizeLead")} />

      <section className="panel feature">
        <header>
          <div className="feat-head"><span className="feat-ico"><Copy size={18} aria-hidden /></span><div><h2>{t("dupTitle")}</h2><p className="muted">{t("dupBody")}</p></div></div>
          {phase !== "scan" && <button className="btn primary" onClick={find}><Sparkles size={15} aria-hidden /> {t("findDupes")}</button>}
        </header>
        {phase === "scan" && (
          <div className="inline-progress" role="status">
            <Loader2 size={16} className="spin" aria-hidden /> {t("dupScanning")} · <b className="mono">{Math.round(files).toLocaleString()}</b> {t("files")} · <span className="mono">{human(prog?.bytes ?? 0)}</span>
            <div className="progress indet" aria-hidden><i /></div>
          </div>
        )}
        {phase === "done" && groups.length === 0 && <p className="muted">{t("noDupes")}</p>}
        {phase === "done" && groups.length > 0 && (
          <>
            <div className="bigline">
              <span><b className="mono">{human(total)}</b> {t("won")} · {pending.length} × {t("copies")}</span>
              {cloneOk
                ? <button className="btn primary" disabled={!pending.length || busy !== null} onClick={optimizeAll}>{busy === "dupes" ? <Loader2 size={15} className="spin" aria-hidden /> : <Sparkles size={15} aria-hidden />} {busy === "dupes" ? t("optimizing") : t("optimizeAll")}</button>
                : <button className="btn" disabled={!pending.length} onClick={trashCopies}>{t("movePlan")}</button>}
            </div>
            {!cloneOk && <p className="note-inline">{t("cloneUnsupported")}</p>}
            <ul className="rows cards dupes">
              {groups.slice(0, 60).map((g) => (
                <li key={g.hash} className={g.optimized ? "done" : ""}>
                  <div className="row-main dup">
                    <span className="rowtext"><b>{g.files[0].split(/[\\/]/).pop()}</b><small>{g.files.length} {t("copies")} · {human(g.size)} {g.optimized ? `· ${t("alreadyOptimized")}` : ""}</small></span>
                    <span className="mono size">{g.optimized ? "✓" : `+${human(g.reclaimable)}`}</span>
                    <button className="icon-btn" aria-expanded={openG === g.hash} aria-label={t("details")} onClick={() => setOpenG(openG === g.hash ? null : g.hash)}><ChevronDown size={16} className={openG === g.hash ? "flip" : ""} /></button>
                  </div>
                  <AnimatePresence initial={false}>
                    {openG === g.hash && (
                      <motion.div className="row-more" initial={{ height: 0, opacity: 0 }} animate={{ height: "auto", opacity: 1 }} exit={{ height: 0, opacity: 0 }}>
                        <ul className="plan-list flat">{g.files.map((f) => <li key={f}><span className="plan-path"><small title={f}>{tilde(f, home)}</small></span></li>)}</ul>
                      </motion.div>
                    )}
                  </AnimatePresence>
                </li>
              ))}
            </ul>
          </>
        )}
      </section>

      <section className="panel feature">
        <header>
          <div className="feat-head"><span className="feat-ico"><Archive size={18} aria-hidden /></span><div><h2>{t("compressTitle")}</h2><p className="muted">{t("compressBody")}</p></div></div>
          <button className="btn primary" disabled={busy !== null} onClick={analyze}>{busy === "analyze" ? <Loader2 size={15} className="spin" aria-hidden /> : <FolderPlus size={15} aria-hidden />} {busy === "analyze" ? t("analyzing") : t("analyze")}</button>
        </header>
        {ana && (
          <div className="analysis">
            <p className="muted">{t("methodLabel")}: {ana.method}</p>
            <ul className="types">
              {ana.types.map((x) => (
                <li key={x.category} className={x.worthwhile ? "yes" : "no"}>
                  <span className="rowtext"><b>{t(`typ_${x.category}` as Key)}</b><small>{x.files.toLocaleString()} · {human(x.bytes)}{x.worthwhile ? "" : ` · ${x.reason}`}</small></span>
                  <span className="bar"><i style={{ width: `${Math.round(x.ratio * 100)}%` }} /></span>
                  <span className="mono">{x.worthwhile ? `−${human(x.est_saved)}` : "—"}</span>
                </li>
              ))}
            </ul>
            <div className="bigline">
              <span><b className="mono">{human(ana.est_saved)}</b> {t("estSave")}</span>
              <span className="row-actions">
                <button className="btn primary" disabled={busy !== null || !ana.supported || !ana.est_saved} onClick={runSmart}>{busy === "compress" ? <Loader2 size={15} className="spin" aria-hidden /> : <Sparkles size={15} aria-hidden />} {busy === "compress" ? t("compressing") : t("smartCompress")}</button>
                <button className="btn ghost" disabled={busy !== null || !ana.supported} onClick={() => compress(false)}>{t("compressAll")}</button>
              </span>
            </div>
          </div>
        )}
        {cmp && (
          <div className="bigline">
            <span>{t("compressedNote")} <b>{tilde(cmp.path, home)}</b>: <span className="mono">{human(cmp.before)} → {human(cmp.after)}</span> · <b className="mono">{human(Math.max(0, cmp.before - cmp.after))}</b> {t("savedWord")}</span>
            <button className="btn ghost" disabled={busy !== null} onClick={() => compress(true)}><Undo2 size={14} aria-hidden /> {t("decompressBtn")}</button>
          </div>
        )}
      </section>

      <div className="two">
        <section className="panel">
          <div className="feat-head"><span className="feat-ico amber"><PackageOpen size={18} aria-hidden /></span><div><h2>{t("installersTitle")}</h2><p className="muted">{t("installersBody")}</p></div></div>
          {list(inst)}
        </section>
        <section className="panel">
          <div className="feat-head"><span className="feat-ico amber"><Archive size={18} aria-hidden /></span><div><h2>{t("largeTitle")}</h2><p className="muted">{t("largeBody")}</p></div></div>
          {list(large)}
        </section>
      </div>

      <div className={`dock${picked.size ? " show" : ""}`}>
        <span><b className="mono big">{human(pickedSize)}</b> <span className="muted">· {picked.size} {t("selected")}</span></span>
        <button className="btn primary" onClick={review} disabled={!picked.size}>{t("reviewSelected")}</button>
      </div>
      <PlanDialog plan={plan} onClose={() => setPlan(null)} onDone={() => { setPicked(new Set()); cache.large = undefined; cache.installers = undefined; }} />
    </div>
  );
}
