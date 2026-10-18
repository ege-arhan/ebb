//! Tek tıkla AI bağlama: yüklü MCP istemcilerini bulur, Ebb'i onların yapılandırmasına ekler/çıkarır,
//! ve bağlantıyı gerçekten `ebb mcp` başlatıp araç listesini isteyerek sınar.
use crate::{Error, Result};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Serialize, Clone, Debug)]
pub struct ClientInfo {
    pub id: &'static str,
    pub name: &'static str,
    /// "agent" (kod ajanı), "chat" (sohbet uygulaması), "editor", "local" (yerel model uygulaması)
    pub kind: &'static str,
    pub installed: bool,
    pub connected: bool,
    pub config: Option<PathBuf>,
    /// Kullanıcıya gösterilecek kısa not (ör. "Yeniden başlat").
    pub note: &'static str,
}

#[derive(Serialize, Debug)]
pub struct Outcome {
    pub id: String,
    pub changed: Option<PathBuf>,
    pub backup: Option<PathBuf>,
    pub note: &'static str,
}

enum Via {
    /// İstemcinin kendi CLI'si (`claude mcp add ...`).
    Cli {
        bin: &'static str,
        add: fn(&Path) -> Vec<String>,
        remove: &'static [&'static str],
    },
    /// JSON yapılandırma dosyası; `key` altına `ebb` girdisi yazılır.
    Json { key: &'static str, typed: bool },
}

struct Def {
    id: &'static str,
    name: &'static str,
    kind: &'static str,
    config: PathBuf,
    detect: Vec<PathBuf>,
    via: Via,
    note: &'static str,
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}

fn defs() -> Vec<Def> {
    let h = home();
    let cfg = dirs::config_dir().unwrap_or_else(|| h.join(".config"));
    vec![
        Def {
            id: "claude-code",
            name: "Claude Code",
            kind: "agent",
            config: h.join(".claude.json"),
            detect: vec![h.join(".claude")],
            via: Via::Cli {
                bin: "claude",
                add: |c| {
                    vec![
                        "mcp".into(),
                        "add".into(),
                        "--scope".into(),
                        "user".into(),
                        "ebb".into(),
                        "--".into(),
                        c.to_string_lossy().into(),
                        "mcp".into(),
                    ]
                },
                remove: &["mcp", "remove", "--scope", "user", "ebb"],
            },
            note: "Open a new Claude Code session.",
        },
        Def {
            id: "codex",
            name: "Codex",
            kind: "agent",
            config: h.join(".codex/config.toml"),
            detect: vec![h.join(".codex")],
            via: Via::Cli {
                bin: "codex",
                add: |c| {
                    vec![
                        "mcp".into(),
                        "add".into(),
                        "ebb".into(),
                        "--".into(),
                        c.to_string_lossy().into(),
                        "mcp".into(),
                    ]
                },
                remove: &["mcp", "remove", "ebb"],
            },
            note: "Open a new Codex session.",
        },
        Def {
            id: "claude-desktop",
            name: "Claude Desktop",
            kind: "chat",
            config: cfg.join("Claude/claude_desktop_config.json"),
            detect: vec![cfg.join("Claude")],
            via: Via::Json {
                key: "mcpServers",
                typed: false,
            },
            note: "Restart Claude Desktop.",
        },
        Def {
            id: "lm-studio",
            name: "LM Studio (local models)",
            kind: "local",
            config: h.join(".lmstudio/mcp.json"),
            detect: vec![h.join(".lmstudio")],
            via: Via::Json {
                key: "mcpServers",
                typed: false,
            },
            note: "Start a new chat in LM Studio.",
        },
        Def {
            id: "cursor",
            name: "Cursor",
            kind: "editor",
            config: h.join(".cursor/mcp.json"),
            detect: vec![h.join(".cursor")],
            via: Via::Json {
                key: "mcpServers",
                typed: false,
            },
            note: "Reload Cursor.",
        },
        Def {
            id: "vscode",
            name: "VS Code",
            kind: "editor",
            config: cfg.join("Code/User/mcp.json"),
            detect: vec![cfg.join("Code")],
            via: Via::Json {
                key: "servers",
                typed: true,
            },
            note: "Reload VS Code.",
        },
        Def {
            id: "windsurf",
            name: "Windsurf",
            kind: "editor",
            config: h.join(".codeium/windsurf/mcp_config.json"),
            detect: vec![h.join(".codeium")],
            via: Via::Json {
                key: "mcpServers",
                typed: false,
            },
            note: "Restart Windsurf.",
        },
        Def {
            id: "gemini-cli",
            name: "Gemini CLI",
            kind: "agent",
            config: h.join(".gemini/settings.json"),
            detect: vec![h.join(".gemini")],
            via: Via::Json {
                key: "mcpServers",
                typed: false,
            },
            note: "Start a new Gemini CLI session.",
        },
    ]
}

