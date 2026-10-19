//! Uygulama kaldırıcı: kurulu uygulamaları listeler, artıklarını bulur.
//! Bu modül HİÇBİR ŞEY SİLMEZ; yalnızca yol döndürür. Çağıran `Plan::from_paths` ile plan kurar.
use crate::{Error, Result, scan::measure};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InstalledApp {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub bundle_id: Option<String>,
    pub version: Option<String>,
    pub size: u64,
    pub running: bool,
    pub last_used_days: Option<u64>,
    pub system: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Leftover {
    pub path: PathBuf,
    pub size: u64,
    /// "support" | "cache" | "preferences" | "container" | "launch" | "logs" | "state" | "other"
    pub kind: String,
    /// "high" | "medium"
    pub confidence: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UninstallPreview {
    pub app: InstalledApp,
    pub leftovers: Vec<Leftover>,
    pub total: u64,
    pub warnings: Vec<String>,
}

pub struct LeftoverRoots {
    pub dirs: Vec<(PathBuf, &'static str)>,
}

/// Dahili: uygulama + platforma özel ek bilgi.
struct Raw {
    app: InstalledApp,
    /// Windows: kayıtlı kaldırıcı komutu.
    uninstall: Option<String>,
    /// Linux: çalıştırılabilir dosya adı (süreç eşlemesi için).
    exec: Option<String>,
}

// ---------------------------------------------------------------- yardımcılar

/// Küçük harf, yalnız harf/rakam.
fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Çok genel adlar: ad eşlemesi yapılmaz (çarpışma riski).
const GENERIC: &[&str] = &[
    "app",
    "apps",
    "data",
    "common",
    "shared",
    "system",
    "setup",
    "installer",
    "update",
    "updater",
    "helper",
    "launcher",
    "client",
    "server",
    "tools",
    "utility",
    "utilities",
    "google",
    "microsoft",
    "adobe",
    "apple",
    "mozilla",
    "jetbrains",
    "java",
    "python",
    "node",
    "windows",
    "program",
    "programs",
    "library",
    "cache",
    "caches",
    "logs",
    "temp",
    "user",
    "users",
    "default",
    "config",
    "settings",
    "preferences",
    "support",
];

fn name_ok(n: &str) -> bool {
    n.len() >= 4 && !GENERIC.contains(&n)
}

/// `foo.plist`, `foo.savedState` gibi bilinen sonekleri atar.
fn strip_known_ext(name: &str) -> &str {
    for e in [
        ".plist",
        ".savedstate",
        ".binarycookies",
        ".log",
        ".desktop",
    ] {
        if name.len() > e.len()
            && name.is_char_boundary(name.len() - e.len())
            && name[name.len() - e.len()..].eq_ignore_ascii_case(e)
        {
            return &name[..name.len() - e.len()];
        }
    }
    name
}

/// `needle`, `hay` içinde '.' sınırlarında geçiyor mu? (`com.a.B` ≠ `com.a.BX`)
fn has_id(hay: &str, needle: &str) -> bool {
    let (h, n) = (hay.to_lowercase(), needle.to_lowercase());
    let mut from = 0;
    while let Some(i) = h.get(from..).and_then(|t| t.find(&n)) {
        let s = from + i;
        let e = s + n.len();
        let before = s == 0 || h.as_bytes()[s - 1] == b'.';
        let after = e == h.len() || h.as_bytes()[e] == b'.';
        if before && after {
            return true;
        }
        from = s + 1;
    }
    false
}

/// Kullanıcının yazabildiği bir yol mu? (Çöp kutusuna taşıma ayrıcalık gerektirmesin.)
fn writable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let Some(parent) = p.parent() else {
            return false;
        };
        let Ok(c) = std::ffi::CString::new(parent.as_os_str().as_bytes()) else {
            return false;
        };
        // SAFETY: c geçerli, NUL sonlu bir C dizgesi.
        unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
    }
    #[cfg(not(unix))]
    {
        p.parent().is_some()
    }
}

/// Öğeleri iş parçacıklarına dağıtıp sırayı koruyarak sonuç döndürür.
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let n = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(items.len().max(1));
    let next = AtomicUsize::new(0);
    let out: Vec<Mutex<Option<R>>> = items.iter().map(|_| Mutex::new(None)).collect();
    std::thread::scope(|s| {
        for _ in 0..n {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(it) = items.get(i) else { break };
                    *out[i].lock().unwrap() = Some(f(it));
                }
            });
        }
    });
    out.into_iter()
        .filter_map(|m| m.into_inner().unwrap())
        .collect()
}

