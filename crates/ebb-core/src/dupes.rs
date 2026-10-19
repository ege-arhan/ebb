//! Kopya dosya bulucu: boyuta göre grupla → kısmi hash (ilk + son 64 KiB) → tam blake3.
//! Symlink'ler, boş dosyalar ve aynı inode'un ek yolları (hardlink) atlanır; mount sınırı geçilmez.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{File, Metadata};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DupGroup {
    /// Tam içeriğin blake3 özeti (hex).
    pub hash: String,
    /// Her kopyanın mantıksal boyutu.
    pub size: u64,
    /// files[0] korunur; optimize diğerlerini onun klonu yapar.
    pub files: Vec<PathBuf>,
    pub reclaimable: u64,
    pub optimized: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct DupProgress {
    pub stage: &'static str,
    pub files: u64,
    pub bytes: u64,
}

const CHUNK: u64 = 64 * 1024;
const BUF: usize = 256 * 1024;

#[derive(Default, Clone, Copy, Debug)]
struct Meta {
    ok: bool,
    size: u64,
    hl: Option<(u64, u64)>,
}
type Walk = jwalk::WalkDirGeneric<((), Meta)>;

#[cfg(unix)]
fn hardlink_id(m: &Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    (m.nlink() > 1).then(|| (m.dev(), m.ino()))
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

/// Groups of identical regular files under `root` with logical size >= min_size, largest `reclaimable` first.
/// `reclaimable` = size * (files.len() - 1). Stages: "listing", "hashing".
pub fn find_duplicates(
    root: &Path,
    min_size: u64,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(DupProgress),
) -> Result<Vec<DupGroup>> {
    let root = root.canonicalize()?;
    let root_dev = device(&std::fs::symlink_metadata(&root)?);
    let min = min_size.max(1);
    // lstat jwalk'ın paralel iş parçacıklarında yapılır (scan.rs ile aynı desen).
    let walker = Walk::new(&root)
        .skip_hidden(false)
        .follow_links(false)
        .sort(false)
        .process_read_dir(move |_, _, _, children| {
            for child in children.iter_mut().flatten() {
                let Ok(m) = child.metadata() else { continue };
                if m.is_dir() {
                    if device(&m) != root_dev {
                        child.read_children_path = None;
                    }
                } else if m.is_file() && m.len() >= min {
                    child.client_state = Meta {
                        ok: true,
                        size: m.len(),
                        hl: hardlink_id(&m),
                    };
                }
            }
        });

    // ponytail: eşik üstündeki her yol bellekte tutulur (2M dosya ≈ birkaç yüz MB); gerekirse iki geçişli listeleme.
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    let mut seen: HashSet<(u64, u64)> = HashSet::new();
    let (mut files, mut bytes) = (0u64, 0u64);
    for (i, entry) in walker.into_iter().enumerate() {
        if i % 1024 == 0 && cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let Ok(entry) = entry else { continue };
        let m = entry.client_state;
        if !m.ok {
            continue;
        }
        if let Some(id) = m.hl
            && !seen.insert(id)
        {
            continue; // aynı inode, bir kez say
        }
        by_size.entry(m.size).or_default().push(entry.path());
        files += 1;
        bytes += m.size;
        if files % 5000 == 0 {
            on_progress(DupProgress {
                stage: "listing",
                files,
                bytes,
            });
        }
    }
    on_progress(DupProgress {
        stage: "listing",
        files,
        bytes,
    });
    drop(seen);

    let cands: Vec<(u64, PathBuf)> = by_size
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .flat_map(|(s, v)| v.into_iter().map(move |p| (s, p)))
        .collect();

    let mut done = (0u64, 0u64);
    let mut last = Instant::now();
    let mut tick = |f: u64, n: u64, force: bool| {
        done.0 += f;
        done.1 += n;
        if force || last.elapsed() > Duration::from_millis(100) {
            last = Instant::now();
            on_progress(DupProgress {
                stage: "hashing",
                files: done.0,
                bytes: done.1,
            });
        }
    };

    // 1. tur: kısmi hash. ≤128 KiB dosyalarda bu zaten tam içeriğin hash'idir.
    let partial = hash_all(&cands, false, cancel, &mut tick)?;
    let mut finals: Vec<(u64, blake3::Hash, Vec<PathBuf>)> = vec![];
    let mut big: Vec<(u64, PathBuf)> = vec![];
    for (size, hash, paths) in regroup(cands, partial) {
        if size <= 2 * CHUNK {
            finals.push((size, hash, paths));
        } else {
            big.extend(paths.into_iter().map(|p| (size, p)));
        }
    }
    // 2. tur: yalnız kalan büyük adaylar için tam hash.
    let full = hash_all(&big, true, cancel, &mut tick)?;
    finals.extend(regroup(big, full));
    tick(0, 0, true);

    let mut groups: Vec<DupGroup> = finals
        .into_iter()
        .map(|(size, hash, mut files)| {
            files.sort();
            DupGroup {
                hash: hash.to_hex().to_string(),
                size,
                reclaimable: size * (files.len() as u64 - 1),
                files,
                optimized: false,
            }
        })
        .collect();
    groups.sort_by(|a, b| {
        b.reclaimable
            .cmp(&a.reclaimable)
            .then_with(|| a.hash.cmp(&b.hash))
    });
    Ok(groups)
}

/// (boyut, hash) aynı olanları toplar; tek kalanları atar.
fn regroup(
    items: Vec<(u64, PathBuf)>,
    hashes: Vec<Option<blake3::Hash>>,
) -> Vec<(u64, blake3::Hash, Vec<PathBuf>)> {
    let mut m: HashMap<(u64, blake3::Hash), Vec<PathBuf>> = HashMap::new();
    for ((size, p), h) in items.into_iter().zip(hashes) {
        if let Some(h) = h {
            m.entry((size, h)).or_default().push(p);
        }
    }
    m.into_iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|((s, h), v)| (s, h, v))
        .collect()
}

