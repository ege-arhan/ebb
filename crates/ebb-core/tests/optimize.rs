use ebb_core::{Error, dupes};
use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

#[cfg_attr(windows, allow(dead_code))]
fn find(root: &Path) -> Vec<dupes::DupGroup> {
    dupes::find_duplicates(root, 1, &AtomicBool::new(false), |_| {}).unwrap()
}

fn pattern(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8 ^ seed).collect()
}

#[test]
fn groups_identical_files_largest_first() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path();
    fs::create_dir_all(p.join("sub")).unwrap();
    let a = pattern(100_000, 1);
    for n in ["a1", "a2", "sub/a3"] {
        fs::write(p.join(n), &a).unwrap();
    }
    let b = pattern(300_000, 2);
    fs::write(p.join("b1"), &b).unwrap();
    fs::write(p.join("sub/b2"), &b).unwrap();
    // Aynı boyut, aynı ilk/son 64 KiB, ortası farklı: yalnız tam hash ayırabilir.
    let mut c = b.clone();
    c[150_000] ^= 0xff;
    fs::write(p.join("c"), &c).unwrap();
    fs::write(p.join("e1"), "").unwrap();
    fs::write(p.join("e2"), "").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(p.join("b1"), p.join("b-link")).unwrap();

    let mut stages = vec![];
    let g =
        dupes::find_duplicates(p, 1, &AtomicBool::new(false), |pr| stages.push(pr.stage)).unwrap();
    assert!(stages.contains(&"listing") && stages.contains(&"hashing"));
    assert_eq!(g.len(), 2, "{g:#?}");
    assert_eq!(g[0].size, 300_000);
    assert_eq!(g[0].files.len(), 2);
    assert_eq!(g[0].reclaimable, 300_000);
    assert_eq!(g[0].hash, blake3::hash(&b).to_hex().to_string());
    assert_eq!(g[1].files.len(), 3);
    assert_eq!(g[1].reclaimable, 200_000);
    assert!(g.iter().all(|g| !g.optimized));
    assert!(!g.iter().flat_map(|g| &g.files).any(|f| f.ends_with("c")));

    // Eşik: 200 KB altı gruplar elenir.
    let big = dupes::find_duplicates(p, 200_000, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(big.len(), 1);
}

#[cfg(unix)]
#[test]
fn hardlinks_not_double_counted() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path();
    let x = pattern(50_000, 3);
    fs::write(p.join("x"), &x).unwrap();
    fs::hard_link(p.join("x"), p.join("x-link")).unwrap();
    fs::write(p.join("y"), &x).unwrap();
    let g = find(p);
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].files.len(), 2, "{g:#?}");
    assert_eq!(g[0].reclaimable, 50_000);

    // Yalnız aynı inode'un yolları: kopya yok.
    fs::remove_file(p.join("y")).unwrap();
    assert!(find(p).is_empty());
}

