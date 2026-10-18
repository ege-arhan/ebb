import { Channel, invoke } from "@tauri-apps/api/core";
import { mockApi } from "./mock";
import type { Api, Progress } from "./types";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const real: Api = {
  homeDir: () => invoke("home_dir"),
  diskUsage: () => invoke("disk_usage"),
  scan(path, onProgress) {
    const ch = new Channel<Progress>();
    ch.onmessage = onProgress;
    return invoke("scan", { path, onProgress: ch });
  },
  cancelScan: () => invoke("cancel_scan"),
  findings: (tr) => invoke("findings", { tr }),
  aiStorage: () => invoke("ai_storage"),
  makePlan: (findings, ids, paths) => invoke("make_plan", { findings, ids, paths }),
  applyPlan: (planId) => invoke("apply_plan", { planId }),
  undoPlan: (planId) => invoke("undo_plan", { planId }),
  journalList: () => invoke("journal_list"),
  explain: (path) => invoke("explain", { path }),
  mcpCommand: () => invoke("mcp_command"),
  connectClients: () => invoke("connect_clients"),
  connectClient: (id) => invoke("connect_client", { id }),
  disconnectClient: (id) => invoke("disconnect_client", { id }),
  connectTest: () => invoke("connect_test"),
  journalItems: (planId) => invoke("journal_items", { planId }),
  reveal: (path) => invoke("reveal", { path }),
};

export const api: Api = inTauri ? real : mockApi;
