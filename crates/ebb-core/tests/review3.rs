//! Third independent review (destructive code). Run `cargo build -p ebb-cli` first.
//! Anything that depends on HOME/dirs runs in a child with HOME, USERPROFILE, EBB_DATA_DIR in a tempdir.
//! Never touches the real Trash/apps/configs: the only reads outside tempdirs are guard checks.
#![cfg(not(windows))]
use ebb_core::{
    apps::{self, LeftoverRoots},
    autocare::{self, Rule, RuleKind, Schedule},
    dupes::DupGroup,
    guard,
    journal::Journal,
    optimize, projects,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
    time::{Duration, Instant, SystemTime},
};

// ---------------------------------------------------------------- child harness

fn run_case(case: &str, path_env: &str, setup: impl FnOnce(&Path)) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    for name in ["home", "data", "tmp"] {
        fs::create_dir(root.join(name)).unwrap();
    }
    setup(&root);
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "fixture3", "--nocapture"])
        .env("EBB_REVIEW3_CASE", case)
        .env("HOME", root.join("home"))
        .env("USERPROFILE", root.join("home"))
        .env("EBB_DATA_DIR", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("home/.config"))
        .env("XDG_CACHE_HOME", root.join("home/.cache"))
        .env("XDG_DATA_HOME", root.join("home/.local/share"))
        .env("TMPDIR", root.join("tmp"))
        .env("PATH", path_env.replace("{root}", root.to_str().unwrap()))
        .current_dir(&root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(40) {
            let _ = child.kill();
            panic!("{case}: fixture exceeded 40s");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{case}\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn fixture3() {
    let Ok(case) = std::env::var("EBB_REVIEW3_CASE") else {
        return;
    };
    let root = std::env::current_dir().unwrap();
    let home = root.join("home");
    // Safety net: refuse to run anything if HOME isolation did not take effect.
    assert_eq!(dirs::home_dir().unwrap().canonicalize().unwrap(), home);
    assert_eq!(ebb_core::data_dir(), root.join("data"));
    match case.as_str() {
        "git_failopen" => {
            // PATH is empty: `git` cannot run, so the dirty-repo check cannot be evaluated.
            let f = root.join("repo/note.txt");
            assert!(
                guard::check(&f).is_err(),
                "guard allowed an untracked file in a repo when git was unavailable (fail-open)"
            );
        }
        "prune_cwd" => {
            let neutral = root.join("untrusted-checkout");
            std::env::set_current_dir(&neutral).unwrap();
            let r = ebb_core::prune::run("npm").unwrap();
            assert!(r.ok, "{}", r.output);
            let cwd = fs::read_to_string(root.join("npm-cwd.txt")).unwrap();
            assert_ne!(
                PathBuf::from(cwd.trim()).canonicalize().unwrap(),
                neutral.canonicalize().unwrap(),
                "prune tool ran inside the caller's cwd (project-level .npmrc/.yarnrc are honoured)"
            );
        }
        "largeold_never_auto" => {
            let big = home.join("big.bin");
            let rule = Rule {
                id: "x".into(),
                name: "x".into(),
                kind: RuleKind::LargeOld {
                    min_mb: 0,
                    min_age_days: 0,
                },
                schedule: Schedule::Daily,
                enabled: true,
                auto_apply: true,
                last_run: None,
                last_result: None,
            };
            assert!(!autocare::effective_auto_apply(&rule));
            let j = Journal::open_default().unwrap();
            let out = autocare::run_rule(&j, &rule).unwrap();
            assert!(!out.applied, "{out:?}");
            assert!(big.exists());
        }
        "care_isolation" => {
            fs::create_dir_all(home.join("Downloads")).unwrap();
            fs::write(home.join("Downloads/x.dmg"), b"installer").unwrap();
            let mk = |id: &str, kind| Rule {
                id: id.into(),
                name: id.into(),
                kind,
                schedule: Schedule::Daily,
                enabled: true,
                auto_apply: false,
                last_run: None,
                last_result: None,
            };
            let j = Journal::open_default().unwrap();
            j.save_rule(&mk("a-trash", RuleKind::TrashOlderThan { days: 30 }))
                .unwrap();
            j.save_rule(&mk("b-inst", RuleKind::OldInstallers { min_age_days: 0 }))
                .unwrap();
            let outs = autocare::run_due(&j, ebb_core::plan::now(), 50.0);
            assert_eq!(outs.len(), 2, "{outs:?}");
            assert!(outs[1].plan_id.is_some(), "second rule blocked: {outs:?}");
            assert!(!outs[1].applied && !outs[0].applied);
            assert!(j.rules().unwrap().iter().all(|r| r.last_run.is_some()));
            assert!(home.join("Downloads/x.dmg").exists());
        }
        #[cfg(target_os = "macos")]
        "trash_basic" => {
            let trash = home.join(".Trash");
            let victim = root.join("victim");
            fs::create_dir_all(&victim).unwrap();
            fs::write(victim.join("precious.txt"), b"keep").unwrap();
            fs::write(trash.join("a.txt"), b"a").unwrap();
            std::os::unix::fs::symlink(&victim, trash.join("link")).unwrap();
            let j = Journal::open_default().unwrap();
            let t = |s: String| vec![s];
            for bad in [
                format!("{}/../victim/precious.txt", trash.display()),
                victim.join("precious.txt").display().to_string(),
                trash.display().to_string(),
                format!("{}/.", trash.display()),
                format!("{}/..", trash.display()),
                format!("{}/missing", trash.display()),
                "/".into(),
                String::new(),
            ] {
                assert!(
                    ebb_core::trashbin::purge(&j, &t(bad.clone())).is_err(),
                    "accepted {bad:?}"
                );
            }
            // A symlink inside the Trash is removed as a link; its target survives.
            ebb_core::trashbin::purge(&j, &t(trash.join("link").display().to_string())).unwrap();
            assert!(!trash.join("link").exists());
            assert!(victim.join("precious.txt").exists());
            // restore never overwrites an existing original.
            let orig = home.join("orig.txt");
            fs::write(&orig, b"mine").unwrap();
            assert!(ebb_core::platform::restore_item(&orig, Some(&trash.join("a.txt"))).is_err());
            assert_eq!(fs::read(&orig).unwrap(), b"mine");
            assert!(trash.join("a.txt").exists());
        }
        #[cfg(target_os = "macos")]
        "trash_slash" => {
            let trash = home.join(".Trash");
            let victim = root.join("victim");
            fs::create_dir_all(&victim).unwrap();
            fs::write(victim.join("precious.txt"), b"keep").unwrap();
            std::os::unix::fs::symlink(&victim, trash.join("link")).unwrap();
            let j = Journal::open_default().unwrap();
            let _ = ebb_core::trashbin::purge(&j, &[format!("{}/link/", trash.display())]);
            assert!(
                victim.join("precious.txt").exists(),
                "purge with a trailing slash on a Trash symlink deleted the link target's contents"
            );
        }
        other => panic!("unknown case {other}"),
    }
}

// ---------------------------------------------------------------- guard / projects / prune / autocare

#[test]
fn dirty_git_check_does_not_fail_open() {
    if !git(Path::new("/"), &["--version"]) {
        return;
    }
    run_case("git_failopen", "", |root| {
        let repo = root.join("repo");
        fs::create_dir(&repo).unwrap();
        assert!(git(&repo, &["init", "-q"]));
        fs::write(repo.join("note.txt"), b"uncommitted").unwrap();
        // Control: with git available the file is protected.
        assert!(guard::check(&repo.join("note.txt")).is_err());
    });
}

#[test]
fn prune_runs_outside_caller_cwd() {
    run_case("prune_cwd", "{root}/bin", |root| {
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir_all(root.join("untrusted-checkout")).unwrap();
        let s = root.join("bin/npm");
        fs::write(
            &s,
            format!("#!/bin/sh\npwd > '{}/npm-cwd.txt'\n", root.display()),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&s, fs::Permissions::from_mode(0o755)).unwrap();
    });
}

#[test]
fn largeold_rule_never_auto_applies() {
    run_case("largeold_never_auto", "", |root| {
        fs::write(root.join("home/big.bin"), vec![1u8; 4096]).unwrap();
    });
}

#[test]
fn failing_rule_does_not_block_others() {
    run_case("care_isolation", "", |_| {});
}

#[test]
fn trash_days_zero_selects_everything_and_unknown_dates_are_kept() {
    use ebb_core::trashbin::TrashEntry;
    let e = |id: &str, at: Option<i64>| TrashEntry {
        id: id.into(),
        name: id.into(),
        size: 1,
        deleted_at: at,
        original: None,
        by_ebb: false,
        plan_id: None,
        restorable: false,
    };
    let now = 100_000_000;
    let v = vec![
        e("new", Some(now - 1000)),
        e("unknown", None),
        e("old", Some(now - 40 * 86_400)),
    ];
    let ids: Vec<_> = autocare::old_trash(&v, 30, now)
        .iter()
        .map(|x| x.id.clone())
        .collect();
    assert_eq!(ids, ["old"]);
    // No floor on `days`: a rule with days=0 + auto_apply permanently deletes every dated item.
    assert_eq!(autocare::old_trash(&v, 0, now).len(), 2);
}

#[cfg(target_os = "macos")]
#[test]
fn trash_purge_rejects_paths_outside_trash_and_keeps_link_targets() {
    run_case("trash_basic", "", |root| {
        fs::create_dir_all(root.join("home/.Trash")).unwrap();
    });
}

#[cfg(target_os = "macos")]
#[test]
fn trash_purge_trailing_slash_does_not_follow_symlink() {
    run_case("trash_slash", "", |root| {
        fs::create_dir_all(root.join("home/.Trash")).unwrap();
    });
}

#[test]
fn active_project_is_not_idle() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().canonicalize().unwrap().join("proj");
    fs::create_dir_all(p.join("src")).unwrap();
    fs::create_dir_all(p.join("target")).unwrap();
    fs::write(p.join("Cargo.toml"), b"[package]").unwrap();
    fs::write(p.join("src/main.rs"), b"fn main(){}").unwrap(); // edited just now
    let old = SystemTime::now() - Duration::from_secs(90 * 86_400);
    for f in [p.join("Cargo.toml"), p.join("src")] {
        let h = fs::File::open(&f).unwrap();
        h.set_modified(old).unwrap();
    }
    let r =
        projects::find_artifacts(&[d.path().to_path_buf()], 30, &AtomicBool::new(false)).unwrap();
    assert!(
        r.is_empty(),
        "offered {:?}",
        r.iter().map(|a| &a.path).collect::<Vec<_>>()
    );
}

#[test]
fn symlinked_artifacts_and_projects_are_not_followed() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().canonicalize().unwrap();
    let real = root.join("elsewhere/proj");
    fs::create_dir_all(real.join("node_modules")).unwrap();
    fs::write(real.join("package.json"), b"{}").unwrap();
    let scan_root = root.join("code");
    fs::create_dir(&scan_root).unwrap();
    std::os::unix::fs::symlink(&real, scan_root.join("linked")).unwrap();
    // symlinked node_modules inside a real project
    let p2 = scan_root.join("p2");
    fs::create_dir_all(&p2).unwrap();
    fs::write(p2.join("package.json"), b"{}").unwrap();
    std::os::unix::fs::symlink(real.join("node_modules"), p2.join("node_modules")).unwrap();
    let old = SystemTime::now() - Duration::from_secs(400 * 86_400);
    for f in [real.join("package.json"), p2.join("package.json")] {
        fs::File::open(&f).unwrap().set_modified(old).unwrap();
    }
    let r = projects::find_artifacts(&[scan_root], 30, &AtomicBool::new(false)).unwrap();
    assert!(r.is_empty(), "{r:?}");
}

