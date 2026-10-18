# Architecture

One Rust core (`ebb-core`) shared by the CLI, the MCP server and the Tauri app.

- `scan` – parallel walk (`jwalk`), allocated vs logical size, hardlinks counted once, no mount crossing.
- `catalog` – YAML recipes embedded in the binary. Tokens `{home} {cache} {data} {local} {temp}` keep one recipe cross-platform. Safety: `safe` (rebuilds itself), `review`, `never`.
- `guard` – protected paths, checked when a plan is created and again when it is applied.
- `plan` / `journal` – a `Plan` is data; `Journal` (SQLite) applies it to the Trash and records where each item went so it can be restored.
- `ops` – high-level calls reused by CLI, MCP and GUI.
- `agents` – AI agent and model storage.

- `connect` – detects MCP clients and writes Ebb into their config (JSON atomically, or via the client's own CLI); ownership-checked, symlink-refusing, first backup kept; `selftest` starts `ebb mcp` for real.

Safety model: AI can only read and create plans (the MCP server has no apply and no undo tool). Applying needs `ebb apply --yes`, an interactive confirmation, or the GUI. Undo refuses to restore through a folder chain that became a symlink. No telemetry.

Reviews: two independent Codex reviews found 16 issues (guard ancestors, re-armed plans, symlink overwrites, MCP undo, concurrent config writes, ...). All are fixed and pinned by `crates/ebb-core/tests/review*.rs`. Reports: `.ortak/INCELEME*.md`.

Known v0.1 limits: Windows reports logical size (allocated size comes later); restore on Windows/Linux uses the `trash` crate's `os_limited` API and is only covered by CI compilation, not by tests on those systems.
