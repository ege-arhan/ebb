//! Araçların kendi temizlik komutları. Argümanlar sabittir; kullanıcı girdisinden üretilmez.
use crate::{Error, Result, connect::find_cli, ops::disk_usage};
use serde::Serialize;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Serialize, Clone, Debug)]
pub struct PruneAction {
    pub id: &'static str,
    pub label: String,
    pub description: String,
    pub command: String,
    pub estimate: Option<u64>,
    pub installed: bool,
    pub safety: &'static str,
}

#[derive(Serialize, Clone, Debug)]
pub struct PruneResult {
    pub id: String,
    pub ok: bool,
    pub output: String,
    pub before: u64,
    pub after: u64,
}

struct Def {
    id: &'static str,
    label: &'static str,
    description: &'static str,
    bin: &'static str,
    /// Her biri sırayla çalışan sabit komutlar (program adı hariç argümanlar).
    steps: &'static [&'static [&'static str]],
    safety: &'static str,
}

const DEFS: &[Def] = &[
    Def {
        id: "docker",
        label: "Docker build cache and dangling images",
        description: "Removes unused build cache and untagged images. Containers, volumes and tagged images are kept.",
        bin: "docker",
        steps: &[&["builder", "prune", "-f"], &["image", "prune", "-f"]],
        safety: "safe",
    },
    Def {
        id: "homebrew",
        label: "Homebrew old versions and downloads",
        description: "Removes outdated formula versions and cached downloads.",
        bin: "brew",
        steps: &[&["cleanup", "-s", "--prune=all"]],
        safety: "safe",
    },
    Def {
        id: "xcode-simulators",
        label: "Unavailable Xcode simulators",
        description: "Deletes simulators whose runtime is no longer installed.",
        bin: "xcrun",
        steps: &[&["simctl", "delete", "unavailable"]],
        safety: "safe",
    },
    Def {
        id: "npm",
        label: "npm cache",
        description: "Clears the npm download cache; packages are fetched again when needed.",
        bin: "npm",
        steps: &[&["cache", "clean", "--force"]],
        safety: "safe",
    },
    Def {
        id: "pnpm",
        label: "pnpm store",
        description: "Removes packages from the pnpm store that no project references.",
        bin: "pnpm",
        steps: &[&["store", "prune"]],
        safety: "safe",
    },
    Def {
        id: "yarn",
        label: "Yarn cache",
        description: "Clears the Yarn download cache.",
        bin: "yarn",
        steps: &[&["cache", "clean"]],
        safety: "safe",
    },
    Def {
        id: "pip",
        label: "pip cache",
        description: "Clears the pip wheel and HTTP cache.",
        bin: "pip",
        steps: &[&["cache", "purge"]],
        safety: "safe",
    },
    Def {
        id: "cargo",
        label: "Cargo registry cache",
        description: "Clears cargo's registry and git caches via cargo-cache.",
        bin: "cargo-cache",
        steps: &[&["-a"]],
        safety: "review",
    },
    Def {
        id: "go",
        label: "Go build cache",
        description: "Clears the Go build cache.",
        bin: "go",
        steps: &[&["clean", "-cache"]],
        safety: "safe",
    },
    Def {
        id: "conda",
        label: "conda packages and caches",
        description: "Removes unused conda packages, tarballs and index caches.",
        bin: "conda",
        steps: &[&["clean", "--all", "-y"]],
        safety: "review",
    },
];

fn command_for(d: &Def, step: &[&str]) -> Option<Command> {
    let bin = find_cli(d.bin)?;
    let mut c = Command::new(bin);
    c.args(step)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    Some(c)
}

/// Kullanıcıya gösterilen tam komut metni (cargo-cache `cargo cache -a` olarak gösterilir).
fn shown(d: &Def) -> String {
    let prog = if d.bin == "cargo-cache" {
        "cargo cache"
    } else {
        d.bin
    };
    d.steps
        .iter()
        .map(|s| format!("{prog} {}", s.join(" ")))
        .collect::<Vec<_>>()
        .join(" && ")
}

/// Zaman aşımlı çalıştırma: bitince (başarı, çıktı); süre dolarsa çocuk öldürülür.
fn run_cmd(mut c: Command, timeout: Duration) -> (bool, String) {
    let Ok(mut child) = c.spawn() else {
        return (false, "could not start".into());
    };
    let drain = |s: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut b = String::new();
            if let Some(mut s) = s {
                let _ = s.read_to_string(&mut b);
            }
            b
        })
    };
    let out = drain(child.stdout.take().map(|s| Box::new(s) as _));
    let err = drain(child.stderr.take().map(|s| Box::new(s) as _));
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s.success()),
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let mut text = out.join().unwrap_or_default();
    text.push_str(&err.join().unwrap_or_default());
    if status.is_none() {
        text.push_str("\ntimed out after 120 s");
    }
    (status == Some(true), text)
}

