// Tarayıcıda (Tauri dışı) geliştirme ve ekran görüntüsü doğrulaması için sahte IPC.
import type { Api, AiStore, ClientInfo, Finding, Node, Plan, PlanSummary, Report } from "./types";

const HOME = "/Users/demo";
const GB = 1e9, MB = 1e6;
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

const dir = (name: string, parent: string, children: Node[]): Node => {
  const path = `${parent}/${name}`;
  const size = children.reduce((a, c) => a + c.size, 0);
  return { name, path, size, logical: size, is_dir: true, files: children.reduce((a, c) => a + c.files, 0), children };
};
const file = (name: string, parent: string, size: number): Node => ({
  name, path: `${parent}/${name}`, size, logical: size, is_dir: false, files: 1, children: [],
});

function tree(): Node {
  const L = `${HOME}/Library`, D = `${HOME}/Developer`;
  const caches = dir("Caches", L, [
    dir("Google", `${L}/Caches`, [file("Chrome-cache.bin", `${L}/Caches/Google`, 1.9 * GB)]),
    dir("pnpm", `${L}/Caches`, [file("store.db", `${L}/Caches/pnpm`, 1.2 * GB)]),
    file("Homebrew.tar", `${L}/Caches`, 0.6 * GB),
  ]);
  const appsup = dir("Application Support", L, [
    dir("Spotify", `${L}/Application Support`, [file("PersistentCache", `${L}/Application Support/Spotify`, 7.9 * GB)]),
    dir("Code", `${L}/Application Support`, [file("CachedData", `${L}/Application Support/Code`, 0.9 * GB)]),
    dir("Slack", `${L}/Application Support`, [file("Cache", `${L}/Application Support/Slack`, 0.4 * GB)]),
  ]);
  const lib = dir("Library", HOME, [caches, appsup, dir("Containers", L, [file("docker.raw", `${L}/Containers`, 41 * GB)])]);
  const dev = dir("Developer", HOME, [
    dir("ebb", D, [dir("target", `${D}/ebb`, [file("debug.bin", `${D}/ebb/target`, 5.2 * GB)]), file("src.tar", `${D}/ebb`, 12 * MB)]),
    dir("web", D, [dir("node_modules", `${D}/web`, [file("modules.bin", `${D}/web/node_modules`, 2.4 * GB)])]),
  ]);
  const ai = dir(".cache", HOME, [dir("huggingface", `${HOME}/.cache`, [file("models.safetensors", `${HOME}/.cache/huggingface`, 3.7 * GB)])]);
  const media = dir("Movies", HOME, [file("Trip.mov", `${HOME}/Movies`, 6.1 * GB), file("Demo take 3.mov", `${HOME}/Movies`, 1.8 * GB)]);
  const codex = dir(".codex", HOME, [file("sessions.db", `${HOME}/.codex`, 6.8 * GB)]);
  const kids = [lib, dev, media, codex, ai, dir("Documents", HOME, [file("Thesis.pdf", `${HOME}/Documents`, 86 * MB)])];
  return dir("demo", "/Users", kids.sort((a, b) => b.size - a.size));
}

let findings: Finding[] = [
  ["spotify-cache", "Spotify offline cache", "app", "review", `${HOME}/Library/Application Support/Spotify/PersistentCache`, 7.9 * GB],
  ["huggingface-hub", "Hugging Face model cache", "ai", "review", `${HOME}/.cache/huggingface/hub`, 3.7 * GB],
  ["gradle-caches", "Gradle caches", "dev", "safe", `${HOME}/.gradle/caches`, 2.3 * GB],
  ["chrome-cache", "Chrome cache", "browser", "safe", `${HOME}/Library/Caches/Google/Chrome/Default/Cache`, 1.9 * GB],
  ["pnpm-store", "pnpm store", "dev", "review", `${HOME}/Library/pnpm/store`, 1.2 * GB],
  ["cargo-registry", "Cargo registry cache", "dev", "safe", `${HOME}/.cargo/registry/src`, 0.8 * GB],
  ["npm-cache", "npm cache", "dev", "safe", `${HOME}/.npm/_cacache`, 0.24 * GB],
  ["vscode-cache", "VS Code cached data", "app", "safe", `${HOME}/Library/Application Support/Code/CachedData`, 0.12 * GB],
  ["user-logs-macos", "User logs", "system", "safe", `${HOME}/Library/Logs`, 0.06 * GB],
  ["docker-desktop-data", "Docker Desktop VM disk", "dev", "never", `${HOME}/Library/Containers/com.docker.docker/Data`, 41 * GB],
].map(([entry_id, label, category, safety, path, size]) => ({
  entry_id, label, category, safety, path, size: Math.round(size as number), files: 1200,
  note: ({
    "spotify-cache": "Holds offline downloads; they re-download when needed.",
    "huggingface-hub": "Models re-download on demand, but can be many GB.",
    "pnpm-store": "Hard-linked into projects; pruning is safer than deleting.",
    "docker-desktop-data": "Use 'docker system prune' instead.",
  } as Record<string, string>)[entry_id as string] ?? null,
}) as Finding);

