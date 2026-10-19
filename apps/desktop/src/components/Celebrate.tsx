import { animate, AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useState } from "react";
import { human } from "../format";
import { useApp } from "../state";

/** Temizlik bittiğinde: çekilen alan sayılarak yükselir, halkalar yayılır, damlalar sıçrar. */
export function Celebrate() {
  const { celebrate, celebrateKind, clearCelebrate, t } = useApp();
  const word = celebrateKind === "optimize" ? t("won") : celebrateKind === "compress" ? t("savedWord") : t("ebbed");
  const sub = celebrateKind === "optimize" ? t("wonSub") : celebrateKind === "compress" ? t("compressSub") : t("celebrateSub");
  const [x, setX] = useState(0);
  const drops = useMemo(
    () => Array.from({ length: 22 }, (_, i) => ({ i, dx: (Math.random() - 0.5) * 520, dy: -(120 + Math.random() * 260), d: Math.random() * 0.25, s: 4 + Math.random() * 7 })),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [celebrate],
  );
  useEffect(() => {
    if (!celebrate) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) setX(celebrate);
    const c = animate(0, celebrate, { duration: 1.5, ease: [0.16, 1, 0.3, 1], onUpdate: setX });
    const id = setTimeout(clearCelebrate, 4600);
    return () => { c.stop(); clearTimeout(id); };
  }, [celebrate, clearCelebrate]);
  return (
    <AnimatePresence>
      {celebrate > 0 && (
        <motion.div className="celebrate" role="status" onClick={clearCelebrate} initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0, transition: { duration: 0.35 } }}>
          <span className="ring r1" /><span className="ring r2" /><span className="ring r3" />
          {drops.map((d) => (
            <i key={d.i} className="drop" style={{ ["--dx" as string]: `${d.dx}px`, ["--dy" as string]: `${d.dy}px`, animationDelay: `${0.15 + d.d}s`, width: d.s, height: d.s }} />
          ))}
          <motion.div className="cel-body" initial={{ scale: 0.8, y: 20 }} animate={{ scale: 1, y: 0 }} transition={{ type: "spring", stiffness: 160, damping: 16 }}>
            <p className="cel-num">{human(x)}</p>
            <p className="cel-word">{word}</p>
            <small>{sub}</small>
            <small className="cel-tap">{t("tapClose")}</small>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
