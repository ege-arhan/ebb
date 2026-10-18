import { useState } from "react";
import { human, tilde } from "../format";
import { useApp } from "../state";
import type { Plan } from "../types";
import { Modal } from "./Overlay";

export function PlanDialog({ plan, onClose, onDone }: { plan: Plan | null; onClose(): void; onDone?(): void }) {
  const { t, home, apply } = useApp();
  const [busy, setBusy] = useState(false);
  const go = async () => {
    if (!plan) return;
    setBusy(true);
    try { await apply(plan.id, ""); onDone?.(); } finally { setBusy(false); onClose(); }
  };
  return (
    <Modal open={!!plan} onClose={onClose} title={t("planTitle")}>
      {plan && (
        <div className="plan">
          <h2>{t("planTitle")}</h2>
          <p className="muted">{t("planBody")}</p>
          <ul className="plan-list">
            {plan.items.map((i) => (
              <li key={i.path}>
                <span className="plan-path" title={i.path}><b>{i.reason}</b><small>{tilde(i.path, home)}</small></span>
                <span className="mono">{human(i.size)}</span>
              </li>
            ))}
          </ul>
          {plan.skipped.length > 0 && (
            <div className="skipped">
              <h3>{t("skipped")}</h3>
              {plan.skipped.map(([p, why]) => <p key={p} title={why}><span className="mono">{tilde(p, home)}</span></p>)}
            </div>
          )}
          <footer className="plan-foot">
            <span className="muted">{t("trashHint")}</span>
            <button className="btn" onClick={onClose}>{t("cancel")}</button>
            <button className="btn primary" disabled={busy || plan.items.length === 0} onClick={go}>
              {t("moveToTrash")} · {human(plan.total)}
            </button>
          </footer>
        </div>
      )}
    </Modal>
  );
}
