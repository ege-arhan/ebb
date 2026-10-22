# Changelog

## 0.5.3
- **Compression cleanup fixed:** folders containing files flagged immutable (`uchg`, common in some git/backup trees) no longer leave a hidden `.name.ebb-compress.N` copy behind. Ebb now double-checks the new copy contains everything before deleting the old one, and keeps the old one (and says where) if it doesn't.
- Clear message when a folder barely shrinks (already-compressed PDFs, photos, videos, archives).

## 0.5.2
- The update button installs the new version in place (signed updater) and restarts Ebb; falls back to the download page if that fails.

## 0.5.1
- **Responsive menu:** the sidebar is full on wide windows, an icon rail on medium ones and a slide-in drawer (hamburger) on narrow ones; the window can now be much smaller.
- **Menu bar / tray you can configure:** choose what shows next to the icon (free space, safe to clean, memory, Trash size, health score) and which lines appear in the menu. New menu entries: Open Trash, Speed up.

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
