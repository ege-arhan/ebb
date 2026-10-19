# Changelog

## 0.5.0
- **Uninstall apps completely**: app + leftovers in one reviewed plan (macOS, Windows, Linux).
- **Developer cleanup**: rebuildable project folders (node_modules, target, .venv, Pods…) in idle projects, and tools' own cleanup commands (Docker, Homebrew, Xcode simulators, npm, pnpm, pip, go, conda).
- **Auto Care**: scheduled rules that prepare plans (apply automatically only if you turn it on; never for large files).
- **Assistant**: chat with a local model (Ollama / LM Studio) that can look and plan, never apply. `ebb ask`.
- Auto-update via signed releases; German, Spanish and French UI; grouped navigation; 3 more read-only MCP tools.
- CLI: `ebb uninstall`, `ebb projects`, `ebb prune`, `ebb care`, `ebb ask`.

## 0.4.0
- Trash screen: restore Ebb's items, delete selected, empty. Free-space history chart. Optional daily update check.

## 0.3.0
- Optimize without deleting (clone dedupe, folder compression), large/old files and installers, Speed up (health, memory, apps, startup items), 60+ cache recipes.

## 0.2.x
- Location picker for Explore, AI setup by prompt (`docs/AI-SETUP.md`, install scripts), menu bar / tray, live UI.

## 0.1.0
- Core scanner, catalog, plan → review → apply → undo, CLI, MCP server, desktop app.
