//! Paralel disk tarayıcı. Tahsis edilen boyutu (blok) ve mantıksal boyutu ayrı sayar,
//! hardlink'leri tekilleştirir, mount sınırını geçmez.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Node {
    pub name: String,
    pub path: PathBuf,
    /// Diskte tahsis edilen bayt.
    pub size: u64,
    pub logical: u64,
    pub is_dir: bool,
    /// Altındaki dosya sayısı.
    pub files: u64,
    pub children: Vec<Node>,
}

#[derive(Clone, Debug)]
pub struct ScanOptions {
    /// Bir klasörde saklanan en büyük çocuk sayısı; kalanlar "(other)" altında toplanır.
    pub max_children: usize,
    pub cross_mounts: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_children: 200,
            cross_mounts: false,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub files: u64,
    pub bytes: u64,
    pub current: PathBuf,
}

#[cfg(unix)]
fn sizes(m: &Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (m.blocks() * 512, m.len())
}
#[cfg(not(unix))]
// ponytail: Windows'ta mantıksal boyut; GetCompressedFileSizeW ile tahsis edilen boyuta v0.3'te geçilir.
fn sizes(m: &Metadata) -> (u64, u64) {
    (m.len(), m.len())
}

#[cfg(unix)]
fn hardlink_id(m: &Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    (m.nlink() > 1 && !m.is_dir()).then(|| (m.dev(), m.ino()))
}
#[cfg(not(unix))]
fn hardlink_id(_: &Metadata) -> Option<(u64, u64)> {
    None
}

#[cfg(unix)]
fn device(m: &Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    m.dev()
}
#[cfg(not(unix))]
fn device(_: &Metadata) -> u64 {
    0
}

/// jwalk'ın paralel iş parçacığında toplanan dosya meta verisi.
#[derive(Default, Clone, Copy, Debug)]
struct Meta {
    ok: bool,
    size: u64,
    logical: u64,
    hl: Option<(u64, u64)>,
}
type Walk = jwalk::WalkDirGeneric<((), Meta)>;

#[derive(Default)]
struct Agg {
    size: u64,
    logical: u64,
    files: u64,
    file_bytes: u64,
    file_logical: u64,
    dirs: Vec<PathBuf>,
    top: Vec<(String, u64, u64)>,
}

fn push_top(top: &mut Vec<(String, u64, u64)>, cap: usize, item: (String, u64, u64)) {
    top.push(item);
    if top.len() > cap * 2 + 8 {
        top.sort_by_key(|a| std::cmp::Reverse(a.1));
        top.truncate(cap);
    }
}

/// `root` altını tarar. `on_progress` her ~5000 girişte çağrılır.
pub fn scan(
    root: &Path,
    opts: &ScanOptions,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(Progress),
) -> Result<Node> {
    let root = root.canonicalize()?;
    let root_meta = std::fs::symlink_metadata(&root)?;
    if !root_meta.is_dir() {
        let (size, logical) = sizes(&root_meta);
        return Ok(Node {
            name: file_name(&root),
            path: root,
            size,
            logical,
            is_dir: false,
            files: 1,
            children: vec![],
        });
    }
    let root_dev = device(&root_meta);
    let cross = opts.cross_mounts;
    // lstat çağrıları jwalk'ın paralel iş parçacıklarında yapılır ve girişe iliştirilir;
    // tüketici döngü artık sistem çağrısı yapmaz (ev klasörü taramasında en büyük darboğazdı).
    let walker = Walk::new(&root)
        .skip_hidden(false)
        .follow_links(false)
        .sort(false)
        .process_read_dir(move |_, _, _, children| {
            for child in children.iter_mut().flatten() {
                let Ok(m) = child.metadata() else { continue };
                let (size, logical) = sizes(&m);
                child.client_state = Meta {
                    ok: true,
                    size,
                    logical,
                    hl: hardlink_id(&m),
                };
                if !cross && child.file_type().is_dir() && device(&m) != root_dev {
                    child.read_children_path = None;
                }
            }
        });

    let root_state = {
        let (size, logical) = sizes(&root_meta);
        Meta {
            ok: true,
            size,
            logical,
            hl: None,
        }
    };
    let mut aggs: HashMap<PathBuf, Agg> = HashMap::new();
    let mut seen: HashSet<(u64, u64)> = HashSet::new();
    let (mut files, mut bytes) = (0u64, 0u64);
    for (i, entry) in walker.into_iter().enumerate() {
        if i % 1024 == 0 && cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let Ok(entry) = entry else { continue }; // izin hatası vb.: atla
        let m = if entry.depth == 0 {
            root_state
        } else {
            entry.client_state
        };
        if !m.ok {
            continue;
        }
        if entry.file_type().is_dir() {
            let path = entry.path();
            let agg = aggs.entry(path.clone()).or_default();
            agg.size += m.size;
            agg.logical += m.logical;
            if entry.depth > 0 {
                aggs.entry(entry.parent_path.to_path_buf())
                    .or_default()
                    .dirs
                    .push(path);
            }
            continue;
        }
        if let Some(id) = m.hl
            && !seen.insert(id)
        {
            continue; // aynı inode, bir kez say
        }
        // Üst klasörün kaydı çoğunlukla vardır; yeni PathBuf ayırmadan bul.
        let agg = match aggs.get_mut(&*entry.parent_path) {
            Some(a) => a,
            None => aggs.entry(entry.parent_path.to_path_buf()).or_default(),
        };
        agg.size += m.size;
        agg.logical += m.logical;
        agg.files += 1;
        agg.file_bytes += m.size;
        agg.file_logical += m.logical;
        push_top(
            &mut agg.top,
            opts.max_children,
            (
                entry.file_name().to_string_lossy().into_owned(),
                m.size,
                m.logical,
            ),
        );
        files += 1;
        bytes += m.size;
        if files % 5000 == 0 {
            on_progress(Progress {
                files,
                bytes,
                current: entry.path(),
            });
        }
    }
    on_progress(Progress {
        files,
        bytes,
        current: root.clone(),
    });
    Ok(build(&root, &mut aggs, opts.max_children))
}