fn lower_path(p: &Path) -> PathBuf {
    PathBuf::from(p.to_string_lossy().to_lowercase())
}

// ---------------------------------------------------------------- artık bulucu

pub fn default_roots() -> LeftoverRoots {
    #[allow(unused_mut)]
    let mut dirs: Vec<(PathBuf, &'static str)> = vec![];
    #[cfg(target_os = "macos")]
    {
        if let Some(h) = dirs::home_dir() {
            let l = h.join("Library");
            for (d, k) in [
                ("Application Support", "support"),
                ("Caches", "cache"),
                ("Preferences", "preferences"),
                ("Containers", "container"),
                ("Group Containers", "container"),
                ("Saved Application State", "state"),
                ("Logs", "logs"),
                ("HTTPStorages", "state"),
                ("WebKit", "state"),
                ("LaunchAgents", "launch"),
                ("Cookies", "state"),
                ("Application Scripts", "support"),
            ] {
                dirs.push((l.join(d), k));
            }
        }
        // Yönetici gerektirir; yazılamazsa uninstall_paths atlar.
        for (d, k) in [
            ("Application Support", "support"),
            ("LaunchAgents", "launch"),
            ("LaunchDaemons", "launch"),
            ("Preferences", "preferences"),
        ] {
            dirs.push((Path::new("/Library").join(d), k));
        }
    }
    #[cfg(windows)]
    {
        if let Some(d) = dirs::config_dir() {
            dirs.push((d, "support"));
        }
        if let Some(d) = dirs::data_local_dir() {
            dirs.push((d.join("Programs"), "support"));
            dirs.push((d, "support"));
        }
        if let Some(d) = std::env::var_os("PROGRAMDATA") {
            dirs.push((PathBuf::from(d), "support"));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(d) = dirs::config_dir() {
            dirs.push((d, "preferences"));
        }
        if let Some(d) = dirs::cache_dir() {
            dirs.push((d, "cache"));
        }
        if let Some(d) = dirs::data_dir() {
            dirs.push((d, "support"));
        }
        if let Some(h) = dirs::home_dir() {
            dirs.push((h.join(".local/state"), "state"));
            dirs.push((h.join(".var/app"), "container"));
        }
    }
    LeftoverRoots { dirs }
}

/// Kökler altında yalnızca uygulamaya ait girdileri bulur. Bundle kimliği eşleşmesi = high,
/// tam ad eşleşmesi = medium. Kökün kendisini ya da üst klasörleri asla döndürmez.
pub fn leftovers_in(roots: &LeftoverRoots, name: &str, bundle_id: Option<&str>) -> Vec<Leftover> {
    let bid = bundle_id
        .map(str::trim)
        .filter(|b| b.len() >= 5 && b.contains('.') && !b.to_lowercase().starts_with("com.apple."));
    let nn = norm(name);
    let by_name = name_ok(&nn);
    let mut found: Vec<(PathBuf, &'static str, &'static str)> = vec![];
    for (root, kind) in &roots.dirs {
        let Ok(rd) = std::fs::read_dir(root) else {
            continue;
        };
        for e in rd.flatten() {
            let fname = e.file_name().to_string_lossy().into_owned();
            let path = e.path();
            if path == *root {
                continue;
            }
            let stem = strip_known_ext(&fname);
            let conf = if bid.is_some_and(|b| has_id(&fname, b) || has_id(stem, b)) {
                "high"
            } else if by_name && norm(stem) == nn {
                "medium"
            } else {
                continue;
            };
            found.push((path, kind, conf));
        }
    }
    // Aynı yol iki kez ya da biri ötekinin altında ise yalnız üsttekini tut (çifte sayım yok).
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found.dedup_by(|b, a| {
        if a.0 == b.0 {
            if b.2 == "high" {
                a.2 = "high";
            }
            true
        } else {
            false
        }
    });
    let all: Vec<PathBuf> = found.iter().map(|f| f.0.clone()).collect();
    found.retain(|f| !all.iter().any(|o| *o != f.0 && f.0.starts_with(o)));
    let sizes = par_map(&found, |f| measure(&f.0).0);
    let mut v: Vec<Leftover> = found
        .into_iter()
        .zip(sizes)
        .map(|((path, kind, conf), size)| Leftover {
            path,
            size,
            kind: kind.into(),
            confidence: conf.into(),
        })
        .collect();
    v.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
    v
}

// ---------------------------------------------------------------- liste

type Procs = Vec<(String, Option<PathBuf>)>;

fn running_procs() -> Procs {
    let mut s = System::new();
    s.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .without_tasks()
            .with_exe(UpdateKind::OnlyIfNotSet),
    );
    s.processes()
        .values()
        .map(|p| {
            (
                p.name().to_string_lossy().to_lowercase(),
                p.exe().map(lower_path),
            )
        })
        .collect()
}

fn is_running(r: &Raw, procs: &Procs) -> bool {
    let p = &r.app.path;
    let by_exe = !p.as_os_str().is_empty() && p.is_dir() && {
        let lp = lower_path(p);
        procs
            .iter()
            .any(|(_, e)| e.as_ref().is_some_and(|e| e.starts_with(&lp)))
    };
    by_exe
        || r.exec
            .as_ref()
            .is_some_and(|x| procs.iter().any(|(n, _)| *n == x.to_lowercase()))
}

/// Pahalı alanlar: boyut ve son kullanım.
fn fill(app: &mut InstalledApp) {
    if !app.path.as_os_str().is_empty() {
        app.size = measure(&app.path).0;
    }
    #[cfg(target_os = "macos")]
    {
        app.last_used_days = last_used_days(&app.path);
    }
}

fn raw_list() -> Vec<Raw> {
    let mut v = platform_scan();
    let mut seen = std::collections::HashSet::new();
    v.retain(|r| seen.insert(r.app.id.clone()));
    let procs = running_procs();
    for r in &mut v {
        r.app.running = is_running(r, &procs);
    }
    v
}

/// Kurulu uygulamalar, boyuta göre büyükten küçüğe.
pub fn installed_apps() -> Vec<InstalledApp> {
    let apps: Vec<InstalledApp> = raw_list().into_iter().map(|r| r.app).collect();
    let mut apps = par_map(&apps, |a| {
        let mut a = a.clone();
        fill(&mut a);
        a
    });
    apps.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
    apps
}

/// Uygulamayla birlikte gidecek her şey. Çalışan uygulama uyarı alır; sistem uygulaması reddedilir.
pub fn preview(app_id: &str) -> Result<UninstallPreview> {
    let raw = raw_list()
        .into_iter()
        .find(|r| r.app.id == app_id)
        .ok_or_else(|| Error::NotFound(app_id.into()))?;
    let mut app = raw.app;
    if app.system {
        return Err(Error::Guard(format!("{} is a system app", app.name)));
    }
    fill(&mut app);
    let mut warnings = vec![];
    if app.running {
        warnings.push(format!(
            "{} is running; quit it before uninstalling.",
            app.name
        ));
    }
    if let Some(u) = &raw.uninstall {
        warnings.push(format!(
            "Run the registered uninstaller first ({u}); registry entries are not cleaned by Ebb."
        ));
    }
    let mut leftovers = leftovers_in(&default_roots(), &app.name, app.bundle_id.as_deref());
    // Uygulamanın kendisi ya da üst klasörü artık sayılmaz.
    leftovers.retain(|l| {
        app.path.as_os_str().is_empty()
            || !(l.path.starts_with(&app.path) || app.path.starts_with(&l.path))
    });
    for l in &leftovers {
        if !writable(&l.path) {
            warnings.push(format!("Needs administrator: {}", l.path.display()));
        }
    }
    let total = app.size + leftovers.iter().map(|l| l.size).sum::<u64>();
    Ok(UninstallPreview {
        app,
        leftovers,
        total,
        warnings,
    })
}

/// `Plan::from_paths` girdisi: önce uygulama, sonra artıklar (yazılamayanlar atlanır).
pub fn uninstall_paths(p: &UninstallPreview, include_medium: bool) -> Vec<PathBuf> {
    let mut v = vec![];
    if !p.app.path.as_os_str().is_empty() {
        v.push(p.app.path.clone());
    }
    v.extend(
        p.leftovers
            .iter()
            .filter(|l| (include_medium || l.confidence == "high") && writable(&l.path))
            .map(|l| l.path.clone()),
    );
    v
}

#[allow(dead_code)]
fn blank(id: String, name: String, path: PathBuf) -> InstalledApp {
    InstalledApp {
        id,
        name,
        path,
        bundle_id: None,
        version: None,
        size: 0,
        running: false,
        last_used_days: None,
        system: false,
    }
}

// ---------------------------------------------------------------- macOS

#[cfg(target_os = "macos")]
fn platform_scan() -> Vec<Raw> {
    let mut dirs = vec![PathBuf::from("/Applications")];
    if let Some(h) = dirs::home_dir() {
        dirs.push(h.join("Applications"));
    }
    let is_app = |p: &Path| p.extension().is_some_and(|e| e == "app");
    let mut paths = vec![];
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if is_app(&p) {
                paths.push(p);
            } else if e.file_type().is_ok_and(|t| t.is_dir())
                && let Ok(sub) = std::fs::read_dir(&p)
            {
                // Bir seviye alt klasör (Utilities, satıcı klasörleri).
                paths.extend(sub.flatten().map(|s| s.path()).filter(|s| is_app(s)));
            }
        }
    }
    par_map(&paths, |p| mac_app(p))
}

