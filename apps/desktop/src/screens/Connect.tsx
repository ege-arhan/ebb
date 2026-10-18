import { Bot, Check, CircleAlert, Cpu, FileCode2, Loader2, MessageSquare, Plug, ShieldCheck, ShieldX, Unplug } from "lucide-react";
import { motion } from "motion/react";
import { useEffect, useState } from "react";
import { PageHead, Snippet } from "../components/Bits";
import { api } from "../ipc";
import type { Key } from "../i18n";
import { useApp } from "../state";
import type { ClientInfo } from "../types";

const KIND_ICON = { agent: Bot, chat: MessageSquare, editor: FileCode2, local: Cpu } as const;
const ORDER: ClientInfo["kind"][] = ["agent", "chat", "local", "editor"];
type Test = { s: "idle" | "run" | "ok" | "fail"; n?: number; err?: string };

export function Connect() {
  const { t, clients, refreshClients, toast } = useApp();
  const [busy, setBusy] = useState<string | null>(null);
  const [done, setDone] = useState<Record<string, string>>({});
  const [test, setTest] = useState<Test>({ s: "idle" });
  const [cmd, setCmd] = useState("ebb");
  useEffect(() => { api.mcpCommand().then(setCmd); void refreshClients(); }, [refreshClients]);

  const runTest = async () => {
    setTest({ s: "run" });
    try { setTest({ s: "ok", n: await api.connectTest() }); } catch (e) { setTest({ s: "fail", err: String(e) }); }
  };
  const connectIds = async (ids: string[]) => {
    for (const id of ids) {
      setBusy(id);
      try {
        const o = await api.connectClient(id);
        setDone((d) => ({ ...d, [id]: o.note }));
      } catch (e) { toast(`${id}: ${String(e)}`); }
    }
    setBusy(null);
    await refreshClients();
    void runTest();
  };
  const disconnect = async (id: string) => {
    setBusy(id);
    try { await api.disconnectClient(id); setDone((d) => { const n = { ...d }; delete n[id]; return n; }); } catch (e) { toast(String(e)); }
    setBusy(null);
    await refreshClients();
  };

  const pending = clients.filter((c) => c.installed && !c.connected);
  const connectedN = clients.filter((c) => c.connected).length;
  const q = (s: string) => navigator.clipboard?.writeText(s).then(() => toast(`${t("copied")}: ${s}`)).catch(() => {});
  const json = JSON.stringify({ mcpServers: { ebb: { command: cmd, args: ["mcp"] } } }, null, 2);

  return (
    <div className="page">
      <PageHead eyebrow={t("connect")} title={t("connectTitle")} lead={t("connectLead")}>
        <button className="btn primary big" disabled={!pending.length || busy !== null} onClick={() => connectIds(pending.map((c) => c.id))}>
          {busy ? <Loader2 size={16} className="spin" aria-hidden /> : <Plug size={16} aria-hidden />} {t("connectAll")}{pending.length ? ` (${pending.length})` : ""}
        </button>
      </PageHead>

      <ol className="steps">
        {([["step1", "step1b"], ["step2", "step2b"], ["step3", "step3b"]] as [Key, Key][]).map(([a, b], i) => (
          <li key={a}><span className="step-n">{i + 1}</span><b>{t(a)}</b><small>{t(b)}</small></li>
        ))}
      </ol>

      {ORDER.map((kind) => {
        const rows = clients.filter((c) => c.kind === kind);
        if (!rows.length) return null;
        const Icon = KIND_ICON[kind];
        return (
          <section key={kind} className="group">
            <h2 className="group-title"><Icon size={16} aria-hidden /> {t(`kind_${kind}` as Key)}</h2>
            <ul className="clients">
              {rows.map((c) => {
                const working = busy === c.id;
                return (
                  <motion.li key={c.id} layout className={`client ${c.connected ? "on" : ""} ${c.installed ? "" : "off"}`}>
                    <span className="client-ico"><Icon size={18} aria-hidden /></span>
                    <span className="rowtext">
                      <b>{c.name}</b>
                      <small className="mono">{c.installed ? (c.config ?? "") : t("notFound")}</small>
                      {done[c.id] && <small className="okline"><Check size={13} aria-hidden /> {done[c.id]}</small>}
                    </span>
                    <span className={`chip ${c.connected ? "applied" : c.installed ? "" : "failed-soft"}`}>
                      {c.connected ? t("connected") : c.installed ? t("ready") : t("notFound")}
                    </span>
                    {c.connected ? (
                      <button className="btn ghost" disabled={working} onClick={() => disconnect(c.id)}><Unplug size={14} aria-hidden /> {t("disconnect")}</button>
                    ) : !c.installed ? (
                      <span />
                    ) : (
                      <button className="btn primary" disabled={busy !== null} onClick={() => connectIds([c.id])}>
                        {working ? <Loader2 size={14} className="spin" aria-hidden /> : <Plug size={14} aria-hidden />} {working ? t("connecting") : t("connectOne")}
                      </button>
                    )}
                  </motion.li>
                );
              })}
            </ul>
          </section>
        );
      })}

      {kindNote(t("localTitle"), t("localBody"))}

      <div className="two">
        <section className="panel">
          <header>
            <h2>{t("testConn")}</h2>
            <button className="btn" onClick={runTest} disabled={test.s === "run"}>{test.s === "run" ? <Loader2 size={14} className="spin" aria-hidden /> : <Check size={14} aria-hidden />} {test.s === "run" ? t("testing") : t("testConn")}</button>
          </header>
          {test.s === "ok" && <p className="okline big"><Check size={16} aria-hidden /> {t("testOk").replace("{n}", String(test.n))}</p>}
          {test.s === "fail" && <p className="warn"><CircleAlert size={15} aria-hidden /> {t("testFail")}: {test.err}</p>}
          {test.s === "idle" && <p className="muted">{connectedN} / {clients.filter((c) => c.installed).length} {t("connected").toLowerCase()}</p>}
          <h3 className="sub">{t("tryAsking")}</h3>
          <ul className="prompts">
            {[t("prompt1"), t("prompt2"), t("prompt3")].map((p) => <li key={p}><button onClick={() => q(p)}>“{p}”</button></li>)}
          </ul>
        </section>
        <section className="panel can">
          <h3 className="sub"><ShieldCheck size={15} aria-hidden /> {t("canDo")}</h3>
          <ul>{(["can1", "can2", "can3"] as Key[]).map((k) => <li key={k} className="yes"><Check size={14} aria-hidden /> {t(k)}</li>)}</ul>
          <h3 className="sub"><ShieldX size={15} aria-hidden /> {t("cannotDo")}</h3>
          <ul>{(["cannot1", "cannot2", "cannot3"] as Key[]).map((k) => <li key={k} className="no"><span aria-hidden>✕</span> {t(k)}</li>)}</ul>
        </section>
      </div>

      <details className="manual">
        <summary>{t("manual")}</summary>
        <p className="muted">{t("manualBody")}</p>
        <Snippet label="Claude Code" text={`claude mcp add ebb -- ${cmd.includes(" ") ? `"${cmd}"` : cmd} mcp`} />
        <Snippet label="JSON (mcpServers)" text={json} />
      </details>
    </div>
  );
}

function kindNote(title: string, body: string) {
  return (
    <aside className="note">
      <Cpu size={18} aria-hidden />
      <span className="rowtext"><b>{title}</b><small>{body}</small></span>
    </aside>
  );
}
