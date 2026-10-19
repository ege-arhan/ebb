import { Bot, Loader2, SendHorizontal, Wrench } from "lucide-react";
import { motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { PageHead } from "../components/Bits";
import { PlanDialog } from "../components/PlanDialog";
import { api } from "../ipc";
import { useApp } from "../state";
import type { AiToolCall, ChatMessage, Plan, Provider } from "../types";

interface Turn { role: "user" | "assistant"; text: string; tools?: AiToolCall[]; planId?: string | null }

export function Assistant() {
  const { t, toast } = useApp();
  const [providers, setProviders] = useState<Provider[] | null>(null);
  const [model, setModel] = useState("");
  const [turns, setTurns] = useState<Turn[]>([]);
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [plan, setPlan] = useState<Plan | null>(null);
  const end = useRef<HTMLDivElement>(null);

  useEffect(() => { api.aiProviders().then((p) => { const on = p.filter((x) => x.online && x.models.length); setProviders(on); if (on[0]) setModel(`${on[0].id}|${on[0].models[0]}`); }).catch(() => setProviders([])); }, []);
  useEffect(() => { end.current?.scrollIntoView({ behavior: "smooth" }); }, [turns, busy]);

  const send = async (msg: string) => {
    const [pid, ...rest] = model.split("|");
    const provider = providers?.find((p) => p.id === pid);
    if (!provider || !msg.trim() || busy) return;
    const history: ChatMessage[] = turns.map((x) => ({ role: x.role, content: x.text }));
    setTurns((x) => [...x, { role: "user", text: msg }]); setText(""); setBusy(true);
    try {
      const r = await api.aiChat(provider, rest.join("|"), history, msg);
      setTurns((x) => [...x, { role: "assistant", text: r.text, tools: r.tool_calls, planId: r.plan_id }]);
    } catch (e) { toast(String(e)); }
    setBusy(false);
  };

  if (providers && providers.length === 0) {
    return (
      <div className="page narrow">
        <PageHead eyebrow={t("assistant")} title={t("assistantTitle")} lead={t("assistantLead")} />
        <aside className="note"><Bot size={18} aria-hidden /><span className="rowtext"><b>{t("noModel")}</b><small>{t("noModelBody")}</small></span></aside>
      </div>
    );
  }
  return (
    <div className="page narrow chatpage">
      <PageHead eyebrow={t("assistant")} title={t("assistantTitle")} lead={t("assistantLead")}>
        {providers && providers.length > 0 && (
          <label className="modelpick">{t("modelLabel")}
            <select value={model} onChange={(e) => setModel(e.target.value)}>
              {providers.flatMap((p) => p.models.map((m) => <option key={p.id + m} value={`${p.id}|${m}`}>{p.name} · {m}</option>))}
            </select>
          </label>
        )}
      </PageHead>
      <div className="thread">
        {turns.length === 0 && (
          <div className="suggest">{[t("suggest1"), t("suggest2"), t("suggest3")].map((s) => <button key={s} className="chipbtn big" onClick={() => send(s)}>{s}</button>)}</div>
        )}
        {turns.map((x, i) => (
          <motion.div key={i} className={`bubble ${x.role}`} initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }}>
            <p>{x.text}</p>
            {x.tools && x.tools.length > 0 && <p className="toolchips"><Wrench size={12} aria-hidden /> {t("usedTools")}: {x.tools.map((c) => <code key={c.name + c.result_preview}>{c.name}</code>)}</p>}
            {x.planId && <button className="btn primary" onClick={async () => setPlan(await api.planGet(x.planId!))}>{t("reviewPlan")}</button>}
          </motion.div>
        ))}
        {busy && <div className="bubble assistant"><Loader2 size={16} className="spin" aria-hidden /> {t("thinking")}…</div>}
        <div ref={end} />
      </div>
      <form className="composer" onSubmit={(e) => { e.preventDefault(); void send(text); }}>
        <input value={text} onChange={(e) => setText(e.target.value)} placeholder={t("askPlaceholder")} aria-label={t("askPlaceholder")} />
        <button className="btn primary" disabled={busy || !text.trim()}><SendHorizontal size={15} aria-hidden /> {t("send")}</button>
      </form>
      <PlanDialog plan={plan} onClose={() => setPlan(null)} />
    </div>
  );
}
