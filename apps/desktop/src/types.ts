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

export interface Api {
  homeDir(): Promise<string>;
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
}
