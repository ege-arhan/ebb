//! Genel sistem dosyaları için bulucular: büyük ve uzun süredir açılmamış dosyalar, eski kurulum dosyaları.
//! Yalnızca okur; silme her zaman Plan → Uygula akışından geçer.
use crate::{Error, Result, guard};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

#[derive(Serialize, Clone, Debug)]
pub struct FoundFile {
    pub path: PathBuf,
    pub size: u64,
    /// Son değişiklikten bu yana gün.
    pub age_days: u64,
    /// "large" | "installer"
    pub kind: &'static str,
}

const INSTALLER_EXT: &[&str] = &[
    "dmg", "pkg", "mpkg", "iso", "exe", "msi", "msix", "appimage", "deb", "rpm", "xip",
];
/// Taramada atlanan, büyük ama "dosyan" olmayan klasörler (paketler, önbellekler, sistem verisi).
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "Library",
    "AppData",
    ".cache",
    ".Trash",
    "$RECYCLE.BIN",
    ".npm",
    ".cargo",
    ".rustup",
    "target",
    ".gradle",
];

fn age_days(m: &std::fs::Metadata, now: SystemTime) -> u64 {
    let t = m
        .accessed()
        .ok()
        .into_iter()
        .chain(m.modified().ok())
        .max()
        .unwrap_or(now);
    now.duration_since(t).unwrap_or(Duration::ZERO).as_secs() / 86_400
}

/// `root` altında `min_size` bayttan büyük ve en az `min_age_days` gündür dokunulmamış dosyalar (büyükten küçüğe).
pub fn large_old_files(
    root: &Path,
    min_size: u64,
    min_age_days: u64,
    limit: usize,
    cancel: &AtomicBool,
) -> Result<Vec<FoundFile>> {
    let now = SystemTime::now();
    let mut out = vec![];
    let walker = jwalk::WalkDir::new(root)
        .skip_hidden(false)
        .follow_links(false)
        .process_read_dir(|_, _, _, children| {
            children.retain(|c| {
                c.as_ref()
                    .map(|e| {
                        !(e.file_type().is_dir() && SKIP_DIRS.iter().any(|s| e.file_name() == *s))
                    })
                    .unwrap_or(true)
            });
        });
    for (i, e) in walker.into_iter().flatten().enumerate() {
        if i % 2048 == 0 && cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        if !e.file_type().is_file() {
            continue;
        }
        let Ok(m) = e.metadata() else { continue };
        if m.len() < min_size {
            continue;
        }
        let age = age_days(&m, now);
        if age >= min_age_days {
            out.push(FoundFile {
                path: e.path(),
                size: m.len(),
                age_days: age,
                kind: "large",
            });
        }
    }
    out.sort_by_key(|f| std::cmp::Reverse(f.size));
    out.truncate(limit);
    Ok(out)
}

/// İndirilenler ve Masaüstü'ndeki, `min_age_days` günden eski kurulum dosyaları (.dmg, .pkg, .exe, .msi, .iso…).
pub fn old_installers(min_age_days: u64) -> Vec<FoundFile> {
    let now = SystemTime::now();
    let mut out = vec![];
    for dir in [dirs::download_dir(), dirs::desktop_dir()]
        .into_iter()
        .flatten()
    {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let is_installer = p
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| INSTALLER_EXT.contains(&x.to_ascii_lowercase().as_str()));
            let Ok(m) = std::fs::symlink_metadata(&p) else {
                continue;
            };
            if !is_installer || m.file_type().is_symlink() || guard::check(&p).is_err() {
                continue;
            }
            let age = age_days(&m, now);
            if age >= min_age_days {
                let size = if m.is_dir() {
                    crate::scan::measure(&p).0
                } else {
                    m.len()
                };
                out.push(FoundFile {
                    path: p,
                    size,
                    age_days: age,
                    kind: "installer",
                });
            }
        }
    }
    out.sort_by_key(|f| std::cmp::Reverse(f.size));
    out
}