fn is_executable(p: &Path) -> bool {
    let Ok(m) = p.metadata() else { return false };
    if !m.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        m.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// PATH + GUI uygulamalarında eksik olan yaygın kurulum klasörleri.
/// Boş ve göreli PATH girdileri (çalışma dizinini seçtirir) yok sayılır; dosya çalıştırılabilir olmalı.
pub fn find_cli(name: &str) -> Option<PathBuf> {
    let h = home();
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for extra in [
        ".local/bin",
        ".npm-global/bin",
        ".bun/bin",
        ".cargo/bin",
        ".volta/bin",
    ] {
        dirs.push(h.join(extra));
    }
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    dirs.into_iter()
        .filter(|d| d.is_absolute())
        .map(|d| d.join(&exe))
        .find(|p| is_executable(p))
}

/// Yapılandırmayı okur: (JSON değeri, dosyanın özgün metni). Dosya yoksa ({}, None).
fn load(path: &Path) -> Result<(Value, Option<String>)> {
    match std::fs::read_to_string(path) {
        Ok(s) if s.trim().is_empty() => Ok((json!({}), Some(s))),
        Ok(s) => serde_json::from_str(&s).map(|v| (v, Some(s))).map_err(|_| {
            Error::Guard(format!(
                "{} is not valid JSON (comments?); add the server by hand",
                path.display()
            ))
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((json!({}), None)),
        Err(e) => Err(e.into()),
    }
}

fn read_json(path: &Path) -> Result<Value> {
    load(path).map(|(v, _)| v)
}

/// Girdi Ebb'in kendi girdisi mi? Komut dosyası adı `ebb` ile başlar, argümanlar tam olarak ["mcp"],
/// ve `command`/`args`/`type` dışında anahtar yok (kullanıcının `env` vb. eklediği girdiye dokunulmaz).
fn is_ours(e: &Value) -> bool {
    let name = e
        .get("command")
        .and_then(Value::as_str)
        .and_then(|c| Path::new(c).file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let args_ok = e
        .get("args")
        .and_then(Value::as_array)
        .is_some_and(|a| a.len() == 1 && a[0] == "mcp");
    let keys_ok = e.as_object().is_some_and(|o| {
        o.keys()
            .all(|k| matches!(k.as_str(), "command" | "args" | "type"))
    });
    name.starts_with("ebb") && args_ok && keys_ok
}

fn reject_symlink(p: &Path) -> Result<()> {
    if p.symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err(Error::Guard(format!(
            "{} is a symlink; Ebb will not write through it. Add the server to the target file by hand",
            p.display()
        )));
    }
    Ok(())
}

/// Atomik yazım: yedek (ilk yedek korunur) → aynı dizinde geçici dosya → özgün içerik değişmediyse yeniden adlandır.
fn write_config(path: &Path, root: &Value, original: Option<&str>) -> Result<Option<PathBuf>> {
    reject_symlink(path)?;
    let backup = PathBuf::from(format!("{}.ebb-backup", path.display()));
    reject_symlink(&backup)?;
    let dir = path
        .parent()
        .ok_or_else(|| Error::Guard("config has no parent folder".into()))?;
    std::fs::create_dir_all(dir)?;
    if backup.exists() && !backup.is_file() {
        return Err(Error::Guard(format!(
            "{} exists but is not a file",
            backup.display()
        )));
    }
    let made_backup = if original.is_some() && !backup.exists() {
        std::fs::copy(path, &backup)?;
        Some(backup)
    } else {
        None
    };
    let tmp = dir.join(format!(
        ".{}.ebb-tmp-{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("cfg"),
        std::process::id()
    ));
    std::fs::write(&tmp, serde_json::to_string_pretty(root)? + "\n")?;
    if let Ok(m) = path.metadata() {
        let _ = std::fs::set_permissions(&tmp, m.permissions());
    }
    // Başka biri bu arada yazdıysa onun değişikliğini ezme.
    let now = std::fs::read_to_string(path).ok();
    if now.as_deref() != original {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::Guard(format!(
            "{} changed while Ebb was editing it; try again",
            path.display()
        )));
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    Ok(made_backup)
}

fn is_connected(d: &Def) -> bool {
    match &d.via {
        Via::Json { key, .. } => read_json(&d.config)
            .ok()
            .and_then(|v| v.get(*key)?.get("ebb").cloned())
            .is_some(),
        Via::Cli { .. } if d.id == "codex" => {
            std::fs::read_to_string(&d.config).is_ok_and(|s| s.contains("[mcp_servers.ebb]"))
        }
        Via::Cli { .. } => read_json(&d.config)
            .ok()
            .and_then(|v| v.get("mcpServers")?.get("ebb").cloned())
            .is_some(),
    }
}

pub fn clients() -> Vec<ClientInfo> {
    defs()
        .into_iter()
        .map(|d| {
            let cli_found = matches!(&d.via, Via::Cli { bin, .. } if find_cli(bin).is_some());
            ClientInfo {
                id: d.id,
                name: d.name,
                kind: d.kind,
                installed: cli_found || d.detect.iter().any(|p| p.exists()),
                connected: is_connected(&d),
                config: Some(d.config.clone()),
                note: d.note,
            }
        })
        .collect()
}

fn def(id: &str) -> Result<Def> {
    defs()
        .into_iter()
        .find(|d| d.id == id)
        .ok_or_else(|| Error::NotFound(format!("client {id}")))
}

fn run(bin: &str, args: &[String]) -> Result<()> {
    let path =
        find_cli(bin).ok_or_else(|| Error::NotFound(format!("`{bin}` command not found")))?;
    let mut child = Command::new(path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Trash(format!("`{bin}` did not finish within 30s")));
        }
        std::thread::sleep(Duration::from_millis(40));
    };
    if status.success() {
        return Ok(());
    }
    let mut msg = String::new();
    if let Some(mut e) = child.stderr.take() {
        use std::io::Read;
        let _ = e.read_to_string(&mut msg);
    }
    Err(Error::Trash(msg.trim().to_string()))
}

fn entry(typed: bool, command: &Path) -> Value {
    let mut m = Map::new();
    if typed {
        m.insert("type".into(), json!("stdio"));
    }
    m.insert("command".into(), json!(command.to_string_lossy()));
    m.insert("args".into(), json!(["mcp"]));
    Value::Object(m)
}

fn noop(id: &str, d: &Def, note: &'static str) -> Outcome {
    Outcome {
        id: id.into(),
        changed: None,
        backup: None,
        note: if note.is_empty() { d.note } else { note },
    }
}

/// Mevcut `ebb` girdisi (Claude Code için ~/.claude.json, JSON istemcileri için kendi dosyaları).
fn existing_entry(d: &Def) -> Option<Value> {
    let key = match &d.via {
        Via::Json { key, .. } => *key,
        Via::Cli { .. } if d.id == "codex" => return None,
        Via::Cli { .. } => "mcpServers",
    };
    read_json(&d.config)
        .ok()
        .and_then(|v| v.get(key)?.get("ebb").cloned())
}

/// `command`: `ebb` ikilisinin mutlak yolu.
pub fn connect(id: &str, command: &Path) -> Result<Outcome> {
    let d = def(id)?;
    let typed = matches!(&d.via, Via::Json { typed: true, .. });
    let wanted = entry(typed, command);
    let existing = existing_entry(&d);
    if let Some(e) = &existing {
        if e == &wanted {
            return Ok(noop(id, &d, "Already connected."));
        }
        if !is_ours(e) {
            return Err(Error::Guard(format!(
                "{id} already has a server named 'ebb' that Ebb did not create; rename or remove it first"
            )));
        }
    }
    match &d.via {
        Via::Cli { bin, add, remove } => {
            // Yalnızca gerçekten bir girdi varsa kaldır; böylece add başarısız olursa kaybedecek bir şey yok.
            if existing.is_some() || is_connected(&d) {
                run(
                    bin,
                    &remove.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                )?;
            }
            run(bin, &add(command))?;
            Ok(Outcome {
                id: id.into(),
                changed: Some(d.config.clone()),
                backup: None,
                note: d.note,
            })
        }
        Via::Json { key, .. } => {
            let (mut root, original) = load(&d.config)?;
            let obj = root
                .as_object_mut()
                .ok_or_else(|| Error::Guard("config is not a JSON object".into()))?;
            let servers = obj.entry(*key).or_insert_with(|| json!({}));
            servers
                .as_object_mut()
                .ok_or_else(|| Error::Guard(format!("`{key}` is not an object")))?
                .insert("ebb".into(), wanted);
            let backup = write_config(&d.config, &root, original.as_deref())?;
            Ok(Outcome {
                id: id.into(),
                changed: Some(d.config),
                backup,
                note: d.note,
            })
        }
    }
}

pub fn disconnect(id: &str) -> Result<Outcome> {
    let d = def(id)?;
    if let Some(e) = existing_entry(&d)
        && !is_ours(&e)
    {
        return Err(Error::Guard(format!(
            "{id} has a server named 'ebb' that Ebb did not create; not removing it"
        )));
    }
    match &d.via {
        Via::Cli { bin, remove, .. } => {
            if !is_connected(&d) {
                return Ok(noop(id, &d, "Was not connected."));
            }
            run(
                bin,
                &remove.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            )?;
            Ok(Outcome {
                id: id.into(),
                changed: Some(d.config),
                backup: None,
                note: "Disconnected.",
            })
        }
        Via::Json { key, .. } => {
            let (mut root, original) = load(&d.config)?;
            let removed = root
                .get_mut(*key)
                .and_then(Value::as_object_mut)
                .and_then(|m| m.shift_remove("ebb"))
                .is_some();
            if !removed {
                return Ok(noop(id, &d, "Was not connected."));
            }
            let backup = write_config(&d.config, &root, original.as_deref())?;
            Ok(Outcome {
                id: id.into(),
                changed: Some(d.config),
                backup,
                note: "Disconnected.",
            })
        }
    }
}

/// Çıkışta (hata dahil) çocuğu ve varsa tüm süreç grubunu öldürür.
struct Reaper(std::process::Child);
impl Drop for Reaper {
    fn drop(&mut self) {
        #[cfg(unix)]
        // SAFETY: yalnızca kendi başlattığımız sürecin grubuna SIGKILL gönderilir.
        unsafe {
            libc::kill(-(self.0.id() as i32), libc::SIGKILL);
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// `ebb mcp`'yi gerçekten başlatır, initialize + tools/list ister; kaç araç döndüğünü söyler.
pub fn selftest(command: &Path) -> Result<usize> {
    let mut cmd = Command::new(command);
    cmd.arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = Reaper(cmd.spawn()?);
    let mut stdin = child
        .0
        .stdin
        .take()
        .ok_or_else(|| Error::Trash("no stdin".into()))?;
    let stdout = child
        .0
        .stdout
        .take()
        .ok_or_else(|| Error::Trash("no stdout".into()))?;
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2025-06-18","capabilities":{{}},"clientInfo":{{"name":"ebb-selftest","version":"0"}}}}}}"#
    )?;
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":2,"method":"tools/list"}}"#)?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(|l| l.ok()) {
            if let Ok(v) = serde_json::from_str::<Value>(&line)
                && v.get("id") == Some(&json!(2))
            {
                let _ = tx.send(v["result"]["tools"].as_array().map_or(0, |a| a.len()));
                return;
            }
        }
    });
    rx.recv_timeout(Duration::from_secs(8))
        .map_err(|_| Error::Trash("`ebb mcp` did not answer within 8s".into()))
}
