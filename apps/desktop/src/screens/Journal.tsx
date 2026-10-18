import { ChevronDown, RotateCcw } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { PageHead } from "../components/Bits";
import { ago, human, tilde } from "../format";
import type { Key } from "../i18n";
import { api } from "../ipc";
import { useApp } from "../state";
import type { EntryRow } from "../types";

export function Journal() {
  const { t, journal, undo, lang, home } = useApp();
  const [open, setOpen] = useState<string | null>(null);
  const [items, setItems] = useState<Record<string, EntryRow[]>>({});
  const moved = journal.filter((p) => p.status === "applied").reduce((a, p) => a + p.total, 0);
  const toggle = async (id: string) => {
    setOpen(open === id ? null : id);
    if (!items[id]) setItems({ ...items, [id]: await api.journalItems(id) });
  };
  return (
    <div className="page">
      <PageHead eyebrow={t("journal")} title={t("journal")} lead={t("journalLead")}>
        {journal.length > 0 && <p className="stat-inline"><b className="mono">{human(moved)}</b> <span className="muted">{t("movedTotal")}</span></p>}
      </PageHead>
      {!journal.length && <p className="empty-state">{t("journalEmpty")}</p>}
      <ul className="rows cards journal">
        {journal.map((p) => (
          <li key={p.id}>
            <div className="row-main jrow">
              <span className="rowtext"><b className="mono">{p.id}</b><small>{ago(p.created, lang)}</small></span>
              <span className={`chip ${p.status}`}>{t(p.status as Key)}</span>
              <span className="mono size">{human(p.total)}</span>
              {p.status === "applied" ? <button className="btn" onClick={() => undo(p.id)}><RotateCcw size={14} aria-hidden /> {t("undo")}</button> : <span />}
              <button className="icon-btn" aria-expanded={open === p.id} aria-label={t("details")} onClick={() => toggle(p.id)}><ChevronDown size={16} className={open === p.id ? "flip" : ""} /></button>
            </div>
            <AnimatePresence initial={false}>
              {open === p.id && (
                <motion.div className="row-more" initial={{ height: 0, opacity: 0 }} animate={{ height: "auto", opacity: 1 }} exit={{ height: 0, opacity: 0 }}>
                  <ul className="plan-list flat">
                    {(items[p.id] ?? []).map((e) => (
                      <li key={e.original}><span className="plan-path"><small title={e.original}>{tilde(e.original, home)}</small></span><span className="mono">{human(e.size)}</span></li>
                    ))}
                  </ul>
                </motion.div>
              )}
            </AnimatePresence>
          </li>
        ))}
      </ul>
    </div>
  );
}
