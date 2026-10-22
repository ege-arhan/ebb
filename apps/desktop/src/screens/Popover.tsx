import { Bot, Gauge, HardDrive, Layers, Power, Sparkles, Trash2, Waves } from "lucide-react";
import { motion } from "motion/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { human } from "../format";
import { api } from "../ipc";
import { useApp } from "../state";
import type { PopoverStats } from "../types";

/** Menü çubuğu / tepsi açılır penceresi: hızlı bakış + tek dokunuşla eylemler. Hafif: ağır tarama yok. */
export function Popover() {
  const { t } = useApp();
  const [s, setS] = useState<PopoverStats | null>(null);
  const cv = useRef<HTMLCanvasElement>(null);
  const level = useRef(0);
  const target = useRef(0);

  const load = useCallback(() => { api.popoverStats().then((x) => { setS(x); if (x.disk) target.current = 1 - x.disk.free / x.disk.total; }).catch(() => {}); }, []);
  useEffect(() => {
    load();
    let un: (() => void) | undefined;
    // Pencere her açıldığında tazele.
    void import("@tauri-apps/api/event").then(({ listen }) => listen("ebb://popover-open", load).then((u) => { un = u; })).catch(() => {});
    const onVis = () => !document.hidden && load();
    document.addEventListener("visibilitychange", onVis);
    const id = setInterval(() => !document.hidden && load(), 15000);
    return () => { un?.(); document.removeEventListener("visibilitychange", onVis); clearInterval(id); };
  }, [load]);
  useEffect(() => { const k = (e: KeyboardEvent) => { if (e.key === "Escape") void api.openMain().then(() => {}); }; window.addEventListener("keydown", k); return () => window.removeEventListener("keydown", k); }, []);

  // Küçük dalga göstergesi.
  useEffect(() => {
    const c = cv.current!, ctx = c.getContext("2d")!;
    let raf = 0, w = 0, h = 0;
    const fit = () => { const r = c.getBoundingClientRect(), d = window.devicePixelRatio || 1; w = r.width; h = r.height; c.width = w * d; c.height = h * d; ctx.setTransform(d, 0, 0, d, 0, 0); };
    fit();
    const col = (n: string) => getComputedStyle(c).getPropertyValue(n).trim();
    const draw = (ms: number) => {
      raf = requestAnimationFrame(draw);
      if (document.hidden) return;
      level.current += (target.current - level.current) * 0.06;
      const t0 = ms / 1000, y0 = h * (0.56 + 0.44 * (1 - level.current));
      ctx.clearRect(0, 0, w, h);
      const wave = (a: number, l: number, sp: number, ph: number) => (x: number) => a * Math.sin((x / l) * 6.283 + t0 * sp + ph);
      const w1 = wave(5, 120, 1.1, 0), w2 = wave(3, 70, -1.6, 1.3);
      ctx.beginPath(); ctx.moveTo(0, h);
      for (let x = 0; x <= w; x += 4) ctx.lineTo(x, y0 + w1(x) + w2(x));
      ctx.lineTo(w, h); ctx.closePath();
      const g = ctx.createLinearGradient(0, y0, 0, h); g.addColorStop(0, col("--tide-hi")); g.addColorStop(1, col("--tide-lo"));
      ctx.fillStyle = g; ctx.fill();
      ctx.beginPath();
      for (let x = 0; x <= w; x += 4) ctx[x === 0 ? "moveTo" : "lineTo"](x, y0 + w1(x) + w2(x));
      ctx.strokeStyle = col("--foam"); ctx.lineWidth = 1.5; ctx.stroke();
    };
    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, []);

  const d = s?.disk;
  const free = d ? human(d.free, 0).split(" ") : ["—", ""];
  const go = (screen: string) => void api.openMain(screen);
  const act = (icon: typeof Gauge, label: string, screen: string, sub?: string) => {
    const Icon = icon;
    return (
      <button key={screen} className="pop-act" onClick={() => go(screen)}>
        <span className="pop-ico"><Icon size={17} aria-hidden /></span>
        <span className="rowtext"><b>{label}</b>{sub && <small>{sub}</small>}</span>
      </button>
    );
  };
  const memPct = s ? Math.round(s.memory.pressure * 100) : 0;

  return (
    <motion.div className="pop" initial={{ opacity: 0, y: -8, scale: 0.97 }} animate={{ opacity: 1, y: 0, scale: 1 }} transition={{ duration: 0.18 }}>
      <section className="pop-hero">
        <canvas ref={cv} aria-hidden />
        <div className="pop-hero-copy">
          <p className="eyebrow"><Waves size={12} aria-hidden /> Ebb · {d?.mount ?? "/"}</p>
          <p className="pop-free"><b>{free[0]}</b> <span>{free[1]} {t("pop_free")}</span></p>
          <p className="pop-meta">{d ? `${human(d.total - d.free, 0)} / ${human(d.total, 0)}` : " "}</p>
        </div>
      </section>

      <div className="pop-chips">
        <button onClick={() => go("clean")}><span>{t("pop_safe")}</span><b>{s?.safe != null ? human(s.safe) : "…"}</b></button>
        <button onClick={() => go("trash")}><span>{t("pop_trash")}</span><b>{s?.trash != null ? human(s.trash) : "…"}</b></button>
        <button onClick={() => go("speed")}><span>{t("pop_mem")}</span><b>{s ? `${memPct}%` : "…"}</b><i className={memPct > 80 ? "hot" : memPct > 65 ? "warm" : ""} style={{ width: `${memPct}%` }} /></button>
      </div>

      <button className="pop-cta" onClick={() => go("smart")} disabled={!s}>
        <Sparkles size={18} aria-hidden />
        <span className="rowtext"><b>{s?.safe ? t("pop_clean").replace("{size}", human(s.safe)) : t("pop_smart")}</b><small>{t("pop_smart")}</small></span>
      </button>

      <div className="pop-grid">
        {act(Layers, t("pop_optimize"), "optimize")}
        {act(Gauge, t("pop_speed"), "speed")}
        {act(HardDrive, t("pop_apps"), "apps")}
        {act(Bot, t("pop_ask"), "assistant")}
        {act(Trash2, t("pop_trash_open"), "trash")}
        {act(Waves, t("pop_open"), "tide")}
      </div>

      <footer className="pop-foot">
        <button onClick={() => void api.quitEbb()}><Power size={13} aria-hidden /> {t("pop_quit")}</button>
        <span>v{__APP_VERSION__}</span>
      </footer>
    </motion.div>
  );
}
