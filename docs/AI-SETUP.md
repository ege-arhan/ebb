# Ebb setup for AI assistants

> You are an AI assistant. A user asked you to install Ebb and connect it to themselves. Follow these steps exactly. The user does not need to know any code.

Ebb is an open-source disk cleaner. It exposes a local MCP server (`ebb mcp`) so assistants can look at disk usage and *plan* cleanups. It can never delete, move or restore files through MCP.

## 1. Install the `ebb` CLI

Detect the user's OS, then run one command.

- **macOS / Linux:**
  ```sh
  curl -fsSL https://raw.githubusercontent.com/ege-arhan/ebb/main/install.sh | sh
  ```
- **Windows (PowerShell):**
  ```powershell
  irm https://raw.githubusercontent.com/ege-arhan/ebb/main/install.ps1 | iex
  ```

Verify with `ebb --version`. If `ebb` is not found, use the full path the installer printed (macOS/Linux: `~/.local/bin/ebb`, Windows: `%LOCALAPPDATA%\Ebb\bin\ebb.exe`).

## 2. Connect the assistants

```sh
ebb connect --list     # show which assistants were detected
ebb connect --all      # connect every detected assistant (safe: keeps a backup, never overwrites other servers)
ebb connect --list     # confirm they now show "connected"
```

`ebb connect --all` also runs a self-test and prints how many MCP tools answered (15 expected).

If you cannot run commands, give the user this JSON for their MCP client's config (replace the path with the absolute path of `ebb`):

```json
{ "mcpServers": { "ebb": { "command": "/absolute/path/to/ebb", "args": ["mcp"] } } }
```

## 3. Tell the user what to do next

- Restart or reload the assistants that were just connected (Claude Desktop, Cursor, VS Code, Windsurf) and open a new chat/session (Claude Code, Codex, Gemini CLI, LM Studio).
- Suggest a first question: *"What is using my disk space?"*

## Rules

- **Never** run `ebb apply`, `ebb undo`, or `ebb clean --yes` on the user's behalf. Moving files needs the user's own confirmation (the Ebb app, or `ebb clean` in a terminal they operate).
- Do not run anything beyond the commands above. Do not edit config files by hand if `ebb connect` works.
- If a step fails, show the exact error and stop; do not guess.

Project: https://github.com/ege-arhan/ebb
