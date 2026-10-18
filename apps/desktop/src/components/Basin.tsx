import { animate, useMotionValue, useReducedMotion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { human } from "../format";
import { useApp } from "../state";

/** Doluluk oranını yüksekliğe çevirir; üst %38 her zaman "hava" kalır ki sayı okunabilsin. */
const lvlY = (frac: number) => 0.38 + 0.62 * (1 - frac);

interface Ripple { x: number; t: number; amp: number }
interface Bubble { x: number; y: number; r: number; v: number; ph: number }
interface Drop { x: number; y: number; vx: number; vy: number; r: number }

/**
 * İmza öğe: su seviyesi = dolu alan; açık bant = güvenle çekilebilecek kısım.
 * Canlı: imleç dalga yaratır, tıklama sıçratır, baloncuklar yükselir, temizlikte su çekilirken damlalar sıçrar.
 */
export function Basin() {
  const { disk, reclaimable, t, diskAt } = useApp();
  const reduce = useReducedMotion();
  const canvas = useRef<HTMLCanvasElement>(null);
  const level = useMotionValue(0);
  const ghost = useMotionValue(0);
  const prevUsed = useRef<number | null>(null);
  const splash = useRef(0);
  const [shown, setShown] = useState({ free: 0, used: 0 });
  const [ago, setAgo] = useState(0);

  const usedFrac = disk ? 1 - disk.free / disk.total : 0;
  const ghostFrac = disk ? Math.min(usedFrac, reclaimable / disk.total) : 0;

  useEffect(() => {
    const id = setInterval(() => setAgo(Math.round((Date.now() - diskAt) / 1000)), 1000);
    return () => clearInterval(id);
  }, [diskAt]);

  useEffect(() => {
    if (!disk) return;
    if (prevUsed.current !== null && usedFrac < prevUsed.current - 0.001) splash.current += 1;
    prevUsed.current = usedFrac;
    const opts = reduce ? { duration: 0 } : { type: "spring" as const, stiffness: 38, damping: 14 };
    const a = animate(level, usedFrac, opts);
    const b = animate(ghost, ghostFrac, opts);
    const c = animate(shown.free, disk.free, {
      duration: reduce ? 0 : 1.1, ease: "easeOut",
      onUpdate: (v) => setShown({ free: v, used: disk.total - v }),
    });
    return () => { a.stop(); b.stop(); c.stop(); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [disk, usedFrac, ghostFrac, reduce]);

  useEffect(() => {
    const cv = canvas.current!;
    const ctx = cv.getContext("2d")!;
    let raf = 0, w = 0, h = 0, last = 0, visible = true, lastMove = 0, lastSplash = 0;
    const ripples: Ripple[] = [];
    const drops: Drop[] = [];
    const bubbles: Bubble[] = Array.from({ length: 26 }, () => ({ x: Math.random(), y: Math.random(), r: 1.2 + Math.random() * 3, v: 0.03 + Math.random() * 0.06, ph: Math.random() * 6.28 }));
    const fit = () => {
      const r = cv.getBoundingClientRect(), dpr = window.devicePixelRatio || 1;
      w = r.width; h = r.height;
      cv.width = w * dpr; cv.height = h * dpr; ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };
    const css = (n: string) => getComputedStyle(cv).getPropertyValue(n).trim();
    let colors = { hi: "", lo: "", ghost: "", line: "", foam: "" };
    const readColors = () => { colors = { hi: css("--tide-hi"), lo: css("--tide-lo"), ghost: css("--tide-ghost"), line: css("--tide-line"), foam: css("--foam") }; };

    const addRipple = (x: number, amp: number, now: number) => ripples.push({ x, t: now, amp });
    const burst = (x: number, y: number, n: number, power: number) => {
      for (let i = 0; i < n; i++) drops.push({ x, y, vx: (Math.random() - 0.5) * 120 * power, vy: -(80 + Math.random() * 160) * power, r: 1.5 + Math.random() * 2.5 });
    };

    const onMove = (e: PointerEvent) => {
      if (reduce) return;
      const r = cv.getBoundingClientRect(), now = performance.now() / 1000;
      if (now - lastMove > 0.06) { addRipple(e.clientX - r.left, 3.2, now); lastMove = now; }
    };
    const onDown = (e: PointerEvent) => {
      if (reduce) return;
      const r = cv.getBoundingClientRect(), now = performance.now() / 1000, x = e.clientX - r.left;
      addRipple(x, 11, now);
      burst(x, h * lvlY(level.get()), 9, 0.9);
    };

    const draw = (ms: number) => {
      raf = requestAnimationFrame(draw);
      if (!visible || document.hidden) { last = ms; return; }
      const now = ms / 1000, dt = Math.min(0.05, (ms - (last || ms)) / 1000);
      last = ms;
      const tt = reduce ? 0 : now;
      ctx.clearRect(0, 0, w, h);

      // temizlikte su çekilince sıçrama
      if (splash.current !== lastSplash && !reduce) {
        lastSplash = splash.current;
        for (let i = 0; i < 5; i++) {
          const x = w * (0.15 + 0.7 * Math.random());
          addRipple(x, 9 + Math.random() * 6, now + i * 0.12);
          burst(x, h * lvlY(level.get()), 7, 1.15);
        }
      }
      while (ripples.length && now - ripples[0].t > 3.2) ripples.shift();

      const y0 = h * lvlY(level.get());
      const yGhost = h * lvlY(level.get() - ghost.get());
      const surf = (x: number) => {
        let y = y0 + 7 * Math.sin((x / 220) * Math.PI * 2 + tt * 0.9) + 3 * Math.sin((x / 90) * Math.PI * 2 + tt * 1.9 + 3.1);
        for (const r of ripples) {
          const age = now - r.t;
          if (age < 0) continue;
          y += r.amp * Math.exp(-age * 1.5) * Math.sin(age * 8 - Math.abs(x - r.x) / 34) * Math.exp(-Math.abs(x - r.x) / 240);
        }
        return y;
      };

      // arka dalga
      ctx.beginPath(); ctx.moveTo(0, h);
      for (let x = 0; x <= w; x += 6) ctx.lineTo(x, y0 - 6 + 6 * Math.sin((x / 170) * Math.PI * 2 - tt * 0.6 + 1.1));
      ctx.lineTo(w, h); ctx.closePath(); ctx.fillStyle = colors.hi; ctx.globalAlpha = 0.28; ctx.fill(); ctx.globalAlpha = 1;

      // ana su gövdesi
      ctx.beginPath(); ctx.moveTo(0, h);
      for (let x = 0; x <= w; x += 4) ctx.lineTo(x, surf(x));
      ctx.lineTo(w, h); ctx.closePath();
      const g = ctx.createLinearGradient(0, y0, 0, h);
      g.addColorStop(0, colors.hi); g.addColorStop(1, colors.lo);
      ctx.fillStyle = g; ctx.fill();

      ctx.save(); ctx.clip();
      // ışık huzmeleri (caustics)
      ctx.globalCompositeOperation = "lighter";
      for (let k = 0; k < 4; k++) {
        const cx = ((k * 0.31 + tt * 0.012) % 1.2 - 0.1) * w;
        const grad = ctx.createLinearGradient(cx - 90, y0, cx + 90, h);
        grad.addColorStop(0, "rgba(160,255,240,0.0)"); grad.addColorStop(0.25, "rgba(160,255,240,0.07)"); grad.addColorStop(1, "rgba(160,255,240,0)");
        ctx.fillStyle = grad;
        ctx.beginPath(); ctx.moveTo(cx - 40, y0); ctx.lineTo(cx + 40, y0); ctx.lineTo(cx + 150 + Math.sin(tt * 0.5 + k) * 30, h); ctx.lineTo(cx - 20, h); ctx.closePath(); ctx.fill();
      }
      ctx.globalCompositeOperation = "source-over";
      // çekilebilir bant
      if (ghost.get() > 0.002) {
        ctx.beginPath(); ctx.rect(0, y0 - 20, w, Math.max(0, yGhost - y0) + 20); ctx.clip();
        ctx.beginPath(); ctx.moveTo(0, h);
        for (let x = 0; x <= w; x += 4) ctx.lineTo(x, surf(x) + 3);
        ctx.lineTo(w, h); ctx.closePath(); ctx.fillStyle = colors.ghost; ctx.fill();
      }
      ctx.restore();
      if (ghost.get() > 0.002) {
        ctx.setLineDash([2, 5]); ctx.strokeStyle = colors.line; ctx.lineWidth = 1.25;
        ctx.beginPath(); ctx.moveTo(0, yGhost); ctx.lineTo(w, yGhost); ctx.stroke(); ctx.setLineDash([]);
      }

      // baloncuklar
      if (!reduce) {
        ctx.save();
        ctx.strokeStyle = colors.foam; ctx.lineWidth = 1;
        for (const b of bubbles) {
          b.y -= b.v * dt * 4;
          const x = b.x * w + Math.sin(now * 1.3 + b.ph) * 6;
          const top = surf(x) / h;
          const yy = (b.y < 0 ? 1 : b.y);
          const py = yy * h;
          if (b.y < 0 || py < top * h + 4) {
            if (b.y >= 0 && Math.random() < 0.35) addRipple(x, 1.4, now);
            b.y = 1.02; b.x = Math.random();
            continue;
          }
          if (py < y0 - 2) continue;
          ctx.globalAlpha = 0.18 + 0.4 * ((py - y0) / Math.max(1, h - y0));
          ctx.beginPath(); ctx.arc(x, py, b.r, 0, 6.283); ctx.stroke();
        }
        ctx.restore();
      }

      // sıçrayan damlalar
      ctx.fillStyle = colors.foam;
      for (let i = drops.length - 1; i >= 0; i--) {
        const d = drops[i];
        d.vy += 520 * dt; d.x += d.vx * dt; d.y += d.vy * dt;
        if (d.vy > 0 && d.y > surf(d.x)) { drops.splice(i, 1); continue; }
        ctx.globalAlpha = 0.85; ctx.beginPath(); ctx.arc(d.x, d.y, d.r, 0, 6.283); ctx.fill();
      }
      ctx.globalAlpha = 1;

      // yüzey çizgisi
      ctx.beginPath();
      for (let x = 0; x <= w; x += 4) ctx[x === 0 ? "moveTo" : "lineTo"](x, surf(x));
      ctx.strokeStyle = colors.foam; ctx.lineWidth = 1.6; ctx.stroke();
    };

    fit(); readColors();
    const ro = new ResizeObserver(fit); ro.observe(cv);
    const io = new IntersectionObserver(([e]) => { visible = e.isIntersecting; }); io.observe(cv);
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const theme = new MutationObserver(readColors);
    theme.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    mq.addEventListener("change", readColors);
    cv.addEventListener("pointermove", onMove);
    cv.addEventListener("pointerdown", onDown);
    raf = requestAnimationFrame(draw);
    return () => {
      cancelAnimationFrame(raf); ro.disconnect(); io.disconnect(); theme.disconnect();
      mq.removeEventListener("change", readColors);
      cv.removeEventListener("pointermove", onMove); cv.removeEventListener("pointerdown", onDown);
    };
  }, [level, ghost, reduce]);

  const pct = disk ? Math.round(usedFrac * 100) : 0;
  return (
    <section className="basin" aria-label={t("tide")}>
      <canvas ref={canvas} className="basin-canvas" aria-hidden="true" />
      <p className="live" title={disk?.mount}><i aria-hidden /> {t("live")} · {ago < 5 ? t("justNow") : `${ago}s`}</p>
      <div className="basin-copy">
        <p className="eyebrow">{disk?.mount ?? "/"}</p>
        <p className="basin-free">
          <span className="num">{disk ? human(shown.free, 0).split(" ")[0] : "—"}</span>
          <span className="unit">{disk ? human(shown.free, 0).split(" ")[1] : ""} {t("free")}</span>
        </p>
        <p className="basin-meta">
          {disk ? `${human(disk.total - disk.free, 0)} ${t("used")} ${t("of")} ${human(disk.total, 0)} · ${pct}%` : " "}
        </p>
      </div>
      {reclaimable > 0 && (
        <p className="basin-chip" style={{ top: `calc(${lvlY(usedFrac - ghostFrac) * 100}% - 14px)` }}>
          <span>↓ {human(reclaimable)} {t("canEbb")}</span>
        </p>
      )}
    </section>
  );
}
