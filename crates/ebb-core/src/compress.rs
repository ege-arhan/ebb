//! Akıllı sıkıştırma: klasörü dosya türüne göre çözümler, örnekleme ile gerçek kazancı tahmin eder
//! ve yalnızca değecek dosyaları sıkıştırır. İçerik her zaman byte-byte aynı kalır.
use crate::{Error, Result, guard, optimize, scan};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Bundan küçük dosyalar sıkıştırmaya değmez (APFS/NTFS blok boyutu).
const MIN_SIZE: u64 = 4096;
/// Örnek başına okunan bayt ve kategori başına örnek dosya sayısı.
const SAMPLE_BYTES: usize = 128 * 1024;
const SAMPLE_FILES: usize = 40;
/// `analyze` için "değer" eşiği.
const DEFAULT_MIN_RATIO: f32 = 0.10;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TypeStat {
    /// "text"|"code"|"documents"|"data"|"binaries"|"images"|"audio"|"video"|"archives"|"disk_images"|"other"
    pub category: String,
    pub files: u64,
    pub bytes: u64,
    pub est_saved: u64,
    /// est_saved / bytes
    pub ratio: f32,
    pub worthwhile: bool,
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CompressAnalysis {
    pub path: PathBuf,
    pub total: u64,
    pub est_saved: u64,
    pub types: Vec<TypeStat>,
    pub method: String,
    pub supported: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SmartReport {
    pub path: PathBuf,
    /// Diskte tahsis edilen bayt, işlemden önce ve sonra.
    pub before: u64,
    pub after: u64,
    pub compressed_files: u64,
    pub skipped_files: u64,
    pub skipped_bytes: u64,
}

/// Dosya türü: (kategori, zaten sıkıştırılmış mı). `head` dosyanın ilk baytları.
pub fn classify(path: &Path, head: &[u8]) -> (&'static str, bool) {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let h = |p: &[u8]| head.starts_with(p);
    // Önce sihirli baytlar: uzantı yalan söyleyebilir.
    if h(b"\x89PNG")
        || h(&[0xFF, 0xD8, 0xFF])
        || h(b"GIF8")
        || (h(b"RIFF") && head.get(8..12) == Some(b"WEBP"))
    {
        return ("images", true);
    }
    if h(b"BM") && matches!(ext.as_str(), "bmp") || h(b"II*\0") || h(b"MM\0*") {
        return ("images", false);
    }
    if head.get(4..8) == Some(b"ftyp") {
        return match head.get(8..12) {
            Some(b"heic" | b"heix" | b"mif1" | b"avif") => ("images", true),
            Some(b"M4A " | b"M4B ") => ("audio", true),
            _ => ("video", true),
        };
    }
    if h(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return ("video", true);
    }
    if h(b"ID3") || h(b"fLaC") || h(b"OggS") {
        return ("audio", true);
    }
    if h(b"RIFF") && head.get(8..12) == Some(b"WAVE") || h(b"FORM") {
        return ("audio", false);
    }
    if h(b"PK\x03\x04") {
        let office = matches!(
            ext.as_str(),
            "docx" | "xlsx" | "pptx" | "pages" | "numbers" | "key" | "odt" | "ods" | "odp" | "epub"
        );
        return (if office { "documents" } else { "archives" }, true);
    }
    if h(&[0x1F, 0x8B])
        || h(b"7z\xBC\xAF")
        || h(b"Rar!")
        || h(&[0x28, 0xB5, 0x2F, 0xFD])
        || h(&[0xFD, b'7', b'z', b'X', b'Z'])
        || h(b"BZh")
    {
        return ("archives", true);
    }
    if h(b"%PDF") {
        return ("documents", false);
    }
    if h(&[0x7F, b'E', b'L', b'F'])
        || h(&[0xCF, 0xFA, 0xED, 0xFE])
        || h(&[0xCE, 0xFA, 0xED, 0xFE])
        || h(&[0xCA, 0xFE, 0xBA, 0xBE])
        || h(b"MZ")
    {
        return ("binaries", false);
    }
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "avif" => ("images", true),
        "bmp" | "tif" | "tiff" | "psd" | "svg" | "ico" | "raw" | "dng" => ("images", false),
        "mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v" | "wmv" | "flv" => ("video", true),
        "mp3" | "aac" | "m4a" | "ogg" | "opus" | "flac" => ("audio", true),
        "wav" | "aiff" | "aif" => ("audio", false),
        "zip" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "7z" | "rar" | "jar" | "apk" | "whl" => {
            ("archives", true)
        }
        "dmg" | "iso" | "img" | "vhd" | "vhdx" | "vmdk" | "qcow2" => ("disk_images", ext == "dmg"),
        "txt" | "md" | "log" | "rst" | "tex" | "ini" | "cfg" | "conf" => ("text", false),
        "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "c" | "h" | "cc" | "cpp" | "hpp" | "go"
        | "java" | "kt" | "swift" | "rb" | "php" | "sh" | "css" | "html" | "htm" | "lua" | "cs"
        | "m" | "mm" => ("code", false),
        "json" | "xml" | "csv" | "tsv" | "yaml" | "yml" | "toml" | "sql" | "plist" | "db"
        | "sqlite" | "ndjson" | "jsonl" => ("data", false),
        "doc" | "xls" | "ppt" | "rtf" | "pdf" => ("documents", false),
        "docx" | "xlsx" | "pptx" | "pages" | "numbers" | "key" | "odt" | "epub" => {
            ("documents", true)
        }
        "dylib" | "so" | "dll" | "exe" | "o" | "a" | "lib" | "class" | "node" | "wasm" => {
            ("binaries", false)
        }
        _ => ("other", false),
    }
}

#[derive(Default)]
struct Cat {
    files: u64,
    bytes: u64,
    /// Sıkıştırılabilir (ön-sıkıştırılmamış, yeterince büyük) dosyalar.
    cand: Vec<(PathBuf, u64)>,
    cand_bytes: u64,
}

struct Walk {
    cats: BTreeMap<&'static str, Cat>,
    files: u64,
    total: u64,
}

fn walk(path: &Path, cancel: &AtomicBool) -> Result<Walk> {
    let mut w = Walk {
        cats: BTreeMap::new(),
        files: 0,
        total: 0,
    };
    let mut head = [0u8; 16];
    // ponytail: tek iş parçacığı; 100k dosyada açma+oku ~1-2 sn. Yavaşsa başlık okumayı paralelleştir.
    for e in jwalk::WalkDir::new(path)
        .skip_hidden(false)
        .follow_links(false)
    {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let Ok(e) = e else { continue };
        if !e.file_type().is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy();
        if name.starts_with('.') && name.contains(".ebb-cmp.") {
            continue; // yarım kalmış geçici dosyamız
        }
        let Ok(m) = e.metadata() else { continue };
        let len = m.len();
        let p = e.path();
        let n = std::fs::File::open(&p)
            .and_then(|mut f| f.read(&mut head))
            .unwrap_or(0);
        let (cat, packed) = classify(&p, &head[..n]);
        let c = w.cats.entry(cat).or_default();
        c.files += 1;
        c.bytes += len;
        w.files += 1;
        w.total += len;
        if !packed && len >= MIN_SIZE {
            c.cand_bytes += len;
            c.cand.push((p, len));
        }
    }
    Ok(w)
}

/// Örnek dosyaların ilk 128 KiB'ını zlib-6 ile sıkıştırıp kazanç oranını (0..1) verir.
fn sample_ratio(cand: &[(PathBuf, u64)], cancel: &AtomicBool) -> Result<f64> {
    let step = cand.len().div_ceil(SAMPLE_FILES).max(1);
    let (mut inp, mut out) = (0usize, 0usize);
    let mut buf = vec![0u8; SAMPLE_BYTES];
    for (p, _) in cand.iter().step_by(step) {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let Ok(mut f) = std::fs::File::open(p) else {
            continue;
        };
        let mut n = 0;
        while n < buf.len() {
            match f.read(&mut buf[n..]) {
                Ok(0) | Err(_) => break,
                Ok(k) => n += k,
            }
        }
        if n == 0 {
            continue;
        }
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(6));
        let packed = z.write_all(&buf[..n]).and_then(|_| z.finish());
        inp += n;
        out += packed.map_or(n, |v| v.len().min(n));
    }
    Ok(if inp == 0 {
        0.0
    } else {
        1.0 - out as f64 / inp as f64
    })
}

