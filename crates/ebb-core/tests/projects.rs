//! Proje artefakt bulucu: yalnızca geçici klasörlerde çalışır, hiçbir şey silmez.
use ebb_core::{Error, projects::find_artifacts};
use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime};

fn age(p: &Path, days: u64) {
    let t = SystemTime::now() - Duration::from_secs(days * 86_400);
    fs::File::options()
        .write(true)
        .open(p)
        .or_else(|_| fs::File::open(p))
        .unwrap()
        .set_modified(t)
        .unwrap();
}

/// Marker + artefakt klasörü (içinde 1 KB dosya) kurar, proje dosyalarını `days` gün eskitir.
fn project(root: &Path, name: &str, marker: &str, art: &str, days: u64) -> std::path::PathBuf {
    let p = root.join(name);
    fs::create_dir_all(p.join(art)).unwrap();
    fs::write(p.join(art).join("f.bin"), vec![1u8; 1024]).unwrap();
    fs::write(p.join(marker), "x").unwrap();
    age(&p.join(marker), days);
    p
}

fn run(root: &Path, idle: u64) -> Vec<ebb_core::projects::Artifact> {
    find_artifacts(&[root.to_path_buf()], idle, &AtomicBool::new(false)).unwrap()
}

#[test]
fn detects_kinds_and_requires_markers() {
    let t = tempfile::tempdir().unwrap();
    let r = t.path();
    for (n, m, a, k) in [
        ("a", "package.json", "node_modules", "node_modules"),
        ("b", "Cargo.toml", "target", "cargo_target"),
        ("c", "requirements.txt", ".venv", "python_venv"),
        ("d", "Podfile", "Pods", "pods"),
        ("e", "main.tf", ".terraform", "terraform"),
        ("f", "App.csproj", "obj", "dotnet"),
        ("g", "build.gradle", ".gradle", "gradle"),
    ] {
        project(r, n, m, a, 90);
        let found = run(&r.join(n), 30);
        assert_eq!(found.len(), 1, "{n}");
        assert_eq!(found[0].kind, k);
        assert!(found[0].size > 0 && found[0].files == 1 && found[0].project_idle_days >= 89);
    }
    // Marker yok: "target" tek başına artefakt sayılmaz.
    fs::create_dir_all(r.join("h/target")).unwrap();
    fs::write(r.join("h/target/x"), "x").unwrap();
    assert!(run(&r.join("h"), 0).is_empty());
}

#[test]
fn idle_filter() {
    let t = tempfile::tempdir().unwrap();
    project(t.path(), "old", "package.json", "node_modules", 90);
    project(t.path(), "new", "package.json", "node_modules", 1);
    let f = run(t.path(), 30);
    assert_eq!(f.len(), 1);
    assert!(f[0].project.ends_with("old"));
    assert_eq!(run(t.path(), 0).len(), 2);
}

#[test]
fn nested_node_modules_counted_once_and_sorted() {
    let t = tempfile::tempdir().unwrap();
    let p = project(t.path(), "a", "package.json", "node_modules", 90);
    // İç içe paket: kendi package.json'ı olsa bile node_modules içine inilmez.
    let inner = p.join("node_modules/dep");
    fs::create_dir_all(inner.join("node_modules")).unwrap();
    fs::write(inner.join("package.json"), "{}").unwrap();
    fs::write(inner.join("node_modules/y"), vec![0u8; 8192]).unwrap();
    project(t.path(), "big", "Cargo.toml", "target", 90);
    fs::write(t.path().join("big/target/huge"), vec![1u8; 1 << 20]).unwrap();
    let f = run(t.path(), 30);
    assert_eq!(f.len(), 2);
    assert_eq!(f[0].kind, "cargo_target");
    assert!(f[0].size >= f[1].size);
}

#[test]
fn skips_dirty_git() {
    let git = |d: &Path, a: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(d)
            .args(["-c", "user.email=a@b.c", "-c", "user.name=t"])
            .args(a)
            .output()
    };
    if git(Path::new("."), &["--version"]).map_or(true, |o| !o.status.success()) {
        return;
    }
    let t = tempfile::tempdir().unwrap();
    let p = project(t.path(), "a", "package.json", "node_modules", 90);
    fs::write(p.join(".gitignore"), "node_modules\n").unwrap();
    git(&p, &["init", "-q"]).unwrap();
    git(&p, &["add", "-A"]).unwrap();
    git(&p, &["commit", "-qm", "i"]).unwrap();
    for f in [p.join(".gitignore"), p.join("package.json")] {
        age(&f, 90);
    }
    assert_eq!(run(t.path(), 0).len(), 1, "clean repo is offered");
    fs::write(p.join("wip.txt"), "uncommitted").unwrap();
    age(&p.join("wip.txt"), 90);
    assert!(run(t.path(), 0).is_empty(), "dirty repo is skipped");
}

#[test]
fn cancel_and_symlinks() {
    let t = tempfile::tempdir().unwrap();
    project(t.path(), "a", "package.json", "node_modules", 90);
    let c = AtomicBool::new(true);
    let e = find_artifacts(&[t.path().to_path_buf()], 0, &c).unwrap_err();
    assert!(matches!(e, Error::Cancelled));
    #[cfg(unix)]
    {
        let p = t.path().join("s");
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("package.json"), "{}").unwrap();
        age(&p.join("package.json"), 90);
        std::os::unix::fs::symlink(t.path().join("a/node_modules"), p.join("node_modules"))
            .unwrap();
        assert!(run(&p, 0).is_empty());
    }
}
