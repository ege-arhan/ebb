//! CLI, MCP ve GUI'nin ortak kullandığı üst düzey işlemler.
use crate::{
    Result, catalog, guard,
    journal::Journal,
    scan::{self, Node, ScanOptions},
    snapshot::{self, Growth},
};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub fn scan_path(path: &Path) -> Result<Node> {
    scan_with(path, &AtomicBool::new(false), |_| {})
}

/// İptal bayrağı ve ilerleme geri çağrısıyla tarama (CLI iğnesi ve GUI için).
pub fn scan_with(
    path: &Path,
    cancel: &AtomicBool,
    on_progress: impl FnMut(scan::Progress),
) -> Result<Node> {
    scan::scan(path, &ScanOptions::default(), cancel, on_progress)
}

#[derive(Serialize, Debug)]
pub struct TopItem {
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
}

/// Doğrudan çocuklar (büyükten küçüğe) ve ağaçtaki en büyük `n` dosya.
pub fn top(node: &Node, n: usize) -> (Vec<TopItem>, Vec<TopItem>) {
    let dirs = node
        .children
        .iter()
        .filter(|c| !c.name.starts_with("(other"))
        .take(n)
        .map(|c| TopItem {
            path: c.path.clone(),
            size: c.size,
            is_dir: c.is_dir,
        })
        .collect();
    let mut files = vec![];
    fn walk(n: &Node, out: &mut Vec<TopItem>) {
        for c in &n.children {
            if c.is_dir {
                walk(c, out);
            } else if !c.name.starts_with("(other") {
                out.push(TopItem {
                    path: c.path.clone(),
                    size: c.size,
                    is_dir: false,
                });
            }
        }
    }
    walk(node, &mut files);
    files.sort_by_key(|a| std::cmp::Reverse(a.size));
    files.truncate(n);
    (dirs, files)
}

#[derive(Serialize, Debug)]
pub struct WhyReport {
    pub root: PathBuf,
    pub total: u64,
    /// Önceki anlık görüntünün zamanı; yoksa ilk çalıştırma.
    pub since: Option<i64>,
    pub total_before: Option<u64>,
    pub growth: Vec<Growth>,
}

/// Önceki taramayla kıyaslar, sonra yeni anlık görüntüyü kaydeder.
pub fn why(journal: &Journal, path: &Path, top_n: usize) -> Result<WhyReport> {
    let node = scan_path(path)?;
    let key = node.path.to_string_lossy().into_owned();
    let now = snapshot::flatten(&node, 3);
    let prev = journal.latest_snapshot(&key)?;
    let report = WhyReport {
        root: node.path.clone(),
        total: node.size,
        since: prev.as_ref().map(|p| p.0),
        total_before: prev.as_ref().and_then(|p| p.1.get(&key).copied()),
        growth: prev
            .as_ref()
            .map(|p| snapshot::growth(&p.1, &now, top_n))
            .unwrap_or_default(),
    };
    journal.save_snapshot(&key, &now)?;
    Ok(report)
}

#[derive(Serialize, Debug)]
pub struct Explain {
    pub path: PathBuf,
    pub exists: bool,
    pub size: u64,
    pub files: u64,
    /// Koruma listesi engeli varsa nedeni.
    pub protected: Option<String>,
    pub catalog: Vec<(String, String, catalog::Safety)>,
}

pub fn explain(path: &Path) -> Explain {
    let exists = path.exists();
    let (size, files) = if exists { scan::measure(path) } else { (0, 0) };
    let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let catalog = catalog::builtin()
        .into_iter()
        .filter(|e| {
            catalog::resolve(e)
                .iter()
                .any(|p| canon.starts_with(p.canonicalize().unwrap_or_else(|_| p.clone())))
        })
        .map(|e| (e.id, e.label.en, e.safety))
        .collect();
    Explain {
        path: canon,
        exists,
        size,
        files,
        protected: guard::check(path).err().map(|e| e.to_string()),
        catalog,
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct DiskUsage {
    pub total: u64,
    pub free: u64,
    pub mount: String,
}

/// Ev klasörünün bulunduğu diskin doluluğu.
pub fn disk_usage() -> Option<DiskUsage> {
    let home = dirs::home_dir()?;
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| home.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| DiskUsage {
            total: d.total_space(),
            free: d.available_space(),
            mount: d.mount_point().to_string_lossy().into_owned(),
        })
}