fn stats(w: &Walk, min_ratio: f32, cancel: &AtomicBool) -> Result<Vec<(TypeStat, bool)>> {
    let mut v = Vec::new();
    for (&cat, c) in &w.cats {
        let r = if c.cand.is_empty() {
            0.0
        } else {
            sample_ratio(&c.cand, cancel)?
        };
        let est = (r * c.cand_bytes as f64) as u64;
        let ratio = if c.bytes == 0 {
            0.0
        } else {
            est as f32 / c.bytes as f32
        };
        let worthwhile = est > 0 && ratio >= min_ratio;
        let reason = if c.cand.is_empty() {
            "already compressed or too small"
        } else if worthwhile {
            "compresses well"
        } else {
            "little to gain"
        };
        v.push((
            TypeStat {
                category: cat.into(),
                files: c.files,
                bytes: c.bytes,
                est_saved: if worthwhile { est } else { 0 },
                ratio: if worthwhile { ratio } else { 0.0 },
                worthwhile,
                reason: reason.into(),
            },
            worthwhile,
        ));
    }
    v.sort_by_key(|b| std::cmp::Reverse(b.0.bytes));
    Ok(v)
}

fn method() -> &'static str {
    if cfg!(target_os = "macos") {
        "APFS transparent compression (per file)"
    } else if cfg!(windows) {
        "NTFS compression, LZX (per file)"
    } else {
        "Transparent compression is not available on this system"
    }
}