const ai: AiStore[] = [
  { id: "codex", label: "Codex", kind: "agent", path: `${HOME}/.codex`, size: 6.8 * GB, files: 25736 },
  { id: "huggingface", label: "Hugging Face cache", kind: "model", path: `${HOME}/.cache/huggingface`, size: 3.7 * GB, files: 90 },
  { id: "claude-code", label: "Claude Code", kind: "agent", path: `${HOME}/.claude`, size: 0.45 * GB, files: 4100 },
  { id: "cursor", label: "Cursor", kind: "agent", path: `${HOME}/.cursor`, size: 0.1 * GB, files: 900 },
];

let used = 782 * GB;
const total = 994 * GB;
const journal: PlanSummary[] = [{ id: "a41c9e07d2b3", created: Date.now() / 1000 - 86400 * 2, total: 3.1 * GB, status: "applied" }];
const plans = new Map<string, Plan>();
const clients: ClientInfo[] = [
  { id: "claude-code", name: "Claude Code", kind: "agent", installed: true, connected: false, config: "~/.claude.json", note: "Open a new Claude Code session." },
  { id: "codex", name: "Codex", kind: "agent", installed: true, connected: false, config: "~/.codex/config.toml", note: "Open a new Codex session." },
  { id: "claude-desktop", name: "Claude Desktop", kind: "chat", installed: true, connected: true, config: "claude_desktop_config.json", note: "Restart Claude Desktop." },
  { id: "lm-studio", name: "LM Studio (local models)", kind: "local", installed: true, connected: false, config: "~/.lmstudio/mcp.json", note: "Start a new chat in LM Studio." },
  { id: "cursor", name: "Cursor", kind: "editor", installed: true, connected: false, config: "~/.cursor/mcp.json", note: "Reload Cursor." },
  { id: "vscode", name: "VS Code", kind: "editor", installed: false, connected: false, config: null, note: "Reload VS Code." },
  { id: "windsurf", name: "Windsurf", kind: "editor", installed: false, connected: false, config: null, note: "Restart Windsurf." },
  { id: "gemini-cli", name: "Gemini CLI", kind: "agent", installed: false, connected: false, config: null, note: "Start a new Gemini CLI session." },
];

export const mockApi: Api = {
  homeDir: async () => HOME,
  diskUsage: async () => ({ total, free: total - used, mount: "/" }),
  async scan(_p, onProgress) {
    for (let i = 1; i <= 12; i++) {
      await wait(120);
      onProgress({ files: i * 41_000, bytes: i * 5.5 * GB, current: `${HOME}/Library/Caches/item-${i}` });
    }
    return tree();
  },
  cancelScan: async () => {},
  async findings() { await wait(500); return findings; },
  async aiStorage() { await wait(300); return ai; },
  async makePlan(f, ids, paths) {
    await wait(200);
    const items = paths.length
      ? paths.map((p) => ({ path: p, size: 1.2 * GB, entry_id: null, reason: "selected" }))
      : f.filter((x) => ids.includes(x.entry_id) && x.safety !== "never").map((x) => ({ path: x.path, size: x.size, entry_id: x.entry_id, reason: x.label }));
    const plan: Plan = { id: Math.random().toString(16).slice(2, 14), created: Date.now() / 1000, items, total: items.reduce((a, b) => a + b.size, 0), skipped: [] };
    plans.set(plan.id, plan);
    return plan;
  },
  async applyPlan(id): Promise<Report> {
    await wait(700);
    const p = plans.get(id)!;
    used -= p.total;
    findings = findings.filter((f) => !p.items.some((i) => i.path === f.path));
    journal.unshift({ id, created: p.created, total: p.total, status: "applied" });
    return { plan_id: id, items: p.items.map((i) => ({ path: i.path, ok: true, error: null })), bytes: p.total, failed: 0 };
  },
  async undoPlan(id): Promise<Report> {
    await wait(500);
    const j = journal.find((x) => x.id === id);
    if (j) { j.status = "undone"; used += j.total; }
    return { plan_id: id, items: [], bytes: j?.total ?? 0, failed: 0 };
  },
  journalList: async () => [...journal],
  explain: async (path) => ({ path, exists: true, size: 1.2 * GB, files: 340, protected: null, catalog: [] }),
  mcpCommand: async () => "/Applications/Ebb.app/Contents/MacOS/ebb",
  connectClients: async () => clients.map((c) => ({ ...c })),
  async connectClient(id) {
    await wait(600);
    const c = clients.find((x) => x.id === id)!;
    c.connected = true;
    return { id, changed: c.config, backup: null, note: c.note };
  },
  async disconnectClient(id) {
    await wait(300);
    const c = clients.find((x) => x.id === id)!;
    c.connected = false;
    return { id, changed: c.config, backup: null, note: "Disconnected." };
  },
  async connectTest() { await wait(700); return 8; },
  async journalItems(planId) {
    const p = plans.get(planId);
    return (p?.items ?? [{ path: `${HOME}/.gradle/caches`, size: 2.3 * GB }, { path: `${HOME}/Library/Caches/Google`, size: 0.8 * GB }]).map((i) => ({ original: i.path, size: i.size, status: "applied", error: null }));
  },
  reveal: async () => {},
};