/// Dosyaları iş parçacıklarında hash'ler; ilerleme ana iş parçacığında bildirilir. Okunamayan dosya → None.
fn hash_all(
    items: &[(u64, PathBuf)],
    full: bool,
    cancel: &AtomicBool,
    tick: &mut impl FnMut(u64, u64, bool),
) -> Result<Vec<Option<blake3::Hash>>> {
    let mut out = vec![None; items.len()];
    if items.is_empty() {
        return Ok(out);
    }
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .clamp(2, 16)
        .min(items.len());
    std::thread::scope(|s| {
        let (tx, rx) = mpsc::channel();
        for _ in 0..threads {
            let (tx, next) = (tx.clone(), &next);
            s.spawn(move || {
                let mut buf = vec![0u8; BUF];
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= items.len() || cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let (size, path) = &items[i];
                    let h = if full {
                        hash_file(path, &mut buf)
                    } else {
                        partial_hash(path, *size, &mut buf)
                    };
                    if tx.send((i, h.ok())).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        for (i, h) in rx {
            out[i] = h;
            let size = items[i].0;
            tick(1, if full { size } else { size.min(2 * CHUNK) }, false);
        }
    });
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    Ok(out)
}

/// Dosyanın tamamının blake3 özeti; içerik sabit bir tampon üzerinden akar.
pub(crate) fn hash_file(path: &Path, buf: &mut [u8]) -> io::Result<blake3::Hash> {
    let mut f = File::open(path)?;
    let mut h = blake3::Hasher::new();
    loop {
        match f.read(buf) {
            Ok(0) => return Ok(h.finalize()),
            Ok(n) => {
                h.update(&buf[..n]);
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

fn partial_hash(path: &Path, size: u64, buf: &mut [u8]) -> io::Result<blake3::Hash> {
    if size <= 2 * CHUNK {
        return hash_file(path, buf);
    }
    let buf = &mut buf[..CHUNK as usize];
    let mut f = File::open(path)?;
    let mut h = blake3::Hasher::new();
    f.read_exact(buf)?;
    h.update(buf);
    f.seek(SeekFrom::Start(size - CHUNK))?;
    f.read_exact(buf)?;
    h.update(buf);
    Ok(h.finalize())
}
