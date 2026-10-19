import { Channel, invoke } from "@tauri-apps/api/core";
import { mockApi } from "./mock";
import type { Api, DupProgress, Progress } from "./types";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const real: Api = {
  homeDir: () => invoke("home_dir"),
  locations: () => invoke("locations"),
  pickFolder: () => invoke("pick_folder"),
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
  diskHistory: (days) => invoke("disk_history", { days }),
  trayGet: () => invoke("tray_get"),
  traySet: (config) => invoke("tray_set", { config }),
  planGet: (id) => invoke("plan_get", { id }),
  appsList: () => invoke("apps_list"),
  appPreview: (id) => invoke("app_preview", { id }),
  appUninstallPlan: (id, includeMedium) => invoke("app_uninstall_plan", { id, includeMedium }),
  projectArtifacts: (idleDays) => invoke("project_artifacts", { idleDays }),
  pruneList: () => invoke("prune_list"),
  pruneRun: (id) => invoke("prune_run", { id }),
  careRules: () => invoke("care_rules"),
  careSave: (rule) => invoke("care_save", { rule }),
  careRun: (id) => invoke("care_run", { id }),
  aiProviders: () => invoke("ai_providers"),
  aiChat: (provider, model, history, message) => invoke("ai_chat", { provider, model, history, message }),
  trashList: () => invoke("trash_list"),
  trashRestore: (ids) => invoke("trash_restore", { ids }),
  trashPurge: (ids) => invoke("trash_purge", { ids }),
  trashEmpty: () => invoke("trash_empty"),
  openPrivacySettings: () => invoke("open_privacy_settings"),
  openRelease: (url) => invoke("open_release", { url }),
  findDuplicates(root, onProgress) {
    const ch = new Channel<DupProgress>();
    ch.onmessage = onProgress;
    return invoke("find_duplicates", { root, onProgress: ch });
  },
  optimizeClones: (groups) => invoke("optimize_clones", { groups }),
  compress: (path) => invoke("compress", { path }),
  decompress: (path) => invoke("decompress", { path }),
  largeOld: (root) => invoke("large_old", { root }),
  oldInstallers: () => invoke("old_installers"),
  health: () => invoke("health"),
  topApps: () => invoke("top_apps"),
  quitApp: (pids) => invoke("quit_app", { pids }),
  startupItems: () => invoke("startup_items"),
  setStartup: (id, enabled) => invoke("set_startup", { id, enabled }),
};

export const api: Api = inTauri ? real : mockApi;