#[test]
fn cancel_stops_search() {
    let d = tempfile::tempdir().unwrap();
    fs::write(d.path().join("a"), "x").unwrap();
    let r = dupes::find_duplicates(d.path(), 1, &AtomicBool::new(true), |_| {});
    assert!(matches!(r, Err(Error::Cancelled)));
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use ebb_core::{journal::Journal, optimize};
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, SystemTime};

    fn xattr(args: &[&str]) -> bool {
        std::process::Command::new("xattr")
            .args(args)
            .output()
            .unwrap()
            .status
            .success()
    }

    #[test]
    fn dedupe_clones_keep_bytes_and_metadata() {
        let d = tempfile::tempdir().unwrap();
        let db = tempfile::tempdir().unwrap();
        let j = Journal::open(&db.path().join("ebb.db")).unwrap();
        let p = d.path().canonicalize().unwrap();
        assert!(optimize::clone_supported(&p), "test volume is not APFS");
        let data = pattern(500_000, 4);
        fs::write(p.join("a"), &data).unwrap();
        fs::write(p.join("b"), &data).unwrap();
        let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
        fs::File::options()
            .write(true)
            .open(p.join("b"))
            .unwrap()
            .set_modified(old)
            .unwrap();
        fs::set_permissions(p.join("b"), fs::Permissions::from_mode(0o640)).unwrap();
        // Korunan kopyanın xattr'ı klonla hedefe taşınmamalı.
        assert!(xattr(&[
            "-w",
            "com.ebb.test",
            "src",
            p.join("a").to_str().unwrap()
        ]));

        let mut g = find(&p);
        assert_eq!(g.len(), 1);
        optimize::mark_optimized(&j, &mut g);
        assert!(!g[0].optimized);
        let r = optimize::dedupe_with_clones(&j, &g).unwrap();
        assert_eq!((r.files, r.saved), (1, 500_000), "{:?}", r.skipped);

        let b = p.join("b");
        assert_eq!(fs::read(&b).unwrap(), data);
        assert_eq!(fs::read(p.join("a")).unwrap(), data);
        let m = fs::metadata(&b).unwrap();
        assert_eq!(m.modified().unwrap(), old);
        assert_eq!(m.permissions().mode() & 0o777, 0o640);
        assert!(!xattr(&["-p", "com.ebb.test", b.to_str().unwrap()]));
        assert!(
            fs::read_dir(&p).unwrap().count() == 2,
            "temp file left behind"
        );

        let mut again = find(&p);
        optimize::mark_optimized(&j, &mut again);
        assert!(again[0].optimized);
        assert_eq!(again[0].reclaimable, 0);

        // Klon sonradan değişirse artık "optimized" sayılmaz.
        fs::write(&b, pattern(500_000, 5)).unwrap();
        fs::write(p.join("c"), &data).unwrap();
        let mut g = find(&p);
        optimize::mark_optimized(&j, &mut g);
        assert!(!g[0].optimized);
    }

    #[test]
    fn file_changed_after_scan_is_skipped() {
        let d = tempfile::tempdir().unwrap();
        let db = tempfile::tempdir().unwrap();
        let j = Journal::open(&db.path().join("ebb.db")).unwrap();
        let p = d.path().canonicalize().unwrap();
        let data = pattern(200_000, 6);
        for n in ["a", "b", "c"] {
            fs::write(p.join(n), &data).unwrap();
        }
        let g = find(&p);
        let changed = pattern(200_000, 7);
        fs::write(p.join("c"), &changed).unwrap();
        let r = optimize::dedupe_with_clones(&j, &g).unwrap();
        assert_eq!(r.files, 1);
        assert_eq!(r.skipped.len(), 1);
        assert!(r.skipped[0].0.ends_with("c"));
        assert_eq!(fs::read(p.join("c")).unwrap(), changed);
        assert_eq!(
            fs::read_dir(&p).unwrap().count(),
            3,
            "temp file left behind"
        );
    }

    #[test]
    fn compress_round_trip_keeps_contents() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().canonicalize().unwrap();
        let dir = p.join("logs");
        fs::create_dir_all(dir.join("sub")).unwrap();
        let text = "the same line again and again\n".repeat(40_000);
        fs::write(dir.join("a.log"), &text).unwrap();
        fs::write(dir.join("sub/b.log"), &text[..5000]).unwrap();
        assert!(optimize::compress_supported(&dir));

        let r = optimize::compress(&dir).unwrap();
        assert_eq!(r.files, 2);
        assert!(r.after < r.before, "{r:?}");
        assert_eq!(fs::read_to_string(dir.join("a.log")).unwrap(), text);
        assert_eq!(
            fs::read_to_string(dir.join("sub/b.log")).unwrap(),
            &text[..5000]
        );

        let r = optimize::decompress(&dir).unwrap();
        assert!(r.after > r.before, "{r:?}");
        assert_eq!(fs::read_to_string(dir.join("a.log")).unwrap(), text);
        assert_eq!(
            fs::read_dir(&p).unwrap().count(),
            1,
            "temp copy left behind"
        );

        // Tek dosya ve symlink.
        let f = dir.join("a.log");
        optimize::compress(&f).unwrap();
        assert_eq!(fs::read_to_string(&f).unwrap(), text);
        std::os::unix::fs::symlink(&f, p.join("link")).unwrap();
        assert!(optimize::compress(&p.join("link")).is_err());
    }
}
