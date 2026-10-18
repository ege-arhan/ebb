//! Independent safety checks. Never call the real Trash adapter for deletion.
use ebb_core::{guard, journal::Journal, plan::Plan, scan};
use std::{fs, path::Path, sync::atomic::AtomicBool};

fn scan(root: &Path, cap: usize) -> scan::Node {
    scan::scan(
        root,
        &scan::ScanOptions {
            max_children: cap,
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap()
}

#[cfg(unix)]
mod paths {
    use super::*;
    use std::{os::unix::fs::symlink, process::Command};

    // HOME is process-wide; isolate fixtures without unsafe set_var or parallel-test races.
    fn run_case(case: &str) {
        let d = tempfile::tempdir().unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "paths::fixture", "--nocapture"])
            .env("EBB_REVIEW_CASE", case)
            .env("HOME", d.path().join("home"))
            .env("EBB_DATA_DIR", d.path().join("data"))
            .current_dir(d.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn fixture() {
        let Ok(case) = std::env::var("EBB_REVIEW_CASE") else {
            return;
        };
        let root = std::env::current_dir().unwrap();
        let home = root.join("home");
        fs::create_dir_all(home.join(".ssh")).unwrap();
        fs::create_dir_all(root.join("scratch")).unwrap();
        fs::write(home.join(".ssh/key"), "secret").unwrap();
        assert_eq!(
            dirs::home_dir().unwrap().canonicalize().unwrap(),
            home.canonicalize().unwrap()
        );
        match case.as_str() {
            "aliases" => {
                symlink(home.join(".ssh"), root.join("alias")).unwrap();
                symlink("/usr", root.join("system-alias")).unwrap();
                let paths = vec![
                    root.join("alias/key"),
                    root.join("scratch/../home/.ssh/key"),
                    home.clone(),
                    home.join(".."),
                    root.join("system-alias"),
                ];
                for p in &paths {
                    assert!(guard::check(p).is_err(), "{}", p.display());
                }
                let p = Plan::from_paths(&paths);
                assert!(p.items.is_empty());
                assert_eq!(p.skipped.len(), paths.len());
            }
            "sibling" => {
                let p = home.join(".ssh-backup");
                fs::create_dir(&p).unwrap();
                assert!(guard::check(&p).is_ok());
                assert_eq!(Plan::from_paths(&[p]).items.len(), 1);
            }
            "case" => {
                let p = home.join(".SSH/key");
                if !p.exists() {
                    return;
                } // Case-sensitive volumes have no alias to bypass.
                let plan = Plan::from_paths(std::slice::from_ref(&p));
                assert!(
                    guard::check(&p).is_err() && plan.items.is_empty(),
                    "case alias admitted: {plan:?}"
                );
            }
            "ancestor" => {
                let parent = root.join("external");
                fs::create_dir_all(parent.join("cloud")).unwrap();
                fs::write(parent.join("cloud/important"), "keep").unwrap();
                symlink(parent.join("cloud"), home.join("Dropbox")).unwrap();
                assert!(guard::check(&parent.join("cloud")).is_err());
                let plan = Plan::from_paths(std::slice::from_ref(&parent));
                assert!(
                    guard::check(&parent).is_err() && plan.items.is_empty(),
                    "protected descendant admitted: {plan:?}"
                );
            }
            _ => panic!("unknown fixture"),
        }
    }

    #[test]
    fn symlinks_dotdot_home_and_home_ancestors_are_blocked() {
        run_case("aliases");
    }

    #[test]
    fn protected_name_prefix_does_not_block_a_sibling() {
        run_case("sibling");
    }

    #[test]
    fn protected_case_alias_is_blocked_on_case_insensitive_volumes() {
        run_case("case");
    }

    #[test]
    fn ancestors_of_protected_symlink_targets_are_blocked() {
        run_case("ancestor");
    }
}

#[test]
fn nonexistent_paths_are_skipped_without_measurement() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("absent");
    assert!(guard::check(&p).is_err());
    let plan = Plan::from_paths(&[p]);
    assert!(plan.items.is_empty());
    assert_eq!(plan.skipped.len(), 1);
}

#[test]
fn completed_plan_cannot_be_applied_twice() {
    let d = tempfile::tempdir().unwrap();
    let j = Journal::open(&d.path().join("journal.db")).unwrap();
    let p = Plan::from_paths(&[]);
    j.save_plan(&p).unwrap();
    assert_eq!(j.apply(&p.id).unwrap().failed, 0);
    assert!(j.apply(&p.id).is_err());
    assert_eq!(j.plan(&p.id).unwrap().1, "applied");
}

#[test]
fn saving_a_completed_plan_does_not_rearm_it() {
    let d = tempfile::tempdir().unwrap();
    let j = Journal::open(&d.path().join("journal.db")).unwrap();
    let p = Plan::from_paths(&[]);
    j.save_plan(&p).unwrap();
    j.apply(&p.id).unwrap();
    let saved = j.save_plan(&p);
    assert!(saved.is_err() || j.plan(&p.id).unwrap().1 == "applied");
    assert!(j.apply(&p.id).is_err());
}

#[test]
fn missing_targets_fail_without_touching_trash_and_cannot_be_reapplied() {
    let d = tempfile::tempdir().unwrap();
    let target = d.path().join("cache");
    fs::write(&target, "temporary").unwrap();
    let p = Plan::from_paths(std::slice::from_ref(&target));
    assert_eq!(p.items.len(), 1);
    let db = d.path().join("journal.db");
    let j = Journal::open(&db).unwrap();
    j.save_plan(&p).unwrap();
    fs::remove_file(&target).unwrap();
    let r = j.apply(&p.id).unwrap();
    assert_eq!(r.failed, 1);
    assert_eq!(r.bytes, 0);
    assert_eq!(j.plan(&p.id).unwrap().1, "failed");
    assert!(j.apply(&p.id).is_err());
    let c = rusqlite::Connection::open(db).unwrap();
    let status: String = c
        .query_row("SELECT status FROM entries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "failed");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "BUG: Windows hardlink identities are not deduplicated"
)]
fn hardlinks_across_directories_are_counted_once() {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir(d.path().join("a")).unwrap();
    fs::create_dir(d.path().join("b")).unwrap();
    fs::write(d.path().join("a/file"), vec![1; 8192]).unwrap();
    fs::hard_link(d.path().join("a/file"), d.path().join("b/link")).unwrap();
    let n = scan(d.path(), 200);
    assert_eq!(n.files, 1);
    assert_eq!(scan::measure(d.path()), (n.size, 1));
}

#[cfg(unix)]
#[test]
fn sparse_sizes_use_allocated_blocks_and_symlinks_are_not_followed() {
    use std::os::unix::fs::{MetadataExt, symlink};
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("root");
    fs::create_dir(&root).unwrap();
    let file = fs::File::create(root.join("sparse")).unwrap();
    file.set_len(64 * 1024 * 1024).unwrap();
    let outside = d.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("not-in-scan"), vec![1; 8192]).unwrap();
    symlink(&outside, root.join("link")).unwrap();
    let expected: u64 = [&root, &root.join("sparse"), &root.join("link")]
        .iter()
        .map(|p| fs::symlink_metadata(p).unwrap().blocks() * 512)
        .sum();
    let n = scan(&root, 200);
    assert_eq!(n.files, 2);
    assert_eq!(n.size, expected);
    assert!(n.logical >= 64 * 1024 * 1024);
    assert_eq!(scan::measure(&root), (n.size, n.files));
}

