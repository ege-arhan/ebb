export type Safety = "safe" | "review" | "never";

export interface Node {
  name: string;
  path: string;
  size: number;
  logical: number;
  is_dir: boolean;
  files: number;
  children: Node[];
}
export interface Progress { files: number; bytes: number; current: string }
export interface Finding {
  entry_id: string; label: string; category: string; safety: Safety;
  path: string; size: number; files: number; note?: string | null;
}
export interface PlanItem { path: string; size: number; entry_id: string | null; reason: string }
export interface Plan { id: string; created: number; items: PlanItem[]; total: number; skipped: [string, string][] }
export interface ItemResult { path: string; ok: boolean; error: string | null }
export interface Report { plan_id: string; items: ItemResult[]; bytes: number; failed: number }
export interface PlanSummary { id: string; created: number; total: number; status: string }
export interface AiStore { id: string; label: string; kind: "agent" | "model"; path: string; size: number; files: number }
export interface DiskUsage { total: number; free: number; mount: string }
export interface Explain {
  path: string; exists: boolean; size: number; files: number; protected: string | null;
  catalog: [string, string, Safety][];
}

export interface ClientInfo {
  id: string; name: string; kind: "agent" | "chat" | "editor" | "local";
  installed: boolean; connected: boolean; config: string | null; note: string;
}
export interface Outcome { id: string; changed: string | null; backup: string | null; note: string }
export interface EntryRow { original: string; size: number; status: string; error: string | null }

export interface Location { id: string; label: string; path: string; kind: "home" | "folder" | "disk" | "volume" }

export interface DupGroup { hash: string; size: number; files: string[]; reclaimable: number; optimized: boolean }
export interface DupProgress { stage: string; files: number; bytes: number }
export interface OptimizeReport { files: number; saved: number; skipped: [string, string][] }
export interface CompressReport { path: string; before: number; after: number; files: number }
export interface FoundFile { path: string; size: number; age_days: number; kind: "large" | "installer" }
export interface MemoryInfo { total: number; used: number; available: number; swap_total: number; swap_used: number; pressure: number }
export interface AppUsage { name: string; pids: number[]; memory: number; cpu: number; quittable: boolean; path: string | null }
export interface StartupItem { id: string; name: string; path: string; scope: "user" | "system"; kind: string; enabled: boolean; can_toggle: boolean }
export interface Tip { id: string; severity: "good" | "info" | "warn" | "bad"; title: string; body: string; action: "clean" | "apps" | "startup" | "restart" | "optimize" | null }
export interface Health { score: number; memory: MemoryInfo; disk_free_ratio: number; uptime_secs: number; startup_enabled: number; tips: Tip[] }

export interface DiskSample { ts: number; total: number; free: number }

export interface TrashEntry { id: string; name: string; size: number; deleted_at: number | null; original: string | null; by_ebb: boolean; plan_id: string | null; restorable: boolean }

export interface InstalledApp { id: string; name: string; path: string; bundle_id: string | null; version: string | null; size: number; running: boolean; last_used_days: number | null; system: boolean }
export interface Leftover { path: string; size: number; kind: string; confidence: "high" | "medium" }
export interface UninstallPreview { app: InstalledApp; leftovers: Leftover[]; total: number; warnings: string[] }
export interface Artifact { path: string; project: string; kind: string; size: number; files: number; project_idle_days: number; safety: "safe" | "review"; rebuild: string }
export interface PruneAction { id: string; label: string; description: string; command: string; estimate: number | null; installed: boolean; safety: "safe" | "review" }
export interface PruneResult { id: string; ok: boolean; output: string; before: number; after: number }
export type RuleKind = { type: "safe_caches" } | { type: "old_installers"; min_age_days: number } | { type: "trash_older_than"; days: number } | { type: "large_old"; min_mb: number; min_age_days: number };
export type Schedule = { type: "daily" } | { type: "weekly"; weekday: number } | { type: "on_low_disk"; free_percent: number };
export interface Rule { id: string; name: string; kind: RuleKind; schedule: Schedule; enabled: boolean; auto_apply: boolean; last_run: number | null; last_result: string | null }
export interface RuleOutcome { rule_id: string; plan_id: string | null; reclaimable: number; applied: boolean; moved: number; message: string }
export interface Provider { id: string; name: string; base_url: string; models: string[]; online: boolean }
export interface ChatMessage { role: "user" | "assistant" | "tool"; content: string }
export interface AiToolCall { name: string; args: unknown; result_preview: string }
export interface Reply { text: string; tool_calls: AiToolCall[]; plan_id: string | null }

export interface TrayConfig { title: string; lines: string[] }

export interface Api {
  homeDir(): Promise<string>;
  locations(): Promise<Location[]>;
  pickFolder(): Promise<string | null>;
  diskUsage(): Promise<DiskUsage | null>;
  scan(path: string | null, onProgress: (p: Progress) => void): Promise<Node>;
  cancelScan(): Promise<void>;
  findings(tr: boolean): Promise<Finding[]>;
  aiStorage(): Promise<AiStore[]>;
  makePlan(findings: Finding[], ids: string[], paths: string[]): Promise<Plan>;
  applyPlan(id: string): Promise<Report>;
  undoPlan(id: string): Promise<Report>;
  journalList(): Promise<PlanSummary[]>;
  explain(path: string): Promise<Explain>;
  mcpCommand(): Promise<string>;
  connectClients(): Promise<ClientInfo[]>;
  connectClient(id: string): Promise<Outcome>;
  disconnectClient(id: string): Promise<Outcome>;
  connectTest(): Promise<number>;
  journalItems(planId: string): Promise<EntryRow[]>;
  reveal(path: string): Promise<void>;
  diskHistory(days: number): Promise<DiskSample[]>;
  trayGet(): Promise<TrayConfig>;
  traySet(config: TrayConfig): Promise<void>;
  planGet(id: string): Promise<Plan>;
  appsList(): Promise<InstalledApp[]>;
  appPreview(id: string): Promise<UninstallPreview>;
  appUninstallPlan(id: string, includeMedium: boolean): Promise<Plan>;
  projectArtifacts(idleDays: number): Promise<Artifact[]>;
  pruneList(): Promise<PruneAction[]>;
  pruneRun(id: string): Promise<PruneResult>;
  careRules(): Promise<Rule[]>;
  careSave(rule: Rule): Promise<void>;
  careRun(id: string): Promise<RuleOutcome>;
  aiProviders(): Promise<Provider[]>;
  aiChat(provider: Provider, model: string, history: ChatMessage[], message: string): Promise<Reply>;
  trashList(): Promise<TrashEntry[]>;
  trashRestore(ids: string[]): Promise<number>;
  trashPurge(ids: string[]): Promise<number>;
  trashEmpty(): Promise<number>;
  openPrivacySettings(): Promise<void>;
  openRelease(url: string): Promise<void>;
  findDuplicates(root: string | null, onProgress: (p: DupProgress) => void): Promise<{ groups: DupGroup[]; cloneSupported: boolean }>;
  optimizeClones(groups: DupGroup[]): Promise<OptimizeReport>;
  compress(path: string): Promise<CompressReport>;
  decompress(path: string): Promise<CompressReport>;
  largeOld(root: string | null): Promise<FoundFile[]>;
  oldInstallers(): Promise<FoundFile[]>;
  health(): Promise<Health>;
  topApps(): Promise<AppUsage[]>;
  quitApp(pids: number[]): Promise<number>;
  startupItems(): Promise<StartupItem[]>;
  setStartup(id: string, enabled: boolean): Promise<void>;
}
