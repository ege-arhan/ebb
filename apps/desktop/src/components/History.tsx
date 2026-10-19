import { useEffect, useMemo, useState } from "react";
import { human } from "../format";
import { api } from "../ipc";
import { useApp } from "../state";
import type { DiskSample } from "../types";

/** Boş alanın zaman içindeki değişimi: hafif bir SVG alan grafiği. */
export function History() {
  const { t, lang, diskAt } = useApp();
  const [days, setDays] = useState(30);
  const [rows, setRows] = useState<DiskSample[] | null>(null);
  useEffect(() => { api.diskHistory(days).then(setRows).catch(() => setRows([])); }, [days, diskAt]);

  const W = 600, H = 120;
  const chart = useMemo(() => {
    if (!rows || rows.length < 2) return null;
    const t0 = rows[0].ts, t1 = rows[rows.length - 1].ts || t0 + 1;
    const lo = Math.min(...rows.map((r) => r.free)), hi = Math.max(...rows.map((r) => r.free));
    const pad = Math.max((hi - lo) * 0.15, 1e9);
    const x = (ts: number) => ((ts - t0) / Math.max(1, t1 - t0)) * W;
    const y = (f: number) => H - ((f - (lo - pad)) / (hi - lo + 2 * pad)) * H;
    const line = rows.map((r, i) => `${i ? "L" : "M"}${x(r.ts).toFixed(1)},${y(r.free).toFixed(1)}`).join(" ");
    return { line, area: `${line} L${W},${H} L0,${H} Z`, delta: rows[rows.length - 1].free - rows[0].free, since: rows[0].ts };
  }, [rows]);

  const date = (ts: number) => new Date(ts * 1000).toLocaleDateString(lang, { day: "numeric", month: "short" });
  return (
    <section className="panel history">
      <header>
        <h2>{t("historyTitle")}</h2>
        <div className="seg" role="group">
          {[7, 30].map((d) => <button key={d} aria-pressed={days === d} onClick={() => setDays(d)}>{d === 7 ? t("history7") : t("history30")}</button>)}
        </div>
      </header>
      {!chart && <p className="muted">{rows === null ? "…" : t("historyEmpty")}</p>}
      {chart && (
        <>
          <p className="hist-delta">
            <b className={`mono ${chart.delta >= 0 ? "up" : "down"}`}>{chart.delta >= 0 ? "+" : "−"}{human(Math.abs(chart.delta))}</b>
            <span className="muted"> {t("free")} · {t("historySince")} {date(chart.since)}</span>
          </p>
          <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" className="hist-svg" role="img" aria-label={t("historyTitle")}>
            <defs>
              <linearGradient id="hist-g" x1="0" x2="0" y1="0" y2="1">
                <stop offset="0" stopColor="var(--accent)" stopOpacity="0.45" />
                <stop offset="1" stopColor="var(--accent)" stopOpacity="0" />
              </linearGradient>
            </defs>
            <path d={chart.area} fill="url(#hist-g)" />
            <path d={chart.line} fill="none" stroke="var(--accent)" strokeWidth="2" vectorEffect="non-scaling-stroke" />
          </svg>
        </>
      )}
    </section>
  );
}
