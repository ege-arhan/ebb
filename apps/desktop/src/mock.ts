// Tarayıcıda (Tauri dışı) geliştirme ve ekran görüntüsü doğrulaması için sahte IPC.
import type { Artifact, InstalledApp, PruneAction, Rule, TrashEntry } from "./types";
import type { Api, AiStore, AppUsage, ClientInfo, DupGroup, Finding, Node, Plan, PlanSummary, Report, StartupItem } from "./types";

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
const appList: InstalledApp[] = [
  { id: "/Applications/Xcode.app", name: "Xcode", path: "/Applications/Xcode.app", bundle_id: "com.apple.dt.Xcode", version: "27.0", size: 38 * GB, running: false, last_used_days: 4, system: false },
  { id: "/Applications/Docker.app", name: "Docker", path: "/Applications/Docker.app", bundle_id: "com.docker.docker", version: "4.40", size: 2.4 * GB, running: true, last_used_days: 0, system: false },
  { id: "/Applications/Adobe Premiere.app", name: "Adobe Premiere Pro", path: "/Applications/Adobe Premiere.app", bundle_id: "com.adobe.premiere", version: "26.1", size: 4.1 * GB, running: false, last_used_days: 212, system: false },
  { id: "/Applications/Unity Hub.app", name: "Unity Hub", path: "/Applications/Unity Hub.app", bundle_id: "com.unity3d.unityhub", version: "3.9", size: 1.1 * GB, running: false, last_used_days: 140, system: false },
  { id: "/Applications/Slack.app", name: "Slack", path: "/Applications/Slack.app", bundle_id: "com.tinyspeck.slackmacgap", version: "5.2", size: 480 * MB, running: true, last_used_days: 0, system: false },
  { id: "/Applications/Zoom.app", name: "zoom.us", path: "/Applications/zoom.us.app", bundle_id: "us.zoom.xos", version: "6.4", size: 310 * MB, running: false, last_used_days: 66, system: false },
];
let artifacts: Artifact[] = [
  { path: `${HOME}/Developer/ebb/target`, project: `${HOME}/Developer/ebb`, kind: "cargo_target", size: 5.2 * GB, files: 41000, project_idle_days: 3, safety: "safe", rebuild: "cargo build" },
  { path: `${HOME}/Developer/web/node_modules`, project: `${HOME}/Developer/web`, kind: "node_modules", size: 2.4 * GB, files: 88000, project_idle_days: 58, safety: "safe", rebuild: "npm install" },
  { path: `${HOME}/Developer/old-api/.venv`, project: `${HOME}/Developer/old-api`, kind: "python_venv", size: 1.3 * GB, files: 21000, project_idle_days: 190, safety: "review", rebuild: "python -m venv .venv" },
  { path: `${HOME}/Developer/ios-app/Pods`, project: `${HOME}/Developer/ios-app`, kind: "pods", size: 640 * MB, files: 9000, project_idle_days: 95, safety: "safe", rebuild: "pod install" },
];
const prunes: PruneAction[] = [
  { id: "docker-build", label: "Docker build cache", description: "Unused build cache and dangling images.", command: "docker builder prune -f && docker image prune -f", estimate: 6.4 * GB, installed: true, safety: "safe" },
  { id: "homebrew", label: "Homebrew downloads", description: "Old versions and cached downloads.", command: "brew cleanup -s --prune=all", estimate: 1.1 * GB, installed: true, safety: "safe" },
  { id: "xcode-simulators", label: "Unavailable simulators", description: "Simulators for Xcode versions you removed.", command: "xcrun simctl delete unavailable", estimate: null, installed: true, safety: "safe" },
  { id: "pnpm", label: "pnpm store", description: "Packages no project uses any more.", command: "pnpm store prune", estimate: 380 * MB, installed: true, safety: "safe" },
];
let rules: Rule[] = [
  { id: "weekly-caches", name: "Clean safe caches every week", kind: { type: "safe_caches" }, schedule: { type: "weekly", weekday: 6 }, enabled: true, auto_apply: false, last_run: Date.now() / 1000 - 86400 * 3, last_result: "5.4 GB ready for review" },
  { id: "old-installers", name: "Old installers (30 days)", kind: { type: "old_installers", min_age_days: 30 }, schedule: { type: "weekly", weekday: 0 }, enabled: false, auto_apply: false, last_run: null, last_result: null },
  { id: "trash-30", name: "Empty items older than 30 days from the Trash", kind: { type: "trash_older_than", days: 30 }, schedule: { type: "daily" }, enabled: false, auto_apply: false, last_run: null, last_result: null },
  { id: "low-disk", name: "When disk is under 10% free: safe caches", kind: { type: "safe_caches" }, schedule: { type: "on_low_disk", free_percent: 10 }, enabled: true, auto_apply: false, last_run: null, last_result: null },
  { id: "big-old", name: "Large forgotten files (suggest only)", kind: { type: "large_old", min_mb: 200, min_age_days: 180 }, schedule: { type: "weekly", weekday: 0 }, enabled: false, auto_apply: false, last_run: null, last_result: null },
];
let trash: TrashEntry[] = [
  { id: "t1", name: "caches", size: 2.3 * GB, deleted_at: Date.now() / 1000 - 3600, original: `${HOME}/.gradle/caches`, by_ebb: true, plan_id: "a41c9e07d2b3", restorable: true },
  { id: "t2", name: "Cache", size: 1.9 * GB, deleted_at: Date.now() / 1000 - 3600, original: `${HOME}/Library/Caches/Google/Chrome/Default/Cache`, by_ebb: true, plan_id: "a41c9e07d2b3", restorable: true },
  { id: "t3", name: "Screen Recording 2026-09-12.mov", size: 740 * MB, deleted_at: Date.now() / 1000 - 86400 * 3, original: null, by_ebb: false, plan_id: null, restorable: false },
  { id: "t4", name: "old-notes.zip", size: 12 * MB, deleted_at: Date.now() / 1000 - 86400 * 9, original: null, by_ebb: false, plan_id: null, restorable: false },
];
const dupes: DupGroup[] = [
  { hash: "a1", size: 1.4 * GB, files: [`${HOME}/Movies/Trip to Lisbon.mov`, `${HOME}/Desktop/Trip to Lisbon copy.mov`], reclaimable: 1.4 * GB, optimized: false },
  { hash: "b2", size: 2.1 * GB, files: [`${HOME}/.cache/huggingface/hub/model.safetensors`, `${HOME}/.lmstudio/models/model.safetensors`], reclaimable: 2.1 * GB, optimized: false },
  { hash: "c3", size: 640 * MB, files: [`${HOME}/Documents/Project backup.zip`, `${HOME}/Downloads/Project backup.zip`], reclaimable: 640 * MB, optimized: false },
  { hash: "d4", size: 86 * MB, files: [`${HOME}/Documents/Thesis final.pdf`, `${HOME}/Downloads/Thesis final.pdf`, `${HOME}/Desktop/Thesis final (1).pdf`], reclaimable: 172 * MB, optimized: false },
];
let apps: AppUsage[] = [
  { name: "Google Chrome", pids: [101, 102, 103], memory: 3.1 * GB, cpu: 12.4, quittable: true, path: "/Applications/Google Chrome.app" },
  { name: "Docker", pids: [201], memory: 2.2 * GB, cpu: 3.1, quittable: true, path: "/Applications/Docker.app" },
  { name: "Cursor", pids: [301, 302], memory: 1.4 * GB, cpu: 6.0, quittable: true, path: "/Applications/Cursor.app" },
  { name: "Slack", pids: [401], memory: 720 * MB, cpu: 0.4, quittable: true, path: "/Applications/Slack.app" },
  { name: "WindowServer", pids: [88], memory: 610 * MB, cpu: 4.2, quittable: false, path: null },
];
const startup: StartupItem[] = [
  { id: "user:com.spotify.webhelper", name: "com.spotify.webhelper", path: `${HOME}/Library/LaunchAgents/com.spotify.webhelper.plist`, scope: "user", kind: "launch_agent", enabled: true, can_toggle: true },
  { id: "user:com.google.keystone.agent", name: "com.google.keystone.agent", path: `${HOME}/Library/LaunchAgents/com.google.keystone.agent.plist`, scope: "user", kind: "launch_agent", enabled: true, can_toggle: true },
  { id: "user:com.adobe.ccxprocess", name: "com.adobe.ccxprocess", path: `${HOME}/Library/LaunchAgents/com.adobe.ccxprocess.plist`, scope: "user", kind: "launch_agent", enabled: false, can_toggle: true },
  { id: "system:com.docker.vmnetd", name: "com.docker.vmnetd", path: "/Library/LaunchDaemons/com.docker.vmnetd.plist", scope: "system", kind: "launch_agent", enabled: true, can_toggle: false },
];
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
  locations: async () => [
    { id: "home", label: "Home", path: HOME, kind: "home" as const },
    { id: "desktop", label: "Desktop", path: `${HOME}/Desktop`, kind: "folder" as const },
    { id: "documents", label: "Documents", path: `${HOME}/Documents`, kind: "folder" as const },
    { id: "downloads", label: "Downloads", path: `${HOME}/Downloads`, kind: "folder" as const },
    { id: "developer", label: "Developer", path: `${HOME}/Developer`, kind: "folder" as const },
    { id: "library", label: "Library", path: `${HOME}/Library`, kind: "folder" as const },
    { id: "disk", label: "Whole disk", path: "/", kind: "disk" as const },
    { id: "vol-Backup", label: "Backup", path: "/Volumes/Backup", kind: "volume" as const },
  ],
  pickFolder: async () => `${HOME}/Developer`,
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
  async diskHistory(days) {
    const now = Date.now() / 1000, out = [];
    for (let i = days * 4; i >= 0; i--) {
      const ts = now - i * 6 * 3600;
      const drift = Math.sin(i / 9) * 6 * GB + (days * 4 - i) * 0.12 * GB;
      out.push({ ts, total, free: Math.max(40 * GB, total - used + 18 * GB - drift) });
    }
    return out;
  },
  openRelease: async (url) => { window.open(url, "_blank"); },
  trashList: async () => { await wait(300); return trash.map((x) => ({ ...x })); },
  async trashRestore(ids) { await wait(400); trash = trash.filter((x) => !ids.includes(x.id)); return ids.length; },
  async trashPurge(ids) { await wait(400); const b = trash.filter((x) => ids.includes(x.id)).reduce((a, x) => a + x.size, 0); trash = trash.filter((x) => !ids.includes(x.id)); used -= b; return b; },
  async trashEmpty() { await wait(700); const b = trash.reduce((a, x) => a + x.size, 0); trash = []; used -= b; return b; },
  openPrivacySettings: async () => {},
  popoverStats: async () => ({ disk: { total, free: total - used, mount: "/" }, memory: { total: 16 * GB, used: 12.9 * GB, available: 3.1 * GB, swap_total: 4 * GB, swap_used: 1.6 * GB, pressure: 0.68 }, safe: 5.4 * GB, trash: 5.0 * GB, trash_items: 4 }),
  openMain: async () => {},
  quitEbb: async () => {},
  trayGet: async () => ({ title: "free", lines: ["free", "safe", "trash"] }),
  traySet: async () => {},
  async planGet(id) { return plans.get(id) ?? { id, created: Date.now() / 1000, items: [], total: 0, skipped: [] }; },
  appsList: async () => { await wait(300); return appList.map((a) => ({ ...a })); },
  async appPreview(id) {
    await wait(300);
    const app = appList.find((a) => a.id === id)!;
    const leftovers = [
      { path: `${HOME}/Library/Application Support/${app.name}`, size: 420 * MB, kind: "support", confidence: "high" as const },
      { path: `${HOME}/Library/Caches/${app.bundle_id}`, size: 310 * MB, kind: "cache", confidence: "high" as const },
      { path: `${HOME}/Library/Preferences/${app.bundle_id}.plist`, size: 24_000, kind: "preferences", confidence: "high" as const },
      { path: `${HOME}/Library/Logs/${app.name}`, size: 12 * MB, kind: "logs", confidence: "medium" as const },
    ];
    return { app, leftovers, total: app.size + leftovers.reduce((a, l) => a + l.size, 0), warnings: app.running ? [`${app.name} is running. Quit it first.`] : [] };
  },
  async appUninstallPlan(id) {
    await wait(300);
    const pv = await mockApi.appPreview(id);
    const items = [{ path: pv.app.path, size: pv.app.size, entry_id: null, reason: pv.app.name }, ...pv.leftovers.filter((l) => l.confidence === "high").map((l) => ({ path: l.path, size: l.size, entry_id: null, reason: l.kind }))];
    const plan = { id: Math.random().toString(16).slice(2, 14), created: Date.now() / 1000, items, total: items.reduce((a, b) => a + b.size, 0), skipped: [] as [string, string][] };
    plans.set(plan.id, plan);
    return plan;
  },
  projectArtifacts: async () => { await wait(900); return artifacts.map((a) => ({ ...a })); },
  pruneList: async () => { await wait(300); return prunes.map((p) => ({ ...p })); },
  async pruneRun(id) { await wait(1200); const p = prunes.find((x) => x.id === id); used -= p?.estimate ?? 0; return { id, ok: true, output: "done", before: total - used, after: total - used + (p?.estimate ?? 0) }; },
  careRules: async () => rules.map((r) => ({ ...r })),
  async careSave(r) { rules = rules.map((x) => (x.id === r.id ? r : x)); },
  async careRun(id) { await wait(900); const r = rules.find((x) => x.id === id)!; r.last_run = Date.now() / 1000; r.last_result = "5.4 GB ready for review"; return { rule_id: id, plan_id: "demo", reclaimable: 5.4 * GB, applied: false, moved: 0, message: "Plan ready: 5.4 GB. Review it in the Journal." }; },
  aiProviders: async () => [{ id: "claude-cli", name: "Claude Code", base_url: "/usr/local/bin/claude", models: ["default"], online: true }, { id: "codex-cli", name: "Codex", base_url: "/usr/local/bin/codex", models: ["default"], online: true }, { id: "ollama", name: "Ollama", base_url: "http://127.0.0.1:11434", models: ["llama3.2:3b", "qwen2.5:7b"], online: true }],
  async aiChat(_p, _m, _h, message) {
    await wait(1400);
    return { text: `Looking at your disk: about ${(782).toFixed(0)} GB is used. The biggest safe wins are Gradle caches (2.3 GB) and Chrome's cache (1.9 GB). I prepared a plan for the safe caches (5.4 GB). Nothing was deleted: review it, then apply it yourself.\n\nYou asked: "${message}"`, tool_calls: [{ name: "disk_usage", args: {}, result_preview: "{\"free\":212e9}" }, { name: "findings", args: {}, result_preview: "[…]" }, { name: "plan_cleanup", args: {}, result_preview: "plan ready" }], plan_id: "demo" };
  },
  async findDuplicates(_root, onProgress) {
    for (let i = 1; i <= 8; i++) { await wait(150); onProgress({ stage: i < 5 ? "listing" : "hashing", files: i * 52_000, bytes: i * 9 * GB }); }
    return { groups: dupes.map((g) => ({ ...g })), cloneSupported: true };
  },
  async optimizeClones(groups) {
    await wait(900);
    const saved = groups.reduce((a, g) => a + g.reclaimable, 0);
    groups.forEach((g) => { const d = dupes.find((x) => x.hash === g.hash); if (d) { d.optimized = true; d.reclaimable = 0; } });
    return { files: groups.reduce((a, g) => a + g.files.length - 1, 0), saved, skipped: [] };
  },
  async compressAnalyze(path) {
    await wait(900);
    const types = [
      { category: "code", files: 12400, bytes: 1.2 * GB, est_saved: 0.84 * GB, ratio: 0.7, worthwhile: true, reason: "compresses well" },
      { category: "documents", files: 320, bytes: 420 * MB, est_saved: 160 * MB, ratio: 0.38, worthwhile: true, reason: "compresses well" },
      { category: "text", files: 2100, bytes: 180 * MB, est_saved: 130 * MB, ratio: 0.72, worthwhile: true, reason: "compresses well" },
      { category: "images", files: 880, bytes: 1.4 * GB, est_saved: 0, ratio: 0, worthwhile: false, reason: "already compressed" },
      { category: "video", files: 14, bytes: 3.1 * GB, est_saved: 0, ratio: 0, worthwhile: false, reason: "already compressed" },
      { category: "archives", files: 60, bytes: 640 * MB, est_saved: 0, ratio: 0, worthwhile: false, reason: "already compressed" },
    ];
    return { path, total: types.reduce((a, x) => a + x.bytes, 0), est_saved: types.reduce((a, x) => a + x.est_saved, 0), types, method: "APFS transparent compression, per file", supported: true };
  },
  async compressSmart(path) { await wait(1500); return { path, before: 6.9 * GB, after: 5.7 * GB, compressed_files: 14820, skipped_files: 962, skipped_bytes: 5.1 * GB }; },
  async aiAskAll(targets, _h, message) {
    await wait(1600);
    return targets.map(([p, m], i) => ({ provider: p.name, model: m, text: `${["Claude Code", "Codex", "Ollama"][i % 3]} says: about 782 GB is used. The quickest safe win is 5.4 GB of caches. (You asked: ${message})`, error: null, elapsed_ms: 900 + i * 700 }));
  },
  async compress(path) { await wait(1200); return { path, before: 4.2 * GB, after: 2.6 * GB, files: 1830 }; },
  async decompress(path) { await wait(900); return { path, before: 2.6 * GB, after: 4.2 * GB, files: 1830 }; },
  async largeOld() {
    await wait(500);
    return [
      { path: `${HOME}/Movies/Old screen recording.mov`, size: 3.8 * GB, age_days: 412, kind: "large" as const },
      { path: `${HOME}/Documents/VM/ubuntu-22.04.vdi`, size: 12.4 * GB, age_days: 260, kind: "large" as const },
      { path: `${HOME}/Desktop/export-final-v2.mp4`, size: 1.1 * GB, age_days: 190, kind: "large" as const },
    ].sort((a, b) => b.size - a.size);
  },
  async oldInstallers() {
    await wait(300);
    return [
      { path: `${HOME}/Downloads/Xcode_16.xip`, size: 7.1 * GB, age_days: 88, kind: "installer" as const },
      { path: `${HOME}/Downloads/Docker.dmg`, size: 610 * MB, age_days: 41, kind: "installer" as const },
      { path: `${HOME}/Downloads/zoom.pkg`, size: 98 * MB, age_days: 120, kind: "installer" as const },
    ];
  },
  async health() {
    await wait(300);
    return {
      score: 72, disk_free_ratio: (total - used) / total, uptime_secs: 9 * 86400, startup_enabled: 3,
      memory: { total: 16 * GB, used: 12.9 * GB, available: 3.1 * GB, swap_total: 4 * GB, swap_used: 1.6 * GB, pressure: 0.68 },
      tips: [
        { id: "uptime", severity: "warn" as const, title: "Your Mac has been on for 9 days", body: "A restart clears memory that apps leaked over time and applies pending updates.", action: "restart" as const },
        { id: "heavy", severity: "info" as const, title: "Google Chrome is using 3.1 GB of memory", body: "Closing tabs you no longer need, or quitting it for a while, frees memory for other apps.", action: "apps" as const },
        { id: "startup", severity: "info" as const, title: "3 items start with your Mac", body: "Fewer startup items mean a faster login and less background work.", action: "startup" as const },
        { id: "disk", severity: "good" as const, title: "Your disk has healthy free space", body: "21% free. macOS stays fast while at least ~10% is free.", action: null },
      ],
    };
  },
  async topApps() { await wait(300); return apps.map((a) => ({ ...a, cpu: Math.max(0, a.cpu + (Math.random() - 0.5) * 3) })); },
  async quitApp(pids) { await wait(400); apps = apps.filter((a) => a.pids[0] !== pids[0]); return pids.length; },
  async startupItems() { await wait(200); return startup.map((s) => ({ ...s })); },
  async setStartup(id, enabled) { await wait(250); const s = startup.find((x) => x.id === id); if (s) s.enabled = enabled; },
};
