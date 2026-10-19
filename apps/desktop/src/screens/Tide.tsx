import { ArrowRight, Eye, History, ListChecks, Plug, Trash2, Undo2 } from "lucide-react";
import type { Screen } from "../App";
import { Basin } from "../components/Basin";
import { Skeleton, Stat } from "../components/Bits";
import { History as DiskHistory } from "../components/History";
import { ago, human, tilde } from "../format";
import { useApp } from "../state";

export function Tide({ go }: { go(s: Screen): void }) {
  const { t, findings, ai, reclaimable, home, journal, lang, clients, trashSize } = useApp();
  const top = (findings ?? []).filter((f) => f.safety === "safe").slice(0, 5);
  const review = (findings ?? []).filter((f) => f.safety === "review").reduce((a, f) => a + f.size, 0);
  const stores = (ai ?? []).slice(0, 4);
  const aiTotal = (ai ?? []).reduce((a, s) => a + s.size, 0);
  const max = stores[0]?.size || 1;
  const last = journal.find((j) => j.status === "applied");
  const pending = clients.filter((c) => c.installed && !c.connected);
  return (
    <div className="page">
      <Basin />
      <p className="hint">{t("basinHint")}</p>

      <div className="stats">
        <Stat tone="safe" label={t("reclaimable")} bytes={reclaimable} hint={t("reclaimableHint")} onClick={() => go("clean")} />
        <Stat tone="review" label={t("needsLook")} bytes={review} hint={t("needsLookHint")} onClick={() => go("clean")} />
        <Stat tone="ai" label={t("aiTools")} bytes={aiTotal} hint={t("aiHint")} onClick={() => go("agents")} />
      </div>

      {trashSize > 500_000_000 && (
        <button className="nudge" onClick={() => go("trash")}>
          <span className="nudge-ico"><Trash2 size={20} aria-hidden /></span>
          <span className="rowtext"><b>{t("trashNudge").replace("{size}", human(trashSize))}</b><small>{t("trashNudgeBody")}</small></span>
          <ArrowRight size={18} aria-hidden />
        </button>
      )}

      {pending.length > 0 && (
        <button className="nudge" onClick={() => go("connect")}>
          <span className="nudge-ico"><Plug size={20} aria-hidden /></span>
          <span className="rowtext"><b>{t("connectNudge")}</b><small>{t("connectNudgeBody")}</small></span>
          <ArrowRight size={18} aria-hidden />
        </button>
      )}

      <div className="two">
        <section className="panel">
          <header>
            <h2>{t("biggestSafe")}</h2>
            <button className="btn primary" onClick={() => go("clean")} disabled={!reclaimable}>
              {t("reviewClean")} <ArrowRight size={15} aria-hidden />
            </button>
          </header>
          {findings === null && <Skeleton rows={4} />}
          {findings && !top.length && <p className="muted">{t("emptyFindings")}</p>}
          <ul className="rows">
            {top.map((f) => (
              <li key={f.path}>
                <span className="rowtext"><b>{f.label}</b><small title={f.path}>{tilde(f.path, home)}</small></span>
                <span className="mono">{human(f.size)}</span>
              </li>
            ))}
          </ul>
        </section>
        <section className="panel">
          <header>
            <h2>{t("agents")}</h2>
            <button className="btn" onClick={() => go("agents")}>{t("open")}</button>
          </header>
          {ai === null && <Skeleton rows={4} />}
          <ul className="bars">
            {stores.map((s) => (
              <li key={s.id}>
                <span className="barlabel"><b>{s.label}</b><span className="mono">{human(s.size)}</span></span>
                <span className="bar"><i className={s.kind} style={{ width: `${Math.max(3, (s.size / max) * 100)}%` }} /></span>
              </li>
            ))}
          </ul>
          {last && (
            <p className="lastline"><History size={14} aria-hidden /> {t("lastCleanup")}: <b className="mono">{human(last.total)}</b> · {ago(last.created, lang)}</p>
          )}
        </section>
      </div>

      <DiskHistory />

      <section className="flow" aria-label={t("safeFlow")}>
        <h2>{t("safeFlow")}</h2>
        <ol>
          {[
            [ListChecks, t("flow1"), t("flow1b")],
            [Eye, t("flow2"), t("flow2b")],
            [Undo2, t("flow3"), t("flow3b")],
          ].map(([Icon, title, body], i) => {
            const I = Icon as typeof Eye;
            return (
              <li key={i}>
                <span className="flow-ico"><I size={18} aria-hidden /></span>
                <b>{title as string}</b>
                <small>{body as string}</small>
              </li>
            );
          })}
        </ol>
      </section>
    </div>
  );
}
