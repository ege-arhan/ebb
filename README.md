<p align="center">
  <img src="apps/desktop/src-tauri/icons/128x128@2x.png" width="96" alt="Ebb">
</p>

<h1 align="center">Ebb</h1>

<p align="center">
  <b>See what fills your disk. Let the safe part go. Let your AI help, never delete.</b><br>
  Open source disk cleaner and AI-storage viewer for macOS, Windows and Linux.
</p>

<p align="center">
  <a href="https://github.com/ege-arhan/ebb/actions/workflows/ci.yml"><img src="https://github.com/ege-arhan/ebb/actions/workflows/ci.yml/badge.svg" alt="ci"></a>
  <a href="https://github.com/ege-arhan/ebb/releases/latest"><img src="https://img.shields.io/github/v/release/ege-arhan/ebb?label=download" alt="release"></a>
  <img src="https://img.shields.io/badge/license-Apache--2.0-blue" alt="Apache-2.0">
  <img src="https://img.shields.io/badge/telemetry-none-brightgreen" alt="no telemetry">
</p>

<p align="center">
  <img src="docs/screenshots/tide.png" alt="Ebb: the tide screen" width="860">
</p>

## Why Ebb

- **A tide, not a dashboard.** The water is your used space; the pale band is the part Ebb can safely let go. It reacts to your pointer, and the water recedes when you clean.
- **Plan → review → apply → undo.** Nothing moves until you confirm a plan you can read. Everything goes to the Trash and the journal remembers it for 90 days.
- **Protected by default.** System folders, your home root, key stores, cloud-sync folders and git repos with uncommitted changes are never touched.
- **AI-native, but safe.** `ebb connect` wires Claude Code, Codex, Claude Desktop, LM Studio, Cursor, VS Code, Windsurf and Gemini CLI to a local MCP server in one step. Assistants can scan, explain and *plan*. They cannot delete, move or restore anything.
- **Knows what AI tools keep on disk.** Claude Code, Codex, Cursor, Ollama, LM Studio, Hugging Face caches and more.
- **Fast.** A 2-million-file home folder scans in ~16 s (3x faster than `du` on the same machine).
- **Lives in your menu bar / tray.** Free space at a glance, quick actions, and closing the window keeps Ebb running.
- **Free, with no limits and no telemetry.** Apache-2.0.

## Install

Download from the [latest release](https://github.com/ege-arhan/ebb/releases/latest):

| Platform | File |
|---|---|
| macOS (Apple Silicon / Intel) | `Ebb_*_aarch64.dmg` / `Ebb_*_x64.dmg` |
| Windows | `Ebb_*_x64-setup.exe` (or the `.msi`) |
| Linux | `.AppImage`, `.deb` |
| Any, CLI only | `ebb-cli-*` archive |

The apps are **not code-signed yet**. On macOS, right-click the app → *Open* the first time (or run `xattr -dr com.apple.quarantine /Applications/Ebb.app`). On Windows, choose *More info → Run anyway* in SmartScreen.

## The CLI

```bash
ebb                    # dashboard: disk, what can ebb, AI storage
ebb clean              # pick what to clean (interactive), moves to the Trash
ebb scan ~/Projects    # where did the space go?
ebb why ~              # what grew since last time
ebb agents             # AI tools and local models on disk
ebb connect            # connect your AI assistants
ebb undo <plan-id>     # restore a cleanup
ebb completions zsh    # shell completions
```

Every command takes `--json`. `ebb apply <id>` and non-interactive `ebb clean` need `--yes`.

## Connect AI in one step

```bash
ebb connect            # detects Claude Code, Codex, Claude Desktop, LM Studio, Cursor, VS Code, Windsurf, Gemini CLI
ebb connect --list     # what was found and what is connected
ebb connect --remove cursor
```

`ebb connect` edits each client's own config atomically, keeps the first backup (`*.ebb-backup`), refuses symlinked or non-JSON configs, never overwrites a server called `ebb` it did not create, and finishes with a real self-test of the MCP server. The desktop app has the same flow on the **Connect AI** screen.

MCP tools: `ebb_scan`, `ebb_top`, `ebb_why`, `ebb_explain_path`, `ebb_findings`, `ebb_agents_storage`, `ebb_plan_cleanup`, `ebb_plan_status`. There is deliberately **no apply and no undo tool**: anything that moves files needs a human (`ebb apply --yes`, `ebb undo`, or the app).

## Build from source

```bash
pnpm -C apps/desktop install
node apps/desktop/scripts/sidecar.mjs debug      # builds the `ebb` CLI the app bundles
pnpm -C apps/desktop tauri dev                   # desktop app
pnpm -C apps/desktop dev                         # UI in a browser with mock data
cargo test --workspace
pnpm -C apps/desktop tauri build                 # installers for your OS
```

Layout: `crates/ebb-core` (scan, catalog, plan/journal, connect) · `crates/ebb-cli` · `crates/ebb-mcp` · `apps/desktop` (Tauri 2 + React) · `catalog/*.yaml` (cleanup recipes: add yours with a PR).

## Safety, reviewed

Two independent code reviews found 16 issues (protected-ancestor bypass, re-armed plans, symlink overwrites, MCP-triggered restore, concurrent config writes, …). All are fixed and pinned by tests in `crates/ebb-core/tests/review*.rs`. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [docs/reviews](docs/reviews). Windows and Linux builds are verified by CI only; real-device reports are very welcome.

## License

Apache-2.0. See [LICENSE](LICENSE). Contributions: [CONTRIBUTING.md](CONTRIBUTING.md). Security: [SECURITY.md](SECURITY.md).
