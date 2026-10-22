//! "Optimize": yer geri kazanılır, hiçbir şey silinmez. Kopyalar copy-on-write klonlara çevrilir
//! (aynı disk bloklarını paylaşırlar) ve klasörler şeffaf olarak sıkıştırılır.
use crate::{
    Error, Result,
    dupes::{DupGroup, hash_file},
    guard,
    journal::Journal,
    plan::now,
    scan,
};
use rusqlite::params;
use serde::Serialize;
use std::fs::{self, File, FileTimes, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Serialize, Debug, Default)]
pub struct OptimizeReport {
    /// Klonla değiştirilen kopya sayısı.
    pub files: u64,
    /// Geri kazanılan bayt (klonlanan kopyaların boyutu).
    pub saved: u64,
    pub skipped: Vec<(PathBuf, String)>,
}

#[derive(Serialize, Debug)]
pub struct CompressReport {
    pub path: PathBuf,
    /// Diskte tahsis edilen bayt, işlemden önce ve sonra.
    pub before: u64,
    pub after: u64,
    pub files: u64,
}

fn mtime(m: &Metadata) -> i64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as i64)
}

impl Journal {
    fn clones_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS clones(path TEXT PRIMARY KEY, hash TEXT, size INTEGER, mtime INTEGER, at INTEGER);",
        )?;
        Ok(())
    }

    fn record_clone(&self, path: &Path, hash: &str, size: u64, mtime: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO clones(path,hash,size,mtime,at) VALUES(?1,?2,?3,?4,?5)",
            params![path.to_string_lossy(), hash, size as i64, mtime, now()],
        )?;
        Ok(())
    }

    fn clone_record(&self, path: &Path) -> Option<(String, u64, i64)> {
        self.conn
            .query_row(
                "SELECT hash,size,mtime FROM clones WHERE path=?1",
                [path.to_string_lossy()],
                |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64, r.get(2)?)),
            )
            .ok()
    }
}

/// Marks groups whose copies Ebb already cloned (unchanged since) as optimized with reclaimable 0.
/// Kısmen klonlanmış grupta reclaimable yalnız kalan kopyaları sayar.
pub fn mark_optimized(journal: &Journal, groups: &mut [DupGroup]) {
    for g in groups.iter_mut() {
        let left = g
            .files
            .iter()
            .skip(1)
            .filter(|p| !cloned_unchanged(journal, g, p))
            .count() as u64;
        g.optimized = g.files.len() > 1 && left == 0;
        g.reclaimable = g.size * left;
    }
}

fn cloned_unchanged(journal: &Journal, g: &DupGroup, p: &Path) -> bool {
    let Ok(m) = fs::symlink_metadata(p) else {
        return false;
    };
    journal.clone_record(p).is_some_and(|(hash, size, t)| {
        hash == g.hash && size == g.size && m.len() == size && mtime(&m) == t
    })
}

/// Keep files[0] of each group; replace every other copy with a CoW clone of it. Nothing is deleted.
pub fn dedupe_with_clones(journal: &Journal, groups: &[DupGroup]) -> Result<OptimizeReport> {
    journal.clones_table()?;
    let mut report = OptimizeReport::default();
    let mut buf = vec![0u8; 1 << 20];
    for g in groups.iter().filter(|g| !g.optimized) {
        let Some((src, targets)) = g.files.split_first() else {
            continue;
        };
        for t in targets {
            match clone_over(src, t, g, &mut buf) {
                Ok(mt) => {
                    journal.record_clone(t, &g.hash, g.size, mt)?;
                    report.files += 1;
                    report.saved += g.size;
                }
                Err(why) => report.skipped.push((t.clone(), why)),
            }
        }
    }
    Ok(report)
}

