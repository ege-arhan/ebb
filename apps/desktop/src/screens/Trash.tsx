import { FolderOpen, Loader2, RotateCcw, ShieldAlert, Trash2 } from "lucide-react";
import { motion } from "motion/react";
import { useEffect, useState } from "react";
import { PageHead, Skeleton, useCountUp } from "../components/Bits";
import { Modal } from "../components/Overlay";
import { ago, human, tilde } from "../format";
import { api, inTauri } from "../ipc";
import { useApp } from "../state";
import type { TrashEntry } from "../types";

export function Trash() {
  const { t, home, lang, toast, refresh, celebrateWith } = useApp();
  const [items, setItems] = useState<TrashEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [confirm, setConfirm] = useState<"empty" | "purge" | null>(null);
  const [busy, setBusy] = useState(false);

  const load = () => api.trashList().then((l) => { setItems(l); setError(null); }).catch((e) => { setItems([]); setError(String(e)); });
  useEffect(() => { void load(); }, []);

  const total = (items ?? []).reduce((a, x) => a + x.size, 0);
  const shown = useCountUp(total, 0.6);
  const sel = (items ?? []).filter((x) => picked.has(x.id));
  const selSize = sel.reduce((a, x) => a + x.size, 0);
  const canRestore = sel.length > 0 && sel.every((x) => x.restorable);
  const toggle = (id: string) => setPicked((s) => { const n = new Set(s); if (n.has(id)) n.delete(id); else n.add(id); return n; });

  const after = async () => { setPicked(new Set()); await load(); void refresh(); };
  const restore = async (ids: string[]) => {
    setBusy(true);
    try { const n = await api.trashRestore(ids); toast(`${t("restoredN")} · ${n}`); } catch (e) { toast(String(e)); }
    setBusy(false); await after();
  };
  const destroy = async () => {
    const kind = confirm; setConfirm(null); setBusy(true);
    try {
      const bytes = kind === "empty" ? await api.trashEmpty() : await api.trashPurge(sel.map((x) => x.id));
      if (bytes > 0) celebrateWith(bytes, "freed");
    } catch (e) { toast(String(e)); }
    setBusy(false); await after();
  };

  const n = confirm === "empty" ? items?.length ?? 0 : sel.length;
  const size = human(confirm === "empty" ? total : selSize);

  return (
    <div className="page">
      <PageHead eyebrow={t("trash")} title={t("trash")} lead={t("trashLead")}>
        <button className="btn danger big" disabled={!items?.length || busy} onClick={() => setConfirm("empty")}><Trash2 size={16} aria-hidden /> {t("trashEmptyBtn")}</button>
      </PageHead>

      <div className="trash-sum">
        <span className="trash-ico"><Trash2 size={22} aria-hidden /></span>
        <p><b className="mono">{human(shown)}</b> <span className="muted">{t("inTrash")} · {items?.length ?? 0}</span></p>
      </div>

      {error && (
        <aside className="note">
          <ShieldAlert size={18} aria-hidden />
          <span className="rowtext"><b>{error}</b>{inTauri && <small><button className="btn" onClick={() => api.openPrivacySettings()}>{t("needFda")}</button></small>}</span>
        </aside>
      )}
      {items === null && <Skeleton rows={5} />}
      {items?.length === 0 && !error && <p className="empty-state">{t("trashIsEmpty")}</p>}

      <ul className="rows cards trashlist">
        {(items ?? []).map((x, i) => (
          <motion.li key={x.id} initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: Math.min(i * 0.02, 0.2) }}>
            <div className="row-main trashrow">
              <label>
                <input type="checkbox" checked={picked.has(x.id)} onChange={() => toggle(x.id)} />
                <span className="rowtext">
                  <b>{x.name} {x.by_ebb && <span className="chip applied">{t("fromEbb")}</span>}</b>
                  <small title={x.original ?? ""}>{x.original ? tilde(x.original, home) : t("otherApp")}{x.deleted_at ? ` · ${ago(x.deleted_at, lang)}` : ""}</small>
                </span>
              </label>
              <span className="mono size">{human(x.size)}</span>
              {x.restorable
                ? <button className="btn ghost" disabled={busy} onClick={() => restore([x.id])}><RotateCcw size={14} aria-hidden /> {t("restore")}</button>
                : <button className="btn ghost" title={t("notRestorable")} onClick={() => api.reveal(x.id)}><FolderOpen size={14} aria-hidden /> {t("reveal")}</button>}
            </div>
          </motion.li>
        ))}
      </ul>

      <div className={`dock${picked.size ? " show" : ""}`}>
        <span><b className="mono big">{human(selSize)}</b> <span className="muted">· {picked.size} {t("selected")}</span></span>
        <button className="btn" disabled={!canRestore || busy} title={canRestore ? "" : t("notRestorable")} onClick={() => restore(sel.map((x) => x.id))}><RotateCcw size={14} aria-hidden /> {t("restore")}</button>
        <button className="btn danger" disabled={busy} onClick={() => setConfirm("purge")}>{busy ? <Loader2 size={14} className="spin" aria-hidden /> : <Trash2 size={14} aria-hidden />} {t("deleteForever")}</button>
      </div>

      <Modal open={confirm !== null} onClose={() => setConfirm(null)} title={t("trashEmptyTitle")}>
        {confirm && (
          <div className="plan">
            <h2><ShieldAlert size={18} aria-hidden /> {confirm === "empty" ? t("trashEmptyTitle") : t("purgeTitle").replace("{n}", String(n))}</h2>
            <p className="muted">{(confirm === "empty" ? t("trashEmptyBody") : t("purgeBody")).replace("{n}", String(n)).replace("{size}", size)}</p>
            <footer className="plan-foot">
              <span />
              <button className="btn" onClick={() => setConfirm(null)}>{t("cancel")}</button>
              <button className="btn danger-solid" onClick={destroy}><Trash2 size={14} aria-hidden /> {confirm === "empty" ? t("trashEmptyBtn") : t("deleteForever")}</button>
            </footer>
          </div>
        )}
      </Modal>
    </div>
  );
}