fn tail(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.trim().to_string();
    }
    let mut i = s.len() - max;
    while !s.is_char_boundary(i) {
        i += 1;
    }
    s[i..].trim().to_string()
}

/// "1.2GB", "340.5MB" gibi metni bayta çevirir.
fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let n = s.find(|c: char| c.is_alphabetic())?;
    let v: f64 = s[..n].trim().parse().ok()?;
    let mul = match s[n..].to_ascii_uppercase().trim_end_matches('B') {
        "" => 1.0,
        "K" | "KI" => 1e3,
        "M" | "MI" => 1e6,
        "G" | "GI" => 1e9,
        "T" | "TI" => 1e12,
        _ => return None,
    };
    Some((v * mul) as u64)
}

fn capture(d: &Def, args: &[&str]) -> Option<String> {
    let mut c = command_for(d, args)?;
    c.stderr(Stdio::null());
    let (ok, out) = run_cmd(c, Duration::from_secs(10));
    ok.then_some(out)
}

fn estimate(d: &Def) -> Option<u64> {
    match d.id {
        "docker" => {
            // `docker system df --format json`: satır başına bir JSON; Reclaimable "1.2GB (45%)".
            let out = capture(d, &["system", "df", "--format", "json"])?;
            let total = out
                .lines()
                .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
                .filter(|v| matches!(v["Type"].as_str(), Some("Images" | "Build Cache")))
                .filter_map(|v| parse_size(v["Reclaimable"].as_str()?.split(' ').next()?))
                .sum::<u64>();
            Some(total)
        }
        "homebrew" => {
            // `brew cleanup -n` sonunda "This operation would free approximately 1.2GB".
            let out = capture(d, &["cleanup", "-n", "-s", "--prune=all"])?;
            let l = out.lines().find(|l| l.contains("free approximately"))?;
            parse_size(
                l.rsplit("approximately")
                    .next()?
                    .trim()
                    .trim_end_matches('.'),
            )
        }
        _ => None,
    }
}

/// Yalnızca aracı kurulu olan eylemler.
pub fn available() -> Vec<PruneAction> {
    DEFS.iter()
        .filter(|d| find_cli(d.bin).is_some())
        .map(|d| PruneAction {
            id: d.id,
            label: d.label.into(),
            description: d.description.into(),
            command: shown(d),
            estimate: estimate(d),
            installed: true,
            safety: d.safety,
        })
        .collect()
}

pub fn run(id: &str) -> Result<PruneResult> {
    let d = DEFS
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| Error::NotFound(format!("prune action `{id}`")))?;
    if find_cli(d.bin).is_none() {
        return Err(Error::NotFound(format!("`{}` command not found", d.bin)));
    }
    let free = || disk_usage().map_or(0, |u| u.free);
    let before = free();
    let deadline = Instant::now() + Duration::from_secs(120);
    let (mut ok, mut output) = (true, String::new());
    for step in d.steps {
        let left = deadline.saturating_duration_since(Instant::now());
        let Some(c) = command_for(d, step) else {
            return Err(Error::NotFound(format!("`{}` command not found", d.bin)));
        };
        let (o, text) = run_cmd(c, left);
        ok &= o;
        output.push_str(&text);
        output.push('\n');
        if !o {
            break;
        }
    }
    Ok(PruneResult {
        id: id.into(),
        ok,
        output: tail(&output, 2048),
        before,
        after: free(),
    })
}