// ---------------------------------------------------------------- apps

fn roots(d: &Path) -> LeftoverRoots {
    LeftoverRoots {
        dirs: vec![(d.to_path_buf(), "support")],
    }
}

fn names(d: &Path, name: &str, bid: Option<&str>) -> Vec<String> {
    let mut v: Vec<String> = apps::leftovers_in(&roots(d), name, bid)
        .into_iter()
        .map(|l| l.path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn touch_all(d: &Path, files: &[&str]) {
    for f in files {
        fs::write(d.join(f), b"x").unwrap();
    }
}

#[test]
fn leftovers_ignore_lookalikes_and_generic_names() {
    let d = tempfile::tempdir().unwrap();
    touch_all(
        d.path(),
        &[
            "com.vendor.App.plist",
            "com.vendor.AppPro.plist", // longer id sharing a prefix without '.' boundary
            "com.vendor.Apple.plist",
            "Data",
            "Google",
            "com.apple.Safari.plist",
        ],
    );
    assert_eq!(
        names(d.path(), "App", Some("com.vendor.App")),
        ["com.vendor.App.plist"]
    );
    // generic / short names never match by name
    assert!(names(d.path(), "Data", None).is_empty());
    assert!(names(d.path(), "Google", None).is_empty());
    assert!(names(d.path(), "App", None).is_empty());
    // Apple apps get nothing
    assert!(names(d.path(), "Safari", Some("com.apple.Safari")).is_empty());
}

#[test]
fn leftovers_do_not_claim_sibling_apps_by_bundle_prefix() {
    let d = tempfile::tempdir().unwrap();
    touch_all(
        d.path(),
        &[
            "com.google.Chrome.plist",
            "com.google.Chrome.canary.plist",
            "com.vendor.Other.plist",
        ],
    );
    assert_eq!(
        names(d.path(), "Chrome Stable", Some("com.google.Chrome")),
        ["com.google.Chrome.plist"]
    );
    assert!(names(d.path(), "Vendor Tool", Some("com.vendor")).is_empty());
}

#[test]
fn uninstall_paths_exclude_medium_by_default() {
    let d = tempfile::tempdir().unwrap();
    touch_all(d.path(), &["com.acme.Tool.plist", "Acme Tool"]);
    let left = apps::leftovers_in(&roots(d.path()), "Acme Tool", Some("com.acme.Tool"));
    assert_eq!(left.len(), 2);
    let pv = apps::UninstallPreview {
        app: apps::InstalledApp {
            id: "x".into(),
            name: "Acme Tool".into(),
            path: PathBuf::new(),
            bundle_id: None,
            version: None,
            size: 0,
            running: false,
            last_used_days: None,
            system: false,
        },
        total: 0,
        warnings: vec![],
        leftovers: left,
    };
    let strict = apps::uninstall_paths(&pv, false);
    assert_eq!(strict.len(), 1);
    assert!(strict[0].ends_with("com.acme.Tool.plist"));
    assert_eq!(apps::uninstall_paths(&pv, true).len(), 2);
}

#[cfg(target_os = "macos")]
#[test]
fn applications_app_can_be_planned_for_uninstall() {
    let Some(app) = fs::read_dir("/Applications")
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "app") && !p.is_symlink())
    else {
        return;
    };
    // guard::check only canonicalizes and (maybe) runs `git status`; from_paths would also measure(), so use the guard directly.
    assert!(
        guard::check(&app).is_ok(),
        "{}",
        guard::check(&app).unwrap_err()
    );
}