#[test]
fn capped_children_keep_all_file_counts_in_other_bucket() {
    let d = tempfile::tempdir().unwrap();
    for i in 0..24 {
        fs::write(d.path().join(format!("file-{i}")), vec![1; 4096]).unwrap();
    }
    let n = scan(d.path(), 2);
    assert_eq!(n.files, 24);
    assert_eq!(n.children.iter().map(|n| n.files).sum::<u64>(), n.files);
}

#[cfg(target_os = "macos")]
mod undo {
    use super::*;
    use rusqlite::{Connection, params};
    use std::os::unix::fs::symlink;

    struct Fixture {
        dir: tempfile::TempDir,
        j: Journal,
        db: Connection,
        id: String,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("journal.db");
            let j = Journal::open(&path).unwrap();
            let p = Plan::from_paths(&[]);
            j.save_plan(&p).unwrap();
            let db = Connection::open(path).unwrap();
            db.execute("UPDATE plans SET status='applied'", []).unwrap();
            Self {
                dir,
                j,
                db,
                id: p.id,
            }
        }

        // Seed a moved item entirely within tempdir, without reading the user's Trash.
        fn entry(&self, name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
            let original = self.dir.path().join(name);
            let trashed = self.dir.path().join(format!("trash-{name}"));
            fs::write(&trashed, "old content").unwrap();
            self.db.execute(
                "INSERT INTO entries(plan_id,original,trashed,size,status,at) VALUES(?1,?2,?3,11,'applied',0)",
                params![self.id, original.to_str().unwrap(), trashed.to_str().unwrap()],
            ).unwrap();
            (original, trashed)
        }

