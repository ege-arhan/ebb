import { AnimatePresence, motion } from "motion/react";
import { CornerDownLeft, Search } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useApp } from "../state";

/** Yerel <dialog>: odak tuzağı ve Esc tarayıcıdan gelir. */
export function Modal({ open, onClose, title, children }: { open: boolean; onClose(): void; title: string; children: ReactNode }) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current!;
    if (open && !d.open) d.showModal();
    if (!open && d.open) d.close();
  }, [open]);
  return (
    <dialog ref={ref} className="modal" onClose={onClose} onClick={(e) => e.target === ref.current && onClose()} aria-label={title}>
      {open && <div className="modal-body">{children}</div>}
    </dialog>
  );
}

export interface Cmd { id: string; label: string; hint?: string; run(): void }

export function Palette({ open, onClose, cmds }: { open: boolean; onClose(): void; cmds: Cmd[] }) {
  const { t } = useApp();
  const [q, setQ] = useState("");
  const [i, setI] = useState(0);
  const list = useMemo(() => cmds.filter((c) => c.label.toLowerCase().includes(q.toLowerCase())), [cmds, q]);
  useEffect(() => { if (open) { setQ(""); setI(0); } }, [open]);
  useEffect(() => setI(0), [q]);
  const go = (c?: Cmd) => { if (c) { onClose(); c.run(); } };
  return (
    <Modal open={open} onClose={onClose} title="Command palette">
      <div className="palette">
        <label className="palette-input">
          <Search size={16} aria-hidden />
          <input
            autoFocus value={q} placeholder={t("palettePlaceholder")} onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") { e.preventDefault(); setI((x) => Math.min(x + 1, list.length - 1)); }
              if (e.key === "ArrowUp") { e.preventDefault(); setI((x) => Math.max(x - 1, 0)); }
              if (e.key === "Enter") go(list[i]);
            }}
          />
        </label>
        <ul className="palette-list" role="listbox">
          {list.map((c, n) => (
            <li key={c.id} role="option" aria-selected={n === i} className={n === i ? "on" : ""} onMouseEnter={() => setI(n)} onClick={() => go(c)}>
              <span>{c.label}</span>
              {c.hint ? <kbd>{c.hint}</kbd> : n === i ? <CornerDownLeft size={14} aria-hidden /> : null}
            </li>
          ))}
          {!list.length && <li className="empty">{t("noResults")}</li>}
        </ul>
      </div>
    </Modal>
  );
}

export function Toasts() {
  const { toasts } = useApp();
  return (
    <div className="toasts" role="status" aria-live="polite">
      <AnimatePresence>
        {toasts.map((x) => (
          <motion.div key={x.id} className="toast" layout initial={{ opacity: 0, y: 14 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 6 }}>
            <span>{x.text}</span>
            {x.action && <button onClick={x.action.run}>{x.action.label}</button>}
          </motion.div>
        ))}
      </AnimatePresence>
    </div>
  );
}