// ---------------------------------------------------------------- optimize

fn group(bytes: &[u8], files: Vec<PathBuf>) -> DupGroup {
    DupGroup {
        hash: blake3::hash(bytes).to_hex().to_string(),
        size: bytes.len() as u64,
        files,
        reclaimable: bytes.len() as u64,
        optimized: false,
    }
}

fn clone_env() -> Option<(tempfile::TempDir, PathBuf, Journal)> {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().canonicalize().unwrap();
    if !optimize::clone_supported(&root) {
        return None;
    }
    let j = Journal::open(&root.join("j.db")).unwrap();
    Some((d, root, j))
}

fn leftovers(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".ebb-"))
        .collect()
}

#[test]
fn clone_dedupe_refuses_changed_symlinked_and_hardlinked_targets() {
    let Some((_d, root, j)) = clone_env() else {
        return;
    };
    let body = vec![7u8; 8192];
    let mut other = body.clone();
    other[100] = 8; // same size, different bytes
    let (src, changed, linked, hard, hard2, victim) = (
        root.join("src"),
        root.join("changed"),
        root.join("linked"),
        root.join("hard"),
        root.join("hard2"),
        root.join("victim"),
    );
    fs::write(&src, &body).unwrap();
    fs::write(&changed, &other).unwrap();
    fs::write(&victim, &body).unwrap();
    std::os::unix::fs::symlink(&victim, &linked).unwrap();
    fs::write(&hard, &body).unwrap();
    fs::hard_link(&hard, &hard2).unwrap();
    let g = group(
        &body,
        vec![src.clone(), changed.clone(), linked.clone(), hard.clone()],
    );
    let rep = optimize::dedupe_with_clones(&j, &[g]).unwrap();
    assert_eq!(rep.files, 0, "{rep:?}");
    assert_eq!(rep.skipped.len(), 3);
    assert_eq!(fs::read(&changed).unwrap(), other);
    assert!(
        fs::symlink_metadata(&linked)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    use std::os::unix::fs::MetadataExt;
    assert_eq!(
        fs::metadata(&hard).unwrap().ino(),
        fs::metadata(&hard2).unwrap().ino()
    );
    assert!(leftovers(&root).is_empty(), "{:?}", leftovers(&root));
}

#[test]
fn clone_dedupe_keeps_target_mode_and_mtime() {
    let Some((_d, root, j)) = clone_env() else {
        return;
    };
    let body = vec![3u8; 8192];
    let (src, dst) = (root.join("src"), root.join("dst"));
    fs::write(&src, &body).unwrap();
    fs::write(&dst, &body).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&src, fs::Permissions::from_mode(0o644)).unwrap();
    fs::set_permissions(&dst, fs::Permissions::from_mode(0o600)).unwrap();
    let old = SystemTime::now() - Duration::from_secs(5 * 86_400);
    fs::File::open(&dst).unwrap().set_modified(old).unwrap();
    let before = fs::metadata(&dst).unwrap().modified().unwrap();
    let rep = optimize::dedupe_with_clones(&j, &[group(&body, vec![src, dst.clone()])]).unwrap();
    assert_eq!(rep.files, 1, "{rep:?}");
    let m = fs::metadata(&dst).unwrap();
    assert_eq!(m.permissions().mode() & 0o777, 0o600);
    assert_eq!(m.modified().unwrap(), before);
    assert_eq!(fs::read(&dst).unwrap(), body);
    assert!(leftovers(&root).is_empty());
}

