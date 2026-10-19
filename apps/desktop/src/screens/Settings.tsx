import pkg from "../../package.json";
import { useEffect, useState } from "react";
import { PageHead } from "../components/Bits";
import type { Key } from "../i18n";
import { api } from "../ipc";
import type { TrayConfig } from "../types";
import { LANGS } from "../i18n";
import { useApp } from "../state";

const isMac = navigator.platform.toLowerCase().includes("mac");
const mod = isMac ? "⌘" : "Ctrl";

const TRAY_IDS = ["free", "safe", "memory", "trash", "health"] as const;

function Tray() {
  const { t } = useApp();
  const [cfg, setCfg] = useState<TrayConfig | null>(null);
  useEffect(() => { api.trayGet().then(setCfg).catch(() => {}); }, []);
  if (!cfg) return null;
  const save = (c: TrayConfig) => { setCfg(c); void api.traySet(c); };
  const lbl = (id: string) => t(`tr_${id}` as Key);
  return (
    <section className="panel">
      <h2>{t("trayTitle")}</h2>
      <p className="muted">{t("trayLead")}</p>
      <div className="field"><span>{t("trayShown")}</span>
        <select className="select" value={cfg.title} onChange={(e) => save({ ...cfg, title: e.target.value })}>
          {TRAY_IDS.map((i) => <option key={i} value={i}>{lbl(i)}</option>)}
          <option value="none">{t("trayNone")}</option>
        </select></div>
      <div className="field top"><span>{t("trayMenu")}</span>
        <div className="checks">{TRAY_IDS.map((i) => (
          <label key={i} className="check"><input type="checkbox" checked={cfg.lines.includes(i)} onChange={(e) => save({ ...cfg, lines: e.target.checked ? [...cfg.lines, i] : cfg.lines.filter((x) => x !== i) })} /> {lbl(i)}</label>
        ))}</div></div>
      <p className="muted small">{t("trayNote")}</p>
    </section>
  );
}

export function Settings() {
  const { t, lang, setLang, theme, setTheme, checkUpdates, setCheckUpdates } = useApp();
  return (
    <div className="page narrow">
      <PageHead eyebrow={t("settings")} title={t("settings")} lead={t("settingsLead")} />
      <section className="panel">
        <h2>{t("appearance")}</h2>
        <div className="field"><span>{t("theme")}</span>
          <div className="seg" role="group">{(["system", "light", "dark"] as const).map((v) => (
            <button key={v} aria-pressed={theme === v} onClick={() => setTheme(v)}>{t(v)}</button>))}</div></div>
        <div className="field"><span>{t("language")}</span>
          <div className="seg wrap" role="group">{LANGS.map((v) => (
            <button key={v.id} aria-pressed={lang === v.id} onClick={() => setLang(v.id)}>{v.label}</button>))}</div></div>
      </section>
      <Tray />
      <section className="panel">
        <div className="field">
          <span className="rowtext"><b>{t("updatesSetting")}</b><small className="wrapsmall">{t("updatesHint")}</small></span>
          <button role="switch" aria-checked={checkUpdates} aria-label={t("updatesSetting")} className="switch" onClick={() => setCheckUpdates(!checkUpdates)}><i /></button>
        </div>
      </section>
      <section className="panel">
        <h2>{t("shortcuts")}</h2>
        <dl className="keys">
          <dt>{t("palette")}</dt><dd><kbd>{mod}</kbd> <kbd>K</kbd></dd>
          <dt>{t("goTo")}</dt><dd><kbd>{mod}</kbd> <kbd>1</kbd>–<kbd>9</kbd></dd>
        </dl>
      </section>
      <section className="panel">
        <h2>{t("about")}</h2>
        <dl className="keys">
          <dt>{t("version")}</dt><dd className="mono">{pkg.version}</dd>
          <dt>{t("license")}</dt><dd>Apache-2.0</dd>
        </dl>
        <p className="muted">{t("privacy")}</p>
      </section>
    </div>
  );
}
