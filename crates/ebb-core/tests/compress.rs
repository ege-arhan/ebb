use ebb_core::compress::{analyze, classify};
use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

fn rnd(n: usize, mut x: u64) -> Vec<u8> {
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

fn text(n: usize) -> Vec<u8> {
    "fn main() { println!(\"hello ebb, compress me please\"); }\n"
        .repeat(n / 50 + 1)
        .into_bytes()[..n]
        .to_vec()
}

fn json(n: usize) -> Vec<u8> {
    let mut s = String::from("[");
    let mut i = 0;
    while s.len() < n {
        s += &format!("{{\"id\":{i},\"name\":\"item\",\"tags\":[\"a\",\"b\"]}},");
        i += 1;
    }
    s.into_bytes()
}

/// Sıkıştırılmamış (stored) gerçek bir zip: sihirli baytlar + rastgele içerik.
fn zip() -> Vec<u8> {
    let mut v = b"PK\x03\x04".to_vec();
    v.extend(rnd(200_000, 7));
    v
}

fn fixture(root: &Path) {
    fs::create_dir_all(root.join("sub")).unwrap();
    for i in 0..5 {
        fs::write(root.join(format!("a{i}.txt")), text(300_000)).unwrap();
        fs::write(root.join("sub").join(format!("d{i}.json")), json(300_000)).unwrap();
    }
    fs::write(root.join("movie.mp4"), rnd(500_000, 3)).unwrap();
    fs::write(root.join("pack.zip"), zip()).unwrap();
    fs::write(root.join("tiny.txt"), b"hi").unwrap();
}

#[test]
fn classify_by_extension_and_magic() {
    let c = |n: &str, h: &[u8]| classify(Path::new(n), h).0;
    assert_eq!(c("a.txt", b"hello"), "text");
    assert_eq!(c("a.rs", b""), "code");
    assert_eq!(c("a.json", b""), "data");
    assert_eq!(c("a.dmg", b""), "disk_images");
    assert_eq!(c("a.unknown", b"xx"), "other");
    // uzantı yalan söylese de sihirli bayt kazanır
    assert_eq!(c("a.txt", b"\x89PNG\r\n\x1a\n"), "images");
    assert_eq!(c("a.txt", &[0xFF, 0xD8, 0xFF, 0xE0]), "images");
    assert_eq!(c("a.txt", b"\0\0\0\x18ftypmp42"), "video");
    assert_eq!(c("a.txt", b"PK\x03\x04"), "archives");
    assert_eq!(c("a.txt", b"%PDF-1.7"), "documents");
    assert_eq!(c("a.txt", &[0x1F, 0x8B, 8]), "archives");
    assert_eq!(c("a.txt", b"7z\xBC\xAF\x27\x1C"), "archives");
    assert_eq!(c("a.txt", b"Rar!\x1a\x07"), "archives");
    assert_eq!(c("a.txt", &[0x28, 0xB5, 0x2F, 0xFD]), "archives");
    assert_eq!(c("a.bin", &[0xCF, 0xFA, 0xED, 0xFE]), "binaries");
    assert_eq!(c("a.bin", &[0x7F, b'E', b'L', b'F']), "binaries");
}

#[test]
fn analyze_mixed_fixture() {
    let d = tempfile::tempdir().unwrap();
    fixture(d.path());
    let a = analyze(d.path(), &AtomicBool::new(false)).unwrap();
    let get = |c: &str| a.types.iter().find(|t| t.category == c);
    let t = get("text").unwrap();
    assert!(t.worthwhile && t.ratio > 0.8, "{t:?}");
    let j = get("data").unwrap();
    assert!(j.worthwhile && j.ratio > 0.5, "{j:?}");
    for c in ["video", "archives"] {
        let t = get(c).unwrap();
        assert!(!t.worthwhile && t.est_saved == 0, "{t:?}");
    }
    assert!(a.est_saved > 0 && a.est_saved < a.total);
}

#[test]
fn analyze_honours_cancel() {
    let d = tempfile::tempdir().unwrap();
    fixture(d.path());
    let r = analyze(d.path(), &AtomicBool::new(true));
    assert!(matches!(r, Err(ebb_core::Error::Cancelled)));
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use ebb_core::{compress::compress_smart, scan};
    use std::process::Command;

    fn snapshot(root: &Path) -> Vec<(String, Vec<u8>, std::time::SystemTime)> {
        let mut v: Vec<_> = jwalk::WalkDir::new(root)
            .skip_hidden(false)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| {
                (
                    e.path().to_string_lossy().into_owned(),
                    fs::read(e.path()).unwrap(),
                    fs::metadata(e.path()).unwrap().modified().unwrap(),
                )
            })
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    #[test]
    fn compress_smart_end_to_end() {
        // tempdir APFS üzerinde (/var/folders).
        let d = tempfile::tempdir().unwrap();
        fixture(d.path());
        let locked = d.path().join("locked.txt");
        fs::write(&locked, text(300_000)).unwrap();
        assert!(
            Command::new("chflags")
                .arg("uchg")
                .arg(&locked)
                .status()
                .unwrap()
                .success()
        );
        let snap = snapshot(d.path());

        let r = compress_smart(d.path(), 0.10, &AtomicBool::new(false));
        let _ = Command::new("chflags").arg("nouchg").arg(&locked).status();
        let r = r.unwrap();
        assert!(r.after < r.before / 2, "{r:?}");
        assert_eq!(r.compressed_files, 10, "{r:?}");
        assert!(r.skipped_files >= 4, "{r:?}"); // mp4, zip, tiny, locked
        assert_eq!(snapshot(d.path()), snap, "contents or mtimes changed");
        assert!(
            !jwalk::WalkDir::new(d.path())
                .skip_hidden(false)
                .into_iter()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().contains(".ebb")),
            "temp leftovers"
        );
        // rastgele dosya sıkıştırılmadı
        let (mp4, _) = scan::measure(&d.path().join("movie.mp4"));
        assert!(mp4 >= 500_000);

        let again = compress_smart(d.path(), 0.10, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            again.compressed_files, 1,
            "only the formerly locked file: {again:?}"
        );
    }
}