/// `dst`'yi `src`'nin klonuyla atomik olarak değiştirir; başarıda yeni mtime döner.
/// Hata olursa geçici dosya silinir ve `dst` dokunulmadan kalır.
fn clone_over(
    src: &Path,
    dst: &Path,
    g: &DupGroup,
    buf: &mut [u8],
) -> std::result::Result<i64, String> {
    let e = |e: io::Error| e.to_string();
    guard::check(dst).map_err(|e| e.to_string())?;
    let sm = fs::symlink_metadata(src).map_err(e)?;
    let before = fs::symlink_metadata(dst).map_err(e)?;
    if !sm.is_file() || !before.is_file() {
        return Err("not a regular file".into());
    }
    if sm.len() != g.size || before.len() != g.size {
        return Err("size changed since the scan".into());
    }
    check_pair(&sm, &before)?;
    let name = dst.file_name().ok_or("no file name")?.to_string_lossy();
    let tmp = dst.with_file_name(format!(".{name}.ebb-clone.{}", std::process::id()));
    clone_file(src, &tmp).map_err(|e| format!("clone failed: {e}"))?;
    // Buradan sonra geçici dosya bizim: her hatada silinir.
    let res = (|| {
        copy_metadata(dst, &before, &tmp).map_err(|e| format!("metadata copy failed: {e}"))?;
        // Takastan hemen önce: yerine geçecek klon da hedef de grubun hash'ini taşımalı.
        if hash_file(&tmp, buf).map_err(e)?.to_hex().as_str() != g.hash {
            return Err("kept copy changed since the scan".to_string());
        }
        if hash_file(dst, buf).map_err(e)?.to_hex().as_str() != g.hash {
            return Err("contents changed since the scan".into());
        }
        // Okuma erişim zamanını değiştirmiş olabilir; hedefin zamanları en son yazılır.
        File::open(&tmp)
            .and_then(|f| {
                let mut t = FileTimes::new().set_modified(before.modified()?);
                if let Ok(a) = before.accessed() {
                    t = t.set_accessed(a);
                }
                f.set_times(t)
            })
            .map_err(|e| format!("could not keep timestamps: {e}"))?;
        let now_m = fs::symlink_metadata(dst).map_err(e)?;
        if fingerprint(&now_m) != fingerprint(&before) {
            return Err("file changed while optimizing".into());
        }
        fs::rename(&tmp, dst).map_err(e)
    })();
    if let Err(why) = res {
        let _ = fs::remove_file(&tmp);
        return Err(why);
    }
    fs::symlink_metadata(dst).map(|m| mtime(&m)).map_err(e)
}

/// Değişiklik tespiti için: boyut, mtime ve (unix) inode + ctime.
#[cfg(unix)]
fn fingerprint(m: &Metadata) -> (u64, i64, u64, i64, i64) {
    use std::os::unix::fs::MetadataExt;
    (m.len(), mtime(m), m.ino(), m.ctime(), m.ctime_nsec())
}
#[cfg(not(unix))]
fn fingerprint(m: &Metadata) -> (u64, i64, u64, i64, i64) {
    (m.len(), mtime(m), 0, 0, 0)
}

#[cfg(unix)]
fn check_pair(src: &Metadata, dst: &Metadata) -> std::result::Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    if src.dev() != dst.dev() {
        return Err("on a different volume than the kept copy".into());
    }
    if src.ino() == dst.ino() {
        return Err("already the same file as the kept copy".into());
    }
    if dst.nlink() != 1 {
        return Err("has other hard links".into());
    }
    if dst.uid() != unsafe { libc::geteuid() } {
        return Err("owned by another user".into());
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::MetadataExt;
        let locked = libc::UF_IMMUTABLE | libc::UF_APPEND | libc::SF_IMMUTABLE | libc::SF_APPEND;
        if (src.st_flags() | dst.st_flags()) & locked != 0 {
            return Err("file is locked".into());
        }
    }
    Ok(())
}
#[cfg(not(unix))]
fn check_pair(_: &Metadata, _: &Metadata) -> std::result::Result<(), String> {
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn cpath(p: &Path) -> io::Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(p.as_os_str().as_bytes()).map_err(io::Error::other)
}

#[cfg(target_os = "macos")]
fn fs_type(path: &Path) -> Option<String> {
    let c = cpath(path).ok()?;
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let name = unsafe { std::ffi::CStr::from_ptr(st.f_fstypename.as_ptr()) };
    Some(name.to_string_lossy().into_owned())
}