#[cfg(target_os = "macos")]
fn mac_app(path: &Path) -> Raw {
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let info = std::process::Command::new("plutil")
        .args(["-convert", "json", "-o", "-", "--"])
        .arg(path.join("Contents/Info.plist"))
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok());
    let get = |k: &str| {
        info.as_ref()
            .and_then(|i| i.get(k))
            .and_then(|v| v.as_str())
            .map(String::from)
    };
    let mut app = blank(
        path.to_string_lossy().into_owned(),
        name,
        path.to_path_buf(),
    );
    app.bundle_id = get("CFBundleIdentifier");
    app.version = get("CFBundleShortVersionString");
    app.system = path.starts_with("/System/")
        || app
            .bundle_id
            .as_deref()
            .is_some_and(|b| b.to_lowercase().starts_with("com.apple."));
    Raw {
        app,
        uninstall: None,
        exec: None,
    }
}

#[cfg(target_os = "macos")]
fn last_used_days(path: &Path) -> Option<u64> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    let from_mdls = std::process::Command::new("mdls")
        .args(["-raw", "-name", "kMDItemLastUsedDate"])
        .arg(path)
        .output()
        .ok()
        .and_then(|o| parse_mdls_date(&String::from_utf8_lossy(&o.stdout)));
    match from_mdls {
        Some(ts) => Some(((now - ts).max(0) / 86400) as u64),
        // atime yedeği
        None => std::fs::metadata(path)
            .ok()?
            .accessed()
            .ok()?
            .elapsed()
            .ok()
            .map(|d| d.as_secs() / 86400),
    }
}

