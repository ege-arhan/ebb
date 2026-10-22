import { Activity, AlertTriangle, CheckCircle2, Cpu, HardDrive, Info, Loader2, Power, Rocket, XCircle } from "lucide-react";
import { motion } from "motion/react";
import { useEffect, useState } from "react";
import type { Screen } from "../App";
import { PageHead, Skeleton, useCountUp } from "../components/Bits";
import { Modal } from "../components/Overlay";
import { human } from "../format";
import { api } from "../ipc";
import { useApp } from "../state";
import type { AppUsage, Health, HiddenItem, StartupItem, Tip } from "../types";

const SEV = { good: CheckCircle2, info: Info, warn: AlertTriangle, bad: XCircle } as const;

function Ring({ score }: { score: number }) {
  const v = useCountUp(score, 1.2);
  const r = 54, c = 2 * Math.PI * r;
  const tone = score >= 80 ? "good" : score >= 55 ? "warn" : "bad";
  return (
    <div className={`ring-gauge ${tone}`}>
      <svg viewBox="0 0 128 128" aria-hidden>
        <circle cx="64" cy="64" r={r} className="track" />
        <motion.circle cx="64" cy="64" r={r} className="val" strokeDasharray={c} initial={{ strokeDashoffset: c }} animate={{ strokeDashoffset: c * (1 - score / 100) }} transition={{ duration: 1.2, ease: [0.2, 0.8, 0.2, 1] }} />
      </svg>
      <b>{Math.round(v)}</b>
    </div>
  );
}

