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
