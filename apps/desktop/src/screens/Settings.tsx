import pkg from "../../package.json";
import { PageHead } from "../components/Bits";
import { useApp } from "../state";

const isMac = navigator.platform.toLowerCase().includes("mac");
const mod = isMac ? "⌘" : "Ctrl";

export function Settings() {
  const { t, lang, setLang, theme, setTheme } = useApp();
  return (
    <div className="page narrow">
      <PageHead eyebrow={t("settings")} title={t("settings")} lead={t("settingsLead")} />
      <section className="panel">
        <h2>{t("appearance")}</h2>
        <div className="field"><span>{t("theme")}</span>
          <div className="seg" role="group">{(["system", "light", "dark"] as const).map((v) => (
            <button key={v} aria-pressed={theme === v} onClick={() => setTheme(v)}>{t(v)}</button>))}</div></div>
        <div className="field"><span>{t("language")}</span>
          <div className="seg" role="group">{(["en", "tr"] as const).map((v) => (
            <button key={v} aria-pressed={lang === v} onClick={() => setLang(v)}>{v === "en" ? "English" : "Türkçe"}</button>))}</div></div>
      </section>
      <section className="panel">
        <h2>{t("shortcuts")}</h2>
        <dl className="keys">
          <dt>{t("palette")}</dt><dd><kbd>{mod}</kbd> <kbd>K</kbd></dd>
          <dt>{t("goTo")}</dt><dd><kbd>{mod}</kbd> <kbd>1</kbd>–<kbd>7</kbd></dd>
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
