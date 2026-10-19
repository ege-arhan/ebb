//! Proje artefaktları: yeniden üretilebilir derleme çıktıları (node_modules, target, ...).
//! Bu modül asla silmez; sonuçlar normal bir Plan ile Çöp Kutusu'na gider.
use crate::{Error, Result, guard, scan::measure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Artifact {
    pub path: PathBuf,
    pub project: PathBuf,
    pub kind: String,
    pub size: u64,
    pub files: u64,
    pub project_idle_days: u64,
    pub safety: String,
    pub rebuild: String,
}

const MAX_DEPTH: usize = 6;

pub fn default_roots() -> Vec<PathBuf> {
    let Some(h) = dirs::home_dir() else {
        return vec![];
    };
    [
        "Developer",
        "Projects",
        "Code",
        "dev",
        "src",
        "repos",
        "Documents",
        "Desktop",
        "Downloads",
    ]
    .iter()
    .map(|d| h.join(d))
    .filter(|p| p.is_dir())
    .collect()
}

struct Hit {
    path: PathBuf,
    project: PathBuf,
    kind: &'static str,
    safety: &'static str,
    rebuild: &'static str,
}

type Rule = (&'static str, &'static str, &'static str, &'static str);

/// Klasördeki dosya adlarına göre (artefakt klasörü, tür, güvenlik, yeniden kurma ipucu).
fn rules(files: &HashSet<String>) -> Vec<Rule> {
    let has = |n: &str| files.contains(n);
    let ext = |e: &str| files.iter().any(|f| f.ends_with(e));
    let mut v: Vec<Rule> = vec![];
    if has("package.json") {
        v.push(("node_modules", "node_modules", "safe", "npm install"));
        v.push((".next", "next", "safe", "npm run build"));
        v.push(("dist", "dist", "review", "npm run build"));
        v.push(("build", "build", "review", "npm run build"));
    }
    if has("Cargo.toml") {
        v.push(("target", "cargo_target", "safe", "cargo build"));
    }
    if has("pyproject.toml") || has("requirements.txt") || has("setup.py") {
        v.push((
            ".venv",
            "python_venv",
            "review",
            "python -m venv .venv && pip install -r requirements.txt",
        ));
        v.push((
            "venv",
            "python_venv",
            "review",
            "python -m venv venv && pip install -r requirements.txt",
        ));
        v.push(("__pycache__", "pycache", "safe", "regenerated on next run"));
    }
    if has("build.gradle") || has("build.gradle.kts") {
        v.push((".gradle", "gradle", "safe", "./gradlew build"));
        v.push(("build", "build", "review", "./gradlew build"));
    }
    if has("Podfile") {
        v.push(("Pods", "pods", "safe", "pod install"));
    }
    if ext(".csproj") || ext(".sln") {
        v.push(("bin", "dotnet", "review", "dotnet build"));
        v.push(("obj", "dotnet", "safe", "dotnet build"));
    }
    if has("main.tf") {
        v.push((".terraform", "terraform", "safe", "terraform init"));
    }
    v
}

fn days_since(t: SystemTime) -> u64 {
    SystemTime::now()
        .duration_since(t)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0)
}

/// Proje kökündeki dosyalar + `.git` HEAD/index'in en yeni mtime'ı (ucuz yaklaşım).
fn idle_days(project: &Path, skip: &HashSet<&str>) -> u64 {
    let mut newest: Option<SystemTime> = None;
    let mut bump = |p: &Path| {
        if let Ok(t) = std::fs::symlink_metadata(p).and_then(|m| m.modified()) {
            newest = Some(newest.map_or(t, |n| n.max(t)));
        }
    };
    if let Ok(rd) = std::fs::read_dir(project) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name != ".git" && !skip.contains(name.as_str()) {
                bump(&e.path());
            }
        }
    }
    bump(&project.join(".git/HEAD"));
    bump(&project.join(".git/index"));
    newest.map_or(0, days_since)
}

/// `guard::dirty_git` ile aynı mantık (o özel olduğu için burada yeniden yazıldı).
fn dirty_git(p: &Path) -> bool {
    if !p.ancestors().any(|a| a.join(".git").exists()) {
        return false;
    }
    let mut c = std::process::Command::new("git");
    c.arg("-C")
        .arg(p)
        .args(["status", "--porcelain", "--"])
        .arg(p);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c.output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

/// Basit paralel map: sıra korunur, iş parçacığı sayısı çekirdek kadar.
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    let n = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(items.len().max(1));
    std::thread::scope(|s| {
        for _ in 0..n {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(it) = items.get(i) else { break };
                    let r = f(it);
                    out.lock().unwrap()[i] = Some(r);
                }
            });
        }
    });
    out.into_inner().unwrap().into_iter().flatten().collect()
}