/// Can this volume make copy-on-write clones? (APFS)
#[cfg(target_os = "macos")]
pub fn clone_supported(path: &Path) -> bool {
    fs_type(path).as_deref() == Some("apfs")
}
/// Can this volume make copy-on-write clones? (Btrfs, XFS)
#[cfg(target_os = "linux")]
pub fn clone_supported(path: &Path) -> bool {
    let Ok(c) = cpath(path) else { return false };
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
        return false;
    }
    // ponytail: XFS reflink'siz kurulmuş olabilir; o durumda FICLONE hata verir ve dosya atlanır.
    matches!(st.f_type as u32, 0x9123_683e | 0x5846_5342) // BTRFS, XFS
}
/// Can this volume make copy-on-write clones? Not supported on this system yet.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn clone_supported(_: &Path) -> bool {
    false
}

/// Can this volume compress files transparently? (APFS, HFS+)
#[cfg(target_os = "macos")]
pub fn compress_supported(path: &Path) -> bool {
    matches!(fs_type(path).as_deref(), Some("apfs" | "hfs"))
}
/// Can this volume compress files transparently?
// ponytail: NTFS varsayılır; NTFS dışı birimde `compact` hata verir ve hata kullanıcıya döner.
#[cfg(windows)]
pub fn compress_supported(_: &Path) -> bool {
    true
}
/// Can this volume compress files transparently? Not supported on this system yet.
#[cfg(not(any(target_os = "macos", windows)))]
pub fn compress_supported(_: &Path) -> bool {
    false
}

#[cfg(target_os = "macos")]
fn clone_file(src: &Path, tmp: &Path) -> io::Result<()> {
    const CLONE_NOFOLLOW: u32 = 0x0001; // sys/clonefile.h
    let (s, t) = (cpath(src)?, cpath(tmp)?);
    // Hedef varsa EEXIST: başkasının dosyasının üzerine yazılmaz.
    if unsafe { libc::clonefile(s.as_ptr(), t.as_ptr(), CLONE_NOFOLLOW) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
#[cfg(target_os = "linux")]
fn clone_file(src: &Path, tmp: &Path) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;
    let s = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(src)?;
    let t = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(tmp)?;
    if unsafe { libc::ioctl(t.as_raw_fd(), libc::FICLONE, s.as_raw_fd()) } != 0 {
        let err = io::Error::last_os_error();
        drop(t);
        let _ = fs::remove_file(tmp);
        return Err(err);
    }
    Ok(())
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn clone_file(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "copy-on-write clones are not supported on this system",
    ))
}