        fn status(&self) -> String {
            self.db
                .query_row("SELECT status FROM entries ORDER BY id LIMIT 1", [], |r| {
                    r.get(0)
                })
                .unwrap()
        }
    }

    #[test]
    fn existing_original_is_preserved_and_partial_undo_can_retry() {
        let f = Fixture::new();
        let (first, trash) = f.entry("conflict");
        let (second, _) = f.entry("restorable");
        fs::write(&first, "new content").unwrap();
        let r = f.j.undo(&f.id).unwrap();
        assert_eq!(r.failed, 1);
        assert_eq!(r.bytes, 11);
        assert_eq!(fs::read_to_string(&first).unwrap(), "new content");
        assert_eq!(fs::read_to_string(&second).unwrap(), "old content");
        assert!(trash.exists());
        assert_eq!(f.status(), "applied");
        assert_eq!(f.j.plan(&f.id).unwrap().1, "applied");
        fs::remove_file(&first).unwrap();
        assert_eq!(f.j.undo(&f.id).unwrap().failed, 0);
        assert_eq!(f.j.plan(&f.id).unwrap().1, "undone");
        assert!(f.j.undo(&f.id).is_err());
    }

    #[test]
    fn dangling_original_symlink_is_not_overwritten() {
        let f = Fixture::new();
        let (original, trash) = f.entry("link");
        symlink(f.dir.path().join("absent"), &original).unwrap();
        let r = f.j.undo(&f.id).unwrap();
        assert_eq!(r.failed, 1, "undo replaced an existing dangling symlink");
        assert!(
            fs::symlink_metadata(&original)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(trash.exists());
    }

    #[test]
    fn undo_does_not_follow_a_replaced_parent_symlink() {
        let f = Fixture::new();
        let (original, _) = f.entry("payload");
        let parent = f.dir.path().join("parent");
        let other = f.dir.path().join("other");
        fs::create_dir(&parent).unwrap();
        fs::create_dir(&other).unwrap();
        let destination = parent.join("payload");
        f.db.execute(
            "UPDATE entries SET original=?1",
            [destination.to_str().unwrap()],
        )
        .unwrap();
        fs::remove_dir(&parent).unwrap();
        symlink(&other, &parent).unwrap();
        let r = f.j.undo(&f.id).unwrap();
        assert_eq!(r.failed, 1, "undo followed a substituted parent");
        assert!(!other.join(original.file_name().unwrap()).exists());
    }

    #[test]
    fn sql_failure_during_undo_leaves_recoverable_item_state() {
        let f = Fixture::new();
        let (original, trash) = f.entry("payload");
        f.db.execute_batch(
            "CREATE TRIGGER fail_undo BEFORE UPDATE OF status ON entries
            WHEN NEW.status='undone' BEGIN SELECT RAISE(ABORT, 'injected write failure'); END;",
        )
        .unwrap();
        assert!(f.j.undo(&f.id).is_err());
        assert!(
            (original.exists() && f.status() == "undone")
                || (trash.exists() && f.status() == "applied"),
            "file moved but journal still says applied; a retry cannot find its trash source"
        );
    }
}
