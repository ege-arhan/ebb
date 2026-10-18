const U = ["B", "KB", "MB", "GB", "TB"];

/** Ondalık (1000'lik) birimler: macOS ve disk üreticileriyle aynı. */
export function human(bytes: number, digits = 1): string {
  let v = bytes, i = 0;
  while (v >= 1000 && i < U.length - 1) { v /= 1000; i++; }
  return i === 0 ? `${Math.round(v)} B` : `${v.toFixed(v >= 100 ? 0 : digits)} ${U[i]}`;
}

export function ago(ts: number, lang: string): string {
  const s = Math.max(0, Date.now() / 1000 - ts);
  const rtf = new Intl.RelativeTimeFormat(lang, { numeric: "auto" });
  if (s < 3600) return rtf.format(-Math.round(s / 60), "minute");
  if (s < 86400) return rtf.format(-Math.round(s / 3600), "hour");
  return rtf.format(-Math.round(s / 86400), "day");
}

export const baseName = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
export const tilde = (p: string, home: string) => (home && p.startsWith(home) ? "~" + p.slice(home.length) : p);
