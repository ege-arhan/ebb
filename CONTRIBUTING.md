# Contributing to Ebb

Thanks for helping. Ebb moves files, so changes are held to a safety bar: a feature that can delete or move anything needs a test that proves it cannot touch protected paths and that undo restores it.

## Setup

```bash
pnpm -C apps/desktop install
node apps/desktop/scripts/sidecar.mjs debug   # builds the `ebb` CLI the desktop app bundles
cargo test --workspace
pnpm -C apps/desktop dev                      # UI in the browser with mock data
pnpm -C apps/desktop tauri dev                # the real desktop app
```

## Adding a cleanup recipe

Recipes live in `catalog/*.yaml`. One entry per cache, with paths per OS (`{home}`, `{cache}`, `{data}`, `{local}`, `{temp}` are expanded). Safety levels:

- `safe`: the app rebuilds it by itself.
- `review`: removing it can cost something (offline data, a long download). Add a `note` that says what.
- `never`: shown for information, never put in a plan.

`cargo test -p ebb-core` checks that the catalog parses and ids are unique.

## Rules of the road

- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `pnpm -C apps/desktop build` must pass.
- The MCP server has no apply and no undo tool, on purpose. Do not add one.
- No telemetry, no network calls except what the user starts.
