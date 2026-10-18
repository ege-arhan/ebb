import { animate, useReducedMotion } from "motion/react";
import { AppWindow, BrainCircuit, Check, Code, Copy, Globe, HardDrive } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { human } from "../format";
import { useApp } from "../state";

export function PageHead({ eyebrow, title, lead, children }: { eyebrow?: string; title: string; lead?: string; children?: ReactNode }) {
  return (
    <header className="pagehead">
      <div>
        {eyebrow && <p className="eyebrow">{eyebrow}</p>}
        <h1>{title}</h1>
        {lead && <p className="lead">{lead}</p>}
      </div>
      {children && <div className="pagehead-actions">{children}</div>}
    </header>
  );
}

export function Stat({ label, value, bytes, hint, tone = "plain", onClick }: { label: string; value?: string; bytes?: number; hint?: string; tone?: "safe" | "review" | "ai" | "plain"; onClick?: () => void }) {
  const Tag = onClick ? "button" : "div";
  const counted = useCountUp(bytes ?? 0, 0.9);
  return (
    <Tag className={`stat ${tone}${onClick ? " click" : ""}`} onClick={onClick}>
      <span className="eyebrow">{label}</span>
      <b className="stat-value">{bytes !== undefined ? human(counted) : value}</b>
      {hint && <small>{hint}</small>}
    </Tag>
  );
}

export function Skeleton({ rows = 4 }: { rows?: number }) {
  return (
    <div className="skel" aria-hidden>
      {Array.from({ length: rows }, (_, i) => <span key={i} style={{ width: `${88 - i * 11}%` }} />)}
    </div>
  );
}

const ICONS = { browser: Globe, dev: Code, app: AppWindow, system: HardDrive, ai: BrainCircuit } as const;
export function CatIcon({ category, size = 16 }: { category: string; size?: number }) {
  const I = ICONS[category as keyof typeof ICONS] ?? HardDrive;
  return <I size={size} aria-hidden />;
}

export function useCountUp(value: number, seconds = 0.7) {
  const reduce = useReducedMotion();
  const [x, setX] = useState(value);
  const from = useRef(value);
  useEffect(() => {
    const c = animate(from.current, value, { duration: reduce ? 0 : seconds, ease: "easeOut", onUpdate: (v) => { from.current = v; setX(v); } });
    return () => c.stop();
  }, [value, seconds, reduce]);
  return x;
}

export function Snippet({ label, text }: { label: string; text: string }) {
  const { t } = useApp();
  const [ok, setOk] = useState(false);
  const copy = async () => {
    try { await navigator.clipboard.writeText(text); setOk(true); setTimeout(() => setOk(false), 1500); } catch { /* pano izni yok */ }
  };
  return (
    <div className="snippet">
      <header><span>{label}</span><button className="btn ghost" onClick={copy}>{ok ? <Check size={14} aria-hidden /> : <Copy size={14} aria-hidden />} {ok ? t("copied") : t("copy")}</button></header>
      <pre>{text}</pre>
    </div>
  );
}