fn build(path: &Path, aggs: &mut HashMap<PathBuf, Agg>, cap: usize) -> Node {
    let agg = aggs.remove(path).unwrap_or_default();
    let mut size = agg.size;
    let mut logical = agg.logical;
    let mut files = agg.files;
    let mut children: Vec<Node> = Vec::with_capacity(agg.dirs.len() + agg.top.len());
    for d in &agg.dirs {
        let n = build(d, aggs, cap);
        size += n.size;
        logical += n.logical;
        files += n.files;
        children.push(n);
    }
    // agg.size tüm dosyaları içerir; `top` yalnızca gösterilen en büyükleri tutar.
    // Kırpılan dosyalar (top'a sığmayanlar) "(other)" kovasında toplanır; sayılar ağaçta tutarlı kalır.
    let dropped = agg.files - agg.top.len() as u64;
    if dropped > 0 {
        let (shown, shown_l) = agg.top.iter().fold((0, 0), |a, t| (a.0 + t.1, a.1 + t.2));
        children.push(Node {
            name: format!("(other {dropped} files)"),
            path: path.to_path_buf(),
            size: agg.file_bytes.saturating_sub(shown),
            logical: agg.file_logical.saturating_sub(shown_l),
            is_dir: false,
            files: dropped,
            children: vec![],
        });
    }
    for (name, s, l) in agg.top {
        children.push(Node {
            path: path.join(&name),
            name,
            size: s,
            logical: l,
            is_dir: false,
            files: 1,
            children: vec![],
        });
    }
    children.sort_by_key(|a| std::cmp::Reverse(a.size));
    if children.len() > cap {
        let rest = children.split_off(cap);
        let (rs, rl, rf) = rest.iter().fold((0, 0, 0), |a, n| {
            (a.0 + n.size, a.1 + n.logical, a.2 + n.files)
        });
        children.push(Node {
            name: format!("(other {})", rest.len()),
            path: path.to_path_buf(),
            size: rs,
            logical: rl,
            is_dir: false,
            files: rf,
            children: vec![],
        });
    }
    Node {
        name: file_name(path),
        path: path.to_path_buf(),
        size,
        logical,
        is_dir: true,
        files,
        children,
    }
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string_lossy().into_owned())
}

/// Hızlı toplam: katalog bulguları için. (tahsis edilen bayt, dosya sayısı)
pub fn measure(path: &Path) -> (u64, u64) {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return (0, 0);
    };
    if !meta.is_dir() {
        return (sizes(&meta).0, 1);
    }
    let mut seen = HashSet::new();
    let (mut total, mut files) = (0, 0);
    for e in jwalk::WalkDir::new(path)
        .skip_hidden(false)
        .follow_links(false)
        .into_iter()
        .flatten()
    {
        let Ok(m) = e.metadata() else { continue };
        if let Some(id) = hardlink_id(&m)
            && !seen.insert(id)
        {
            continue;
        }
        total += sizes(&m).0;
        if !e.file_type().is_dir() {
            files += 1;
        }
    }
    (total, files)
}