/// "2026-10-01 12:00:00 +0000" -> unix saniye (UTC).
#[cfg(target_os = "macos")]
fn parse_mdls_date(s: &str) -> Option<i64> {
    let s = s.get(..19)?;
    let n = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let (y, m, d) = (n(0, 4)?, n(5, 7)?, n(8, 10)?);
    let (hh, mm, ss) = (n(11, 13)?, n(14, 16)?, n(17, 19)?);
    // Hinnant days_from_civil
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hh * 3600 + mm * 60 + ss)
}

// ---------------------------------------------------------------- Windows

#[cfg(windows)]
fn platform_scan() -> Vec<Raw> {
    let mut v = vec![];
    for root in [
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
    ] {
        let Ok(o) = std::process::Command::new("reg")
            .args(["query", root, "/s"])
            .output()
        else {
            continue;
        };
        v.extend(parse_reg(&String::from_utf8_lossy(&o.stdout)));
    }
    v
}

/// `reg query /s` çıktısını ayrıştırır.
#[cfg(any(windows, test))]
fn parse_reg(out: &str) -> Vec<Raw> {
    use std::collections::HashMap;
    let mut blocks: Vec<(String, HashMap<String, String>)> = vec![];
    for line in out.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(char::is_whitespace) {
            blocks.push((line.trim().to_string(), HashMap::new()));
        } else if let Some((_, vals)) = blocks.last_mut() {
            let t = line.trim();
            if let Some(i) = t.find("    REG_") {
                let rest = t[i..].trim_start();
                if let Some((_, val)) = rest.split_once(char::is_whitespace) {
                    vals.insert(t[..i].trim().to_string(), val.trim().to_string());
                }
            }
        }
    }
    let mut v = vec![];
    for (key, vals) in blocks {
        let Some((_, id)) = key.split_once("\\Uninstall\\") else {
            continue;
        };
        if id.contains('\\') || vals.contains_key("ParentKeyName") {
            continue;
        }
        let Some(name) = vals.get("DisplayName").filter(|n| !n.is_empty()) else {
            continue;
        };
        let loc = vals
            .get("InstallLocation")
            .map(|l| l.trim().trim_matches('"').to_string())
            .filter(|l| !l.is_empty() && Path::new(l).is_dir())
            .unwrap_or_default();
        let mut app = blank(id.to_string(), name.clone(), PathBuf::from(loc));
        app.version = vals
            .get("DisplayVersion")
            .cloned()
            .filter(|s| !s.is_empty());
        app.system = vals.get("SystemComponent").is_some_and(|s| s == "0x1");
        v.push(Raw {
            app,
            uninstall: vals
                .get("UninstallString")
                .cloned()
                .filter(|s| !s.is_empty()),
            exec: None,
        });
    }
    v
}