#[cfg(target_os = "macos")]
fn xattrs(p: &Path) -> Vec<String> {
    let o = Command::new("/usr/bin/xattr").arg(p).output().unwrap();
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .collect()
}

#[cfg(target_os = "macos")]
#[test]
fn clone_dedupe_does_not_leak_source_xattrs_to_target() {
    let Some((_d, root, j)) = clone_env() else {
        return;
    };
    let body = vec![5u8; 8192];
    let (src, dst) = (root.join("src"), root.join("dst"));
    fs::write(&src, &body).unwrap();
    fs::write(&dst, &body).unwrap();
    let set = |p: &Path, k: &str| {
        assert!(
            Command::new("/usr/bin/xattr")
                .args(["-w", k, "v"])
                .arg(p)
                .status()
                .unwrap()
                .success()
        );
    };
    set(&src, "com.test.only-src");
    set(&dst, "com.test.only-dst");
    let rep = optimize::dedupe_with_clones(&j, &[group(&body, vec![src, dst.clone()])]).unwrap();
    assert_eq!(rep.files, 1, "{rep:?}");
    let x = xattrs(&dst);
    assert!(x.contains(&"com.test.only-dst".to_string()), "{x:?}");
    assert!(
        !x.contains(&"com.test.only-src".to_string()),
        "src xattr leaked: {x:?}"
    );
}