/// Hedefin izinleri, sahibi, bayrakları, xattr/ACL'leri klona kopyalanır (zamanlar ayrıca).
#[cfg(target_os = "macos")]
fn copy_metadata(from: &Path, _: &Metadata, to: &Path) -> io::Result<()> {
    let (f, t) = (cpath(from)?, cpath(to)?);
    if unsafe {
        libc::copyfile(
            f.as_ptr(),
            t.as_ptr(),
            std::ptr::null_mut(),
            libc::COPYFILE_METADATA,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
// ponytail: Linux'ta xattr kopyalanmaz (izin, grup, zamanlar korunur).
#[cfg(target_os = "linux")]
fn copy_metadata(_: &Path, m: &Metadata, to: &Path) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let _ = std::os::unix::fs::chown(to, None, Some(m.gid()));
    fs::set_permissions(to, m.permissions())
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn copy_metadata(_: &Path, m: &Metadata, to: &Path) -> io::Result<()> {
    fs::set_permissions(to, m.permissions())
}

/// Transparent compression of a file or folder in place; contents stay byte-identical.
pub fn compress(path: &Path) -> Result<CompressReport> {
    transcode(path, true)
}

/// Undoes transparent compression in place; contents stay byte-identical.
pub fn decompress(path: &Path) -> Result<CompressReport> {
    transcode(path, false)
}

fn transcode(path: &Path, on: bool) -> Result<CompressReport> {
    guard::check(path)?;
    let m = fs::symlink_metadata(path)?;
    if m.file_type().is_symlink() {
        return Err(Error::Guard(format!("{} (symlink)", path.display())));
    }
    let (before, files) = scan::measure(path);
    run_compression(path, &m, on)?;
    let (after, _) = scan::measure(path);
    Ok(CompressReport {
        path: path.to_path_buf(),
        before,
        after,
        files,
    })
}

/// ditto ile sıkıştırılmış/açılmış kardeş kopya → doğrula → RENAME_SWAP → eski kopyayı sil.
#[cfg(target_os = "macos")]
fn run_compression(path: &Path, _: &Metadata, on: bool) -> Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| Error::Guard(path.display().to_string()))?
        .to_string_lossy();
    let tmp = path.with_file_name(format!(".{name}.ebb-compress.{}", std::process::id()));
    if fs::symlink_metadata(&tmp).is_ok() {
        return Err(Error::Guard(format!("{} already exists", tmp.display())));
    }
    let res = (|| -> Result<()> {
        // --noclone şart: APFS'te ditto aksi halde klonlar ve hiçbir şeyi sıkıştırmaz.
        let flags: &[&str] = if on {
            &["--noclone", "--hfsCompression"]
        } else {
            &[
                "--noclone",
                "--nohfsCompression",
                "--nopreserveHFSCompression",
            ]
        };
        let out = std::process::Command::new("/usr/bin/ditto")
            .args(flags)
            .arg(path)
            .arg(&tmp)
            .output()?;
        if !out.status.success() {
            return Err(io::Error::other(format!(
                "ditto failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ))
            .into());
        }
        let orig = tally(path);
        if orig.is_none() || orig != tally(&tmp) {
            return Err(Error::Guard(format!(
                "{} (copy did not verify, original left untouched)",
                path.display()
            )));
        }
        let (t, p) = (cpath(&tmp)?, cpath(path)?);
        if unsafe { libc::renamex_np(t.as_ptr(), p.as_ptr(), libc::RENAME_SWAP) } != 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(())
    })();
    // Takas olduysa tmp'de eski kopya, olmadıysa yarım kopya var; ikisi de bizim.
    // Takas sonrası eski kopyada yeni kopyada olmayan bir şey kaldıysa (doğrulama ile takas arasında
    // eklenen dosya) eski kopya silinmez; kullanıcıya yeri söylenir.
    if res.is_ok() {
        let (old, new) = (tally(&tmp), tally(path));
        if old.is_none() || new.is_none_or(|n| old.is_some_and(|o| o.0 > n.0 || o.1 > n.1)) {
            return Err(Error::Guard(format!(
                "compressed, but the old copy at {} differs from the new one, so it was kept. Check it, then delete it yourself",
                tmp.display()
            )));
        }
    }
    let remove = |p: &Path| {
        if fs::symlink_metadata(p).is_ok_and(|m| m.is_dir()) {
            fs::remove_dir_all(p)
        } else {
            fs::remove_file(p)
        }
    };
    let mut cleanup = remove(&tmp);
    if cleanup.is_err() {
        // Değiştirilemez (uchg/schg) bayraklı dosyalar silmeyi engeller; bayraklar yalnızca bu geçici kopyadan kaldırılır.
        let _ = std::process::Command::new("/usr/bin/chflags")
            .args(["-R", "nouchg,noschg"])
            .arg(&tmp)
            .output();
        cleanup = remove(&tmp);
    }
    res?;
    cleanup.map_err(|e| {
        Error::Io(io::Error::other(format!(
            "done, but the old copy at {} could not be removed: {e}",
            tmp.display()
        )))
    })
}

/// (dosya sayısı, toplam mantıksal boyut); symlink'ler izlenmez. Okunamayan giriş → None.
#[cfg(target_os = "macos")]
fn tally(path: &Path) -> Option<(u64, u64)> {
    let mut t = (0, 0);
    for e in jwalk::WalkDir::new(path)
        .skip_hidden(false)
        .follow_links(false)
    {
        let e = e.ok()?;
        if !e.file_type().is_dir() {
            t.0 += 1;
            t.1 += e.metadata().ok()?.len();
        }
    }
    Some(t)
}

#[cfg(windows)]
fn run_compression(path: &Path, m: &Metadata, on: bool) -> Result<()> {
    let mut c = std::process::Command::new("compact");
    c.arg(if on { "/c" } else { "/u" });
    if m.is_dir() {
        c.current_dir(path).arg("/s");
    } else {
        c.arg(path);
    }
    c.args(["/i", "/q", if on { "/exe:lzx" } else { "/exe" }]);
    let out = c.output()?;
    if !out.status.success() {
        return Err(io::Error::other(format!(
            "compact failed: {}",
            String::from_utf8_lossy(&out.stdout).trim()
        ))
        .into());
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", windows)))]
fn run_compression(_: &Path, _: &Metadata, _: bool) -> Result<()> {
    Err(Error::Guard(
        "compression is not supported on this filesystem yet".into(),
    ))
}