fn walk(root: &Path, cancel: &Arc<AtomicBool>, hits: &Arc<Mutex<Vec<Hit>>>) {
    let (cancel, hits) = (cancel.clone(), hits.clone());
    let walker = jwalk::WalkDir::new(root)
        .skip_hidden(false)
        .follow_links(false)
        .sort(false)
        .max_depth(MAX_DEPTH)
        .process_read_dir(move |_, dir, _, children| {
            if cancel.load(Ordering::Relaxed) {
                children.clear();
                return;
            }
            let names: Vec<(String, bool)> = children
                .iter()
                .flatten()
                .map(|c| {
                    (
                        c.file_name().to_string_lossy().into_owned(),
                        c.file_type().is_dir(),
                    )
                })
                .collect();
            let files: HashSet<String> = names
                .iter()
                .filter(|(_, d)| !d)
                .map(|(n, _)| n.clone())
                .collect();
            let rules = rules(&files);
            let mut found = vec![];
            children.retain(|c| {
                let Ok(c) = c else { return true };
                if !c.file_type().is_dir() {
                    return true;
                }
                let name = c.file_name().to_string_lossy();
                if let Some(r) = rules.iter().find(|r| r.0 == name) {
                    found.push(Hit {
                        path: c.path(),
                        project: dir.to_path_buf(),
                        kind: r.1,
                        safety: r.2,
                        rebuild: r.3,
                    });
                    return false;
                }
                // Markersiz node_modules/.git/çöp kutusu içine inme.
                !matches!(&*name, "node_modules" | ".git" | ".Trash" | "Library")
            });
            if !found.is_empty() {
                hits.lock().unwrap().extend(found);
            }
        });
    for _ in walker {}
}

/// `roots` altında `min_idle_days` gündür dokunulmamış projelerin artefaktlarını bulur.
pub fn find_artifacts(
    roots: &[PathBuf],
    min_idle_days: u64,
    cancel: &AtomicBool,
) -> Result<Vec<Artifact>> {
    // jwalk 'static kapanış ister; iptal bayrağı Arc ile köprülenir.
    let flag = Arc::new(AtomicBool::new(cancel.load(Ordering::Relaxed)));
    let hits = Arc::new(Mutex::new(Vec::new()));
    let roots: Vec<PathBuf> = {
        let c: Vec<PathBuf> = roots.iter().filter_map(|r| r.canonicalize().ok()).collect();
        c.iter()
            .filter(|r| !c.iter().any(|o| o != *r && r.starts_with(o)))
            .cloned()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    };
    let done = AtomicBool::new(false);
    std::thread::scope(|s| {
        // İptal bayrağını izleyen küçük bekçi.
        s.spawn(|| {
            while !done.load(Ordering::Relaxed) {
                if cancel.load(Ordering::Relaxed) {
                    flag.store(true, Ordering::Relaxed);
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        });
        for r in &roots {
            walk(r, &flag, &hits);
        }
        done.store(true, Ordering::Relaxed);
    });
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    let hits = std::mem::take(&mut *hits.lock().unwrap());
    let mut by_project: BTreeMap<PathBuf, Vec<Hit>> = BTreeMap::new();
    for h in hits {
        by_project.entry(h.project.clone()).or_default().push(h);
    }
    let projects: Vec<(PathBuf, Vec<Hit>)> = by_project.into_iter().collect();
    // Aşama 2: boşta kalma süresi + kirli git deposu (projeye bir kez).
    let ok = par_map(&projects, |(p, hs)| {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let skip: HashSet<&str> = hs
            .iter()
            .filter_map(|h| h.path.file_name()?.to_str())
            .collect();
        let idle = idle_days(p, &skip);
        (idle >= min_idle_days && !dirty_git(p)).then_some(idle)
    });
    let work: Vec<(&Hit, u64)> = projects
        .iter()
        .zip(ok)
        .filter_map(|((_, hs), idle)| idle.map(|i| hs.iter().map(move |h| (h, i))))
        .flatten()
        .collect();
    // Aşama 3: koruma + boyut.
    let mut out: Vec<Artifact> = par_map(&work, |(h, idle)| {
        if cancel.load(Ordering::Relaxed) || guard::check(&h.path).is_err() {
            return None;
        }
        let (size, files) = measure(&h.path);
        Some(Artifact {
            path: h.path.clone(),
            project: h.project.clone(),
            kind: h.kind.into(),
            size,
            files,
            project_idle_days: *idle,
            safety: h.safety.into(),
            rebuild: h.rebuild.into(),
        })
    })
    .into_iter()
    .flatten()
    .collect();
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    out.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
    Ok(out)
}
