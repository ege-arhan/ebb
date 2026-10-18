use ebb_core::{catalog, guard, journal::Journal, plan::Plan, scan};
use std::sync::atomic::AtomicBool;

fn fixture() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("a/b")).unwrap();
    std::fs::write(d.path().join("a/big.bin"), vec![1u8; 300_000]).unwrap();
    std::fs::write(d.path().join("a/b/small.txt"), "hi").unwrap();
    d
}

#[test]
fn scan_counts_files_and_sizes() {
    let d = fixture();
    let n = scan::scan(
        d.path(),
        &Default::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(n.files, 2);
    assert!(n.logical >= 300_002);
    assert!(n.size >= n.logical); // blok tahsisi mantıksaldan küçük olamaz (seyrek dosya yok)
    assert!(n.children.iter().any(|c| c.name == "a"));
}

#[cfg(unix)]
#[test]
fn hardlinks_counted_once() {
    let d = fixture();
    std::fs::hard_link(d.path().join("a/big.bin"), d.path().join("a/b/link.bin")).unwrap();
    let n = scan::scan(
        d.path(),
        &Default::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(n.files, 2, "hardlink ikinci dosya sayılmamalı");
}

#[test]
fn cancel_stops_scan() {
    let d = fixture();
    let r = scan::scan(
        d.path(),
        &Default::default(),
        &AtomicBool::new(true),
        |_| {},
    );
    assert!(matches!(r, Err(ebb_core::Error::Cancelled)));
}

#[test]
fn builtin_catalog_parses_and_ids_unique() {
    let c = catalog::builtin();
    assert!(c.len() >= 35, "katalog {} girdi", c.len());
    let mut ids: Vec<_> = c.iter().map(|e| &e.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), c.len());
}

#[test]
fn guard_blocks_roots_and_home() {
    assert!(guard::check(std::path::Path::new("/")).is_err());
    assert!(guard::check(&dirs::home_dir().unwrap()).is_err());
    assert!(guard::check(&dirs::home_dir().unwrap().join("Documents")).is_err());
    assert!(guard::check(std::path::Path::new("/usr")).is_err());
}

#[test]
fn apply_then_undo_restores() {
    let d = fixture();
    let target = d.path().join("a");
    let before = std::fs::read(target.join("big.bin")).unwrap();
    let j = Journal::open(&d.path().join("j.db")).unwrap();
    let plan = Plan::from_paths(std::slice::from_ref(&target));
    assert_eq!(plan.items.len(), 1, "skipped: {:?}", plan.skipped);
    j.save_plan(&plan).unwrap();
    let r = j.apply(&plan.id).unwrap();
    assert_eq!(r.failed, 0, "{r:?}");
    assert!(!target.exists());
    assert!(j.apply(&plan.id).is_err(), "ikinci uygulama reddedilmeli");
    let u = j.undo(&plan.id).unwrap();
    assert_eq!(u.failed, 0, "{u:?}");
    assert_eq!(std::fs::read(target.join("big.bin")).unwrap(), before);
}

#[test]
fn from_paths_blocks_before_measuring() {
    // `/` ölçülseydi dakikalar sürerdi; koruma önce çalışmalı.
    let t = std::time::Instant::now();
    let p = Plan::from_paths(&[std::path::PathBuf::from("/"), dirs::home_dir().unwrap()]);
    assert!(p.items.is_empty());
    assert_eq!(p.skipped.len(), 2);
    assert!(t.elapsed().as_secs() < 5);
}