// ---------------------------------------------------------------- Linux

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_scan() -> Vec<Raw> {
    let home = dirs::home_dir();
    let mut v = vec![];
    let mut desk = vec![];
    if let Some(h) = &home {
        desk.push(h.join(".local/share/applications"));
    }
    desk.push(PathBuf::from("/usr/share/applications"));
    for d in desk {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x != "desktop") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            let Some((name, exec)) = parse_desktop(&text) else {
                continue;
            };
            let id = p
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mut app = blank(id.clone(), name, p.clone());
            app.bundle_id = Some(id);
            // Paket yöneticisinin dosyaları çöpe taşınamaz.
            app.system = p.starts_with("/usr");
            v.push(Raw {
                app,
                uninstall: None,
                exec,
            });
        }
    }
    // Flatpak: kimlik klasörlerinden.
    let mut fp = vec![PathBuf::from("/var/lib/flatpak/app")];
    if let Some(h) = &home {
        fp.insert(0, h.join(".local/share/flatpak/app"));
    }
    for d in fp {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let id = e.file_name().to_string_lossy().into_owned();
            let name = id.rsplit('.').next().unwrap_or(&id).to_string();
            let mut app = blank(id.clone(), name, e.path());
            app.bundle_id = Some(id);
            app.system = !writable(&e.path());
            v.push(Raw {
                app,
                uninstall: None,
                exec: None,
            });
        }
    }
    v
}

/// .desktop içeriğinden (Name, Exec dosya adı); gizli/uygulama olmayanlar None.
#[cfg(any(all(unix, not(target_os = "macos")), test))]
fn parse_desktop(text: &str) -> Option<(String, Option<String>)> {
    let (mut name, mut exec, mut hidden, mut in_entry) = (None, None, false, false);
    let mut is_app = true;
    for l in text.lines() {
        let l = l.trim();
        if l.starts_with('[') {
            in_entry = l == "[Desktop Entry]";
        } else if in_entry && let Some((k, v)) = l.split_once('=') {
            match k {
                "Name" => name = Some(v.trim().to_string()),
                "Exec" => {
                    exec = v
                        .split_whitespace()
                        .next()
                        .map(|x| x.trim_matches('"'))
                        .and_then(|x| Path::new(x).file_name())
                        .map(|x| x.to_string_lossy().into_owned())
                }
                "NoDisplay" | "Hidden" => hidden |= v.trim() == "true",
                "Type" => is_app = v.trim() == "Application",
                _ => {}
            }
        }
    }
    if hidden || !is_app {
        return None;
    }
    name.filter(|n| !n.is_empty()).map(|n| (n, exec))
}

#[cfg(not(any(target_os = "macos", windows, unix)))]
fn platform_scan() -> Vec<Raw> {
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reg_and_desktop_parsers() {
        let out = "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Foo_is1\r\n    DisplayName    REG_SZ    Foo Editor\r\n    DisplayVersion    REG_SZ    1.2\r\n    UninstallString    REG_SZ    \"C:\\Foo\\unins.exe\"\r\n\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Sys\r\n    DisplayName    REG_SZ    Sys\r\n    SystemComponent    REG_DWORD    0x1\r\n";
        let r = parse_reg(out);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].app.id, "Foo_is1");
        assert_eq!(r[0].app.name, "Foo Editor");
        assert_eq!(r[0].app.version.as_deref(), Some("1.2"));
        assert!(r[0].uninstall.is_some() && !r[0].app.system);
        assert!(r[1].app.system);
        let d = "[Desktop Entry]\nName=Foo\nExec=/usr/bin/foo %U\nType=Application\n";
        assert_eq!(parse_desktop(d), Some(("Foo".into(), Some("foo".into()))));
        assert_eq!(
            parse_desktop("[Desktop Entry]\nName=X\nNoDisplay=true\n"),
            None
        );
    }
}
