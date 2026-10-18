import { Bot, Boxes, Plug } from "lucide-react";
import { motion } from "motion/react";
import type { Screen } from "../App";
import { PageHead, Skeleton } from "../components/Bits";
import { human, tilde } from "../format";
import type { Key } from "../i18n";
import { useApp } from "../state";

const TIP: Record<string, Key> = {
  codex: "tip_codex", "claude-code": "tip_claude", cursor: "tip_cursor", huggingface: "tip_huggingface",
  ollama: "tip_ollama", "lm-studio": "tip_lmstudio", "lm-studio-cache": "tip_lmstudio",
};

export function Agents({ go }: { go(s: Screen): void }) {
  const { t, ai, home } = useApp();
  const total = (ai ?? []).reduce((a, s) => a + s.size, 0);
  const max = ai?.[0]?.size || 1;
  return (
    <div className="page">
      <PageHead eyebrow={t("agents")} title={t("agentsTitle")} lead={t("agentsHint")}>
        <button className="btn primary" onClick={() => go("connect")}><Plug size={15} aria-hidden /> {t("connect")}</button>
      </PageHead>

      <section className="panel overview">
        <p className="total mono">{human(total)} <small>{t("totalOnDisk")}</small></p>
        <div className="stack" role="img" aria-label={human(total)}>
          {(ai ?? []).map((s, i) => (
            <motion.i key={s.id} className={s.kind} title={`${s.label} · ${human(s.size)}`} style={{ width: `${(s.size / (total || 1)) * 100}%`, opacity: 1 - i * 0.12 }}
              initial={{ scaleX: 0 }} animate={{ scaleX: 1 }} transition={{ delay: i * 0.06 }} />
          ))}
        </div>
        <p className="legend"><span><i className="agent" /> {t("agent")}</span><span><i className="model" /> {t("model")}</span></p>
      </section>

      {ai === null && <Skeleton rows={4} />}
      <ul className="stores rich">
        {(ai ?? []).map((s, i) => (
          <motion.li key={s.id} className={s.kind} initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: i * 0.05 }}>
            <span className="ico">{s.kind === "agent" ? <Bot size={18} aria-hidden /> : <Boxes size={18} aria-hidden />}</span>
            <span className="rowtext">
              <b>{s.label}</b>
              <small title={s.path}>{tilde(s.path, home)} · {s.files.toLocaleString()} {t("files")}</small>
              <em>{t(TIP[s.id] ?? "tip_default")}</em>
            </span>
            <span className="bar"><i style={{ width: `${Math.max(2, (s.size / max) * 100)}%` }} /></span>
            <span className="mono size">{human(s.size)}<small>{total ? Math.round((s.size / total) * 100) : 0}% {t("share")}</small></span>
          </motion.li>
        ))}
      </ul>
    </div>
  );
}
