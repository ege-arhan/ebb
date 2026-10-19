import { CalendarClock, Loader2, Play } from "lucide-react";
import { useEffect, useState } from "react";
import { PageHead, Skeleton } from "../components/Bits";
import { ago } from "../format";
import type { Key } from "../i18n";
import { api } from "../ipc";
import { useApp } from "../state";
import type { Rule, Schedule } from "../types";

export function Care() {
  const { t, lang, toast } = useApp();
  const [rules, setRules] = useState<Rule[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  useEffect(() => { api.careRules().then(setRules).catch(() => setRules([])); }, []);

  const save = async (r: Rule) => { setRules((rs) => rs?.map((x) => (x.id === r.id ? r : x)) ?? null); await api.careSave(r); };
  const run = async (r: Rule) => {
    setBusy(r.id);
    try { const o = await api.careRun(r.id); toast(o.message); setRules(await api.careRules()); } catch (e) { toast(String(e)); }
    setBusy(null);
  };
  const when = (s: Schedule) => s.type === "daily" ? t("schedDaily") : s.type === "weekly" ? t("schedWeekly").replace("{d}", t(`d${s.weekday}` as Key)) : t("schedLow").replace("{n}", String(s.free_percent));

  return (
    <div className="page narrow">
      <PageHead eyebrow={t("care")} title={t("careTitle")} lead={t("careLead")} />
      {rules === null && <Skeleton rows={5} />}
      <ul className="rows cards">
        {(rules ?? []).map((r) => {
          const suggestOnly = r.kind.type === "large_old";
          return (
            <li key={r.id}>
              <div className="row-main care">
                <span className="rowtext">
                  <b><CalendarClock size={14} aria-hidden /> {r.name}</b>
                  <small>{when(r.schedule)} · {r.last_run ? `${t("lastRun")}: ${ago(r.last_run, lang)}${r.last_result ? ` · ${r.last_result}` : ""}` : t("neverRun")}</small>
                </span>
                <button className="btn ghost" disabled={busy !== null} onClick={() => run(r)}>{busy === r.id ? <Loader2 size={14} className="spin" aria-hidden /> : <Play size={14} aria-hidden />} {t("runNow")}</button>
                <button role="switch" aria-checked={r.enabled} aria-label={r.name} className="switch" onClick={() => save({ ...r, enabled: !r.enabled })}><i /></button>
              </div>
              <div className="care-sub">
                {suggestOnly
                  ? <small className="muted">{t("suggestOnly")}</small>
                  : <label className="check"><input type="checkbox" checked={r.auto_apply} onChange={(e) => save({ ...r, auto_apply: e.target.checked })} /> {t("autoApply")} <small className="muted">· {t("autoApplyHint")}</small></label>}
              </div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