#[cfg(target_os = "macos")]
fn compress_dir() -> Option<(tempfile::TempDir, PathBuf)> {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().canonicalize().unwrap();
    optimize::compress_supported(&root).then_some(())?;
    let dir = root.join("data");
    fs::create_dir(&dir).unwrap();
    Some((d, dir))
}

#[cfg(target_os = "macos")]
#[test]
fn compress_roundtrip_keeps_content_mtime_and_leaves_no_temp() {
    let Some((_d, dir)) = compress_dir() else {
        return;
    };
    let text = "hello compressible world\n".repeat(40_000);
    fs::write(dir.join("a.txt"), &text).unwrap();
    let old = SystemTime::now() - Duration::from_secs(9 * 86_400);
    fs::File::open(dir.join("a.txt"))
        .unwrap()
        .set_modified(old)
        .unwrap();
    let want = fs::metadata(dir.join("a.txt")).unwrap().modified().unwrap();
    optimize::compress(&dir).unwrap();
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), text);
    assert_eq!(
        fs::metadata(dir.join("a.txt")).unwrap().modified().unwrap(),
        want
    );
    optimize::decompress(&dir).unwrap();
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), text);
    assert!(leftovers(dir.parent().unwrap()).is_empty());
    // symlink targets are refused
    let l = dir.parent().unwrap().join("lnk");
    std::os::unix::fs::symlink(&dir, &l).unwrap();
    assert!(optimize::compress(&l).is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn compress_preserves_hardlinks() {
    let Some((_d, dir)) = compress_dir() else {
        return;
    };
    fs::write(dir.join("a"), "x".repeat(100_000)).unwrap();
    fs::hard_link(dir.join("a"), dir.join("b")).unwrap();
    optimize::compress(&dir).unwrap();
    use std::os::unix::fs::MetadataExt;
    assert_eq!(
        fs::metadata(dir.join("a")).unwrap().ino(),
        fs::metadata(dir.join("b")).unwrap().ino()
    );
}

// ---------------------------------------------------------------- MCP

fn mcp(envdir: &Path, lines: &[Value]) -> Vec<Value> {
    let home = envdir.join("home");
    fs::create_dir_all(&home).unwrap();
    let path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("ebb");
    assert!(path.is_file(), "first run: cargo build -p ebb-cli");
    let mut c = Command::new(path)
        .arg("mcp")
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("EBB_DATA_DIR", envdir.join("data"))
        .env("PATH", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut si = c.stdin.take().unwrap();
    for l in lines {
        writeln!(si, "{l}").unwrap();
    }
    drop(si);
    let out = c.wait_with_output().unwrap();
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}

#[test]
fn mcp_tool_list_has_no_mutating_tool() {
    let d = tempfile::tempdir().unwrap();
    let r = mcp(
        d.path(),
        &[json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})],
    );
    let names: Vec<String> = r[0]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert!(names.len() >= 10);
    for n in &names {
        for bad in [
            "apply", "undo", "delete", "purge", "empty", "remove", "clean_", "optimize",
            "compress", "quit", "prune", "restore", "run",
        ] {
            assert!(!n.contains(bad), "{n}");
        }
    }
    // unknown / mutating names are rejected
    for n in ["ebb_apply", "ebb_undo", "ebb_trash_purge", "ebb_prune_run"] {
        let r = mcp(
            d.path(),
            &[
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":n,"arguments":{}}}),
            ],
        );
        assert_eq!(r[0]["result"]["isError"], json!(true));
    }
}

#[test]
fn mcp_huge_integer_argument_does_not_crash_server() {
    let d = tempfile::tempdir().unwrap();
    let scan = d.path().join("scan");
    fs::create_dir(&scan).unwrap();
    let r = mcp(
        d.path(),
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ebb_large_old_files",
            "arguments":{"path":scan,"min_mb":u64::MAX}}}),
        ],
    );
    assert_eq!(r.len(), 1, "server died without answering");
}