export function Speed({ go }: { go(s: Screen): void }) {
  const { t, toast } = useApp();
  const [health, setHealth] = useState<Health | null>(null);
  const [apps, setApps] = useState<AppUsage[] | null>(null);
  const [startup, setStartup] = useState<StartupItem[] | null>(null);
  const [confirm, setConfirm] = useState<AppUsage | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [hidden, setHidden] = useState<HiddenItem[] | null>(null);
  const { celebrateWith } = useApp();

  useEffect(() => {
    api.health().then(setHealth).catch(() => {});
    api.startupItems().then(setStartup).catch(() => setStartup([]));
    api.hiddenList().then(setHidden).catch(() => setHidden([]));
    const load = () => { if (!document.hidden) api.topApps().then(setApps).catch(() => setApps([])); };
    load();
    // Yalnızca bu ekran açıkken ve pencere görünürken yenilenir: arka planda yük yok.
    const id = setInterval(load, 5000);
    return () => clearInterval(id);
  }, []);

  const quit = async (a: AppUsage) => {
    setConfirm(null); setBusy(a.name);
    try { await api.quitApp(a.pids); toast(`${t("quitDone")}: ${a.name}`); setTimeout(() => api.topApps().then(setApps), 1500); }
    catch (e) { toast(String(e)); }
    setBusy(null);
  };
  const flip = async (s: StartupItem) => {
    setBusy(s.id);
    try { await api.setStartup(s.id, !s.enabled); setStartup((xs) => xs?.map((x) => (x.id === s.id ? { ...x, enabled: !x.enabled } : x)) ?? null); api.health().then(setHealth); }
    catch (e) { toast(String(e)); }
    setBusy(null);
  };
  const act = (tip: Tip) => {
    if (tip.action === "clean") go("clean");
    else if (tip.action === "optimize") go("optimize");
    else if (tip.action === "apps") document.getElementById("apps")?.scrollIntoView({ behavior: "smooth" });
    else if (tip.action === "startup") document.getElementById("startup")?.scrollIntoView({ behavior: "smooth" });
  };

  const m = health?.memory;
  const maxMem = apps?.[0]?.memory || 1;
  const up = health ? (health.uptime_secs >= 86400 ? `${Math.floor(health.uptime_secs / 86400)} ${t("days")}` : `${Math.floor(health.uptime_secs / 3600)} ${t("hours")}`) : "";

  return (
    <div className="page">
      <PageHead eyebrow={t("speed")} title={t("systemTitle")} lead={t("systemLead")} />

      <section className="panel healthcard">
        {!health ? <Skeleton rows={4} /> : (
          <>
            <div className="health-left">
              <Ring score={health.score} />
              <p className="eyebrow">{t("healthScore")}</p>
            </div>
            <ul className="tips">
              {health.tips.map((tip) => {
                const I = SEV[tip.severity];
                return (
                  <li key={tip.id} className={tip.severity}>
                    <I size={18} aria-hidden />
                    <span className="rowtext"><b>{tip.title}</b><small>{tip.body}</small></span>
                    {tip.action && tip.action !== "restart" && <button className="btn ghost" onClick={() => act(tip)}>{t("tipGo")}</button>}
                  </li>
                );
              })}
            </ul>
          </>
        )}
      </section>

      {m && (
        <div className="stats">
          <div className="stat"><span className="eyebrow">{t("memoryTitle")}</span><b className="stat-value">{human(m.used)}</b><small>{t("memUsed")} {t("of")} {human(m.total)}</small><span className="mini"><i style={{ width: `${(m.used / m.total) * 100}%` }} /></span></div>
          <div className={`stat ${m.pressure > 0.8 ? "review" : "safe"}`}><span className="eyebrow">{t("pressure")}</span><b className="stat-value">{Math.round(m.pressure * 100)}%</b><small>{t("swap")}: {human(m.swap_used)}</small><span className="mini"><i style={{ width: `${m.pressure * 100}%` }} /></span></div>
          <div className="stat"><span className="eyebrow">{t("uptime")}</span><b className="stat-value">{up}</b><small>{health?.startup_enabled ?? 0} · {t("startupTitle")}</small></div>
        </div>
      )}

      <section className="panel" id="apps">
        <div className="feat-head"><span className="feat-ico"><Cpu size={18} aria-hidden /></span><div><h2>{t("appsTitle")}</h2><p className="muted">{t("appsBody")}</p></div></div>
        {apps === null && <Skeleton rows={5} />}
        <ul className="apps">
          {(apps ?? []).slice(0, 12).map((a) => (
            <motion.li key={a.name} layout>
              <span className="rowtext"><b>{a.name}</b><small>{a.pids.length > 1 ? `${a.pids.length} processes · ` : ""}{t("cpu")} {a.cpu.toFixed(1)}%</small></span>
              <span className="bar"><i style={{ width: `${Math.max(3, (a.memory / maxMem) * 100)}%` }} /></span>
              <span className="mono size">{human(a.memory)}</span>
              {a.quittable
                ? <button className="btn ghost" disabled={busy === a.name} onClick={() => setConfirm(a)}>{busy === a.name ? <Loader2 size={14} className="spin" aria-hidden /> : <Power size={14} aria-hidden />} {t("quit")}</button>
                : <span />}
            </motion.li>
          ))}
        </ul>
      </section>

      <section className="panel" id="startup">
        <div className="feat-head"><span className="feat-ico"><Rocket size={18} aria-hidden /></span><div><h2>{t("startupTitle")}</h2><p className="muted">{t("startupBody")}</p></div></div>
        {startup === null && <Skeleton rows={4} />}
        <ul className="rows startup">
          {(startup ?? []).map((s) => (
            <li key={s.id} className={s.can_toggle ? "" : "locked"}>
              <span className="rowtext"><b>{s.name}</b><small title={s.path}>{s.can_toggle ? s.path.split(/[\\/]/).slice(-2).join("/") : t("systemItem")}</small></span>
              <button role="switch" aria-checked={s.enabled} aria-label={s.name} className="switch" disabled={!s.can_toggle || busy === s.id} onClick={() => flip(s)}><i /></button>
            </li>
          ))}
        </ul>
      </section>

      {hidden && hidden.length > 0 && (
        <section className="panel" id="hidden">
          <div className="feat-head"><span className="feat-ico amber"><HardDrive size={18} aria-hidden /></span><div><h2>{t("hiddenTitle")}</h2><p className="muted">{t("hiddenBody")}</p></div></div>
          <ul className="rows">
            {hidden.map((h) => (
              <li key={h.id}>
                <span className="rowtext"><b>{h.label}</b><small className="wrapsmall">{h.description}</small><small className="mono">{h.command}</small></span>
                <span className="mono">{h.estimate ? human(h.estimate) : ""}</span>
                <button className="btn" disabled={!h.available || busy === h.id} title={h.needs_admin && !h.available ? t("hiddenNeedAdmin") : ""} onClick={async () => {
                  setBusy(h.id);
                  try { const r = await api.hiddenRun(h.id); const f = Math.max(0, r.after - r.before); if (f > 0) celebrateWith(f, "freed"); else toast(r.output.slice(0, 120) || t("cleanupDone")); setHidden(await api.hiddenList()); } catch (e) { toast(String(e)); }
                  setBusy(null);
                }}>{busy === h.id ? <Loader2 size={14} className="spin" aria-hidden /> : <Rocket size={14} aria-hidden />} {t("runCleanup")}</button>
              </li>
            ))}
          </ul>
        </section>
      )}

      <Modal open={!!confirm} onClose={() => setConfirm(null)} title={t("quit")}>
        {confirm && (
          <div className="plan">
            <h2><Activity size={18} aria-hidden /> {t("quitConfirm").replace("{name}", confirm.name)}</h2>
            <p className="muted">{t("quitBody")}</p>
            <footer className="plan-foot">
              <span className="muted mono">{human(confirm.memory)}</span>
              <button className="btn" onClick={() => setConfirm(null)}>{t("cancel")}</button>
              <button className="btn primary" onClick={() => quit(confirm)}><Power size={14} aria-hidden /> {t("quit")}</button>
            </footer>
          </div>
        )}
      </Modal>
    </div>
  );
}