/// Klasörü türe göre çözümler ve kazancı örnekleyerek tahmin eder; hiçbir şeyi değiştirmez.
pub fn analyze(path: &Path, cancel: &AtomicBool) -> Result<CompressAnalysis> {
    let w = walk(path, cancel)?;
    let types: Vec<TypeStat> = stats(&w, DEFAULT_MIN_RATIO, cancel)?
        .into_iter()
        .map(|t| t.0)
        .collect();
    Ok(CompressAnalysis {
        path: path.to_path_buf(),
        total: w.total,
        est_saved: types.iter().map(|t| t.est_saved).sum(),
        types,
        method: method().into(),
        supported: optimize::compress_supported(path),
    })
}

/// Yalnızca değecek dosyaları yerinde sıkıştırır; `min_ratio` altındaki kategoriler atlanır.
pub fn compress_smart(path: &Path, min_ratio: f32, cancel: &AtomicBool) -> Result<SmartReport> {
    guard::check(path)?;
    if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(Error::Guard(format!("{} (symlink)", path.display())));
    }
    if !optimize::compress_supported(path) {
        return Err(Error::Guard(
            "compression is not supported on this filesystem yet".into(),
        ));
    }
    let (before, _) = scan::measure(path);
    let w = walk(path, cancel)?;
    let good: Vec<String> = stats(&w, min_ratio, cancel)?
        .into_iter()
        .filter(|(_, ok)| *ok)
        .map(|(t, _)| t.category)
        .collect();
    let list: Vec<(PathBuf, u64)> = good
        .iter()
        .flat_map(|k| w.cats[k.as_str()].cand.iter().cloned())
        .collect();
    let (done, done_bytes) = imp::compress_files(&list, cancel)?;
    let (after, _) = scan::measure(path);
    Ok(SmartReport {
        path: path.to_path_buf(),
        before,
        after,
        compressed_files: done,
        skipped_files: w.files - done,
        skipped_bytes: w.total - done_bytes,
    })
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use std::fs::{self, File, FileTimes, Metadata};
    use std::os::macos::fs::MetadataExt as _;
    use std::os::unix::fs::MetadataExt as _;
    use std::process::Command;
    use std::sync::atomic::AtomicU64;

    const UF_COMPRESSED: u32 = 0x20;

    pub fn compress_files(list: &[(PathBuf, u64)], cancel: &AtomicBool) -> Result<(u64, u64)> {
        let next = std::sync::atomic::AtomicUsize::new(0);
        let (files, bytes) = (AtomicU64::new(0), AtomicU64::new(0));
        let threads = std::thread::available_parallelism()
            .map_or(2, |n| n.get())
            .min(8);
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| {
                    let mut buf = vec![0u8; 1 << 20];
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= list.len() || cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        if compress_one(&list[i].0, &mut buf).is_ok() {
                            files.fetch_add(1, Ordering::Relaxed);
                            bytes.fetch_add(list[i].1, Ordering::Relaxed);
                        }
                    }
                });
            }
        });
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        Ok((files.into_inner(), bytes.into_inner()))
    }

    fn fingerprint(m: &Metadata) -> (u64, i64, u64, i64, i64) {
        (m.len(), m.mtime(), m.ino(), m.ctime(), m.ctime_nsec())
    }

    /// Tek dosya: ditto kopyası → doğrula → atomik rename. Hatada orijinale dokunulmaz.
    fn compress_one(p: &Path, buf: &mut [u8]) -> std::result::Result<(), String> {
        let e = |e: std::io::Error| e.to_string();
        guard::check(p).map_err(|e| e.to_string())?;
        let m = fs::symlink_metadata(p).map_err(e)?;
        let locked = libc::UF_IMMUTABLE | libc::UF_APPEND | libc::SF_IMMUTABLE | libc::SF_APPEND;
        if !m.is_file()
            || m.nlink() != 1
            || m.len() < MIN_SIZE
            || m.uid() != unsafe { libc::geteuid() }
            || m.st_flags() & (locked | UF_COMPRESSED) != 0
        {
            return Err("not eligible".into());
        }
        let name = p.file_name().ok_or("no file name")?.to_string_lossy();
        let tmp = p.with_file_name(format!(".{name}.ebb-cmp.{}", std::process::id()));
        if fs::symlink_metadata(&tmp).is_ok() {
            return Err("temp exists".into());
        }
        let res = (|| -> std::result::Result<(), String> {
            // --noclone şart: aksi halde APFS'te ditto klonlar ve sıkıştırmaz.
            let out = Command::new("/usr/bin/ditto")
                .args(["--noclone", "--hfsCompression"])
                .arg(p)
                .arg(&tmp)
                .output()
                .map_err(e)?;
            if !out.status.success() {
                return Err("ditto failed".into());
            }
            let t = fs::symlink_metadata(&tmp).map_err(e)?;
            if t.st_flags() & UF_COMPRESSED == 0 || t.blocks() >= m.blocks() {
                return Err("no gain".into());
            }
            let (a, b) = (
                crate::dupes::hash_file(p, buf).map_err(e)?,
                crate::dupes::hash_file(&tmp, buf).map_err(e)?,
            );
            if t.len() != m.len() || a != b {
                return Err("copy did not verify".into());
            }
            File::open(&tmp)
                .and_then(|f| {
                    let mut ft = FileTimes::new().set_modified(m.modified()?);
                    if let Ok(a) = m.accessed() {
                        ft = ft.set_accessed(a);
                    }
                    f.set_times(ft)
                })
                .map_err(e)?;
            if fingerprint(&fs::symlink_metadata(p).map_err(e)?) != fingerprint(&m) {
                return Err("file changed while compressing".into());
            }
            fs::rename(&tmp, p).map_err(e)
        })();
        if res.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        res
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::fs::MetadataExt as _;

    const REPARSE: u32 = 0x400;
    const COMPRESSED: u32 = 0x800;
    /// Komut satırı sınırı (32767) altında kalmak için yığın başına karakter.
    const CHUNK_CHARS: usize = 24_000;

    pub fn compress_files(list: &[(PathBuf, u64)], cancel: &AtomicBool) -> Result<(u64, u64)> {
        let (mut files, mut bytes) = (0, 0);
        let mut chunk: Vec<&(PathBuf, u64)> = Vec::new();
        let mut chars = 0;
        let eligible = list.iter().filter(|(p, _)| {
            std::fs::symlink_metadata(p)
                .is_ok_and(|m| m.is_file() && m.file_attributes() & (REPARSE | COMPRESSED) == 0)
        });
        let mut run = |chunk: &mut Vec<&(PathBuf, u64)>| -> Result<()> {
            if chunk.is_empty() {
                return Ok(());
            }
            let ok = std::process::Command::new("compact")
                .args(["/c", "/i", "/q", "/exe:lzx"])
                .args(chunk.iter().map(|(p, _)| p))
                .output()?
                .status
                .success();
            // Başarısız yığın atlanmış sayılır (NTFS dışı birim vb.).
            if ok {
                files += chunk.len() as u64;
                bytes += chunk.iter().map(|(_, n)| n).sum::<u64>();
            }
            chunk.clear();
            Ok(())
        };
        for item in eligible {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            let n = item.0.as_os_str().len() + 3;
            if chars + n > CHUNK_CHARS {
                run(&mut chunk)?;
                chars = 0;
            }
            chunk.push(item);
            chars += n;
        }
        run(&mut chunk)?;
        Ok((files, bytes))
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod imp {
    use super::*;
    pub fn compress_files(_: &[(PathBuf, u64)], _: &AtomicBool) -> Result<(u64, u64)> {
        Err(Error::Guard(
            "compression is not supported on this filesystem yet".into(),
        ))
    }
}
