//! Koruma listesi: bu yollara hiçbir plan dokunamaz.
use crate::{Error, Result};
use std::path::{Path, PathBuf};

fn protected_roots() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = if cfg!(windows) {
        [
            "C:\\Windows",
            "C:\\Program Files",
            "C:\\Program Files (x86)",
            "C:\\ProgramData",
        ]
        .map(PathBuf::from)
        .into()
    } else {
        [
            "/System",
            "/usr",
            "/bin",
            "/sbin",
            "/etc",
            "/boot",
            "/dev",
            "/proc",
            "/sys",
            "/lib",
            "/lib64",
            "/opt",
            "/private/etc",
        ]
        .map(PathBuf::from)
        .into()
    };
    if let Some(h) = dirs::home_dir() {
        for d in [
            ".ssh",
            ".gnupg",
            "Library/Keychains",
            "Library/CloudStorage",
            "Library/Mobile Documents",
            "OneDrive",
            "Dropbox",
            "Google Drive",
            "iCloud Drive",
        ] {
            v.push(h.join(d));
        }
    }
    v
}

/// Kendisi silinemeyecek (ama altındaki önbellek silinebilir) kullanıcı klasörleri.
fn exact_only() -> Vec<PathBuf> {
    let Some(h) = dirs::home_dir() else {
        return vec![];
    };
    let mut v = vec![
        h.clone(),
        PathBuf::from("/"),
        PathBuf::from("/Users"),
        PathBuf::from("/Applications"),
        PathBuf::from("/home"),
        PathBuf::from("/var"),
        PathBuf::from("/tmp"),
    ];
    for d in [
        "Desktop",
        "Documents",
        "Downloads",
        "Pictures",
        "Music",
        "Movies",
        "Videos",
        "Library",
        "AppData",
        ".config",
        ".local",
        ".cache",
    ] {
        v.push(h.join(d));
    }
    v
}

pub fn check(path: &Path) -> Result<()> {
    let p = path
        .canonicalize()
        .map_err(|_| Error::NotFound(path.display().to_string()))?;
    let deny = |why: &str| Err(Error::Guard(format!("{} ({why})", p.display())));
    if exact_only()
        .iter()
        .any(|e| e.canonicalize().map(|c| c == p).unwrap_or(false))
    {
        return deny("system or user root folder");
    }
    if let Some(h) = dirs::home_dir().and_then(|h| h.canonicalize().ok())
        && h.starts_with(&p)
    {
        return deny("contains your home folder");
    }
    for root in protected_roots() {
        let r = root.canonicalize().unwrap_or(root);
        if p.starts_with(&r) {
            return deny("protected location");
        }
        if r.starts_with(&p) {
            return deny("contains a protected location");
        }
    }
    if dirty_git(&p) {
        return deny("git repository with uncommitted changes");
    }
    Ok(())
}

/// Yol bir git çalışma ağacındaysa ve `git status --porcelain` o yol için boş değilse true.
fn dirty_git(p: &Path) -> bool {
    let start = if p.is_dir() {
        p
    } else {
        p.parent().unwrap_or(p)
    };
    if !start.ancestors().any(|a| a.join(".git").exists()) {
        return false;
    }
    std::process::Command::new("git")
        .arg("-C")
        .arg(start)
        .args(["status", "--porcelain", "--"])
        .arg(p)
        .output()
        // git çalışmazsa depo "kirli" sayılır (güvenli taraf): .git var ama durum okunamıyorsa dokunma.
        .map(|o| !o.status.success() || !o.stdout.is_empty())
        .unwrap_or(true)
}
