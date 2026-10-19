//! Sistem sağlığı: bellek, ağır uygulamalar, başlangıç öğeleri ve dürüst öneriler.
//! "RAM temizleme" gibi sahte işlemler yok; yalnızca gerçek kaldıraçlar (uygulama kapatma,
//! başlangıç öğesi kapatma, disk temizliği, yeniden başlatma).
use crate::{Error, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[derive(Serialize, Clone, Debug)]
pub struct MemoryInfo {
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    /// 0..1 = 1 - available/total. macOS'ta available çekirdeğin `kern.memorystatus_level` oranından gelir.
    pub pressure: f32,
}

#[derive(Serialize, Clone, Debug)]
pub struct AppUsage {
    pub name: String,
    pub pids: Vec<u32>,
    /// Süreçlerin yerleşik bellek (RSS) toplamı; paylaşılan sayfalar birden çok kez sayılabilir.
    pub memory: u64,
    /// Tek çekirdeğin yüzdesi (Activity Monitor gibi; 100'ü aşabilir).
    pub cpu: f32,
    pub quittable: bool,
    pub path: Option<PathBuf>,
}

#[derive(Serialize, Clone, Debug)]
pub struct StartupItem {
    /// `{scope}:{kind}:{key}`; açma/kapama boyunca sabit kalır.
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    /// "user" | "system"
    pub scope: &'static str,
    /// "launch_agent" | "run_key" | "startup_folder" | "autostart"
    pub kind: &'static str,
    pub enabled: bool,
    pub can_toggle: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct Tip {
    pub id: &'static str,
    /// "good" | "info" | "warn" | "bad"
    pub severity: &'static str,
    pub title: String,
    pub body: String,
    /// "clean" | "apps" | "startup" | "restart" | "optimize"
    pub action: Option<&'static str>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Health {
    pub score: u8,
    pub memory: MemoryInfo,
    pub disk_free_ratio: f32,
    pub uptime_secs: u64,
    pub startup_enabled: usize,
    pub tips: Vec<Tip>,
}

// ---------------------------------------------------------------- bellek

pub fn memory() -> MemoryInfo {
    let mut s = System::new();
    s.refresh_memory();
    let total = s.total_memory();
    // macOS'ta sysinfo `available` 0 dönebiliyor; çekirdeğin boş bellek oranı daha doğru.
    let available = match os_free_ratio() {
        Some(r) => (total as f64 * r as f64) as u64,
        None => s.available_memory(),
    }
    .min(total);
    let pressure = if total == 0 {
        0.0
    } else {
        1.0 - available as f32 / total as f32
    };
    MemoryInfo {
        total,
        used: s.used_memory().min(total),
        available,
        swap_total: s.total_swap(),
        swap_used: s.used_swap(),
        pressure: pressure.clamp(0.0, 1.0),
    }
}

/// macOS'ta "boş bellek yüzdesi" (memory_pressure aracının gösterdiği değer), 0..1.
#[cfg(target_os = "macos")]
fn os_free_ratio() -> Option<f32> {
    let mut v: libc::c_int = 0;
    let mut len = std::mem::size_of::<libc::c_int>();
    let r = unsafe {
        libc::sysctlbyname(
            c"kern.memorystatus_level".as_ptr(),
            (&mut v as *mut libc::c_int).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (r == 0 && (0..=100).contains(&v)).then(|| v as f32 / 100.0)
}

#[cfg(not(target_os = "macos"))]
fn os_free_ratio() -> Option<f32> {
    None
}

// ---------------------------------------------------------------- uygulamalar

const CPU_SAMPLE: std::time::Duration = std::time::Duration::from_millis(250);

fn processes(sample_cpu: bool) -> System {
    let kind = ProcessRefreshKind::nothing()
        .without_tasks()
        .with_memory()
        .with_exe(UpdateKind::OnlyIfNotSet)
        .with_user(UpdateKind::OnlyIfNotSet);
    let mut s = System::new();
    if sample_cpu {
        s.refresh_processes_specifics(ProcessesToUpdate::All, true, kind.with_cpu());
        std::thread::sleep(CPU_SAMPLE.max(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL));
        s.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .without_tasks()
                .with_memory()
                .with_cpu(),
        );
    } else {
        s.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    }
    s
}

/// Asla kapatılmayacak süreç adları (küçük harf, `.exe` olmadan). Tüm platformlarda uygulanır.
const CRITICAL: &[&str] = &[
    // macOS
    "kernel_task",
    "launchd",
    "windowserver",
    "loginwindow",
    "finder",
    "dock",
    "systemuiserver",
    "controlcenter",
    "coreaudiod",
    "notificationcenter",
    "windowmanager",
    "cfprefsd",
    "distnoted",
    "usereventagent",
    "trustd",
    "securityd",
    "mds",
    "mds_stores",
    "mdworker",
    "fseventsd",
    "logd",
    "configd",
    "powerd",
    // Windows
    "system",
    "idle",
    "registry",
    "smss",
    "csrss",
    "wininit",
    "winlogon",
    "services",
    "lsass",
    "explorer",
    "dwm",
    "svchost",
    "fontdrvhost",
    "sihost",
    "taskhostw",
    "ctfmon",
    "runtimebroker",
    "startmenuexperiencehost",
    "shellexperiencehost",
    "searchhost",
    "logonui",
    "conhost",
    "audiodg",
    "spoolsv",
    "msmpeng",
    // Linux
    "systemd",
    "init",
    "xorg",
    "xwayland",
    "gnome-shell",
    "gnome-session-binary",
    "plasmashell",
    "ksmserver",
    "dbus-daemon",
    "dbus-broker",
    "pipewire",
    "wireplumber",
    "pulseaudio",
    "gdm",
    "sddm",
    "lightdm",
    "mutter",
    "xfwm4",
    "xfce4-session",
    "cinnamon",
    "sway",
    "hyprland",
];

fn critical_name(name: &str) -> bool {
    let n = name.to_lowercase();
    let n = n.strip_suffix(".exe").unwrap_or(&n);
    CRITICAL.contains(&n) || n.starts_with("kwin")
}

fn system_location(exe: &Path) -> bool {
    let s = exe.to_string_lossy().replace('\\', "/").to_lowercase();
    // Linux'ta kullanıcı uygulamaları da /usr/bin altında; orada yalnızca sistem servis yolları engellenir
    // (sahiplik ve ad kontrolü gerisini tutar).
    let roots: &[&str] = if cfg!(target_os = "linux") {
        &[
            "/sbin/",
            "/usr/sbin/",
            "/usr/libexec/",
            "/usr/lib/systemd/",
            "/lib/systemd/",
        ]
    } else {
        &[
            "/system/",
            "/usr/",
            "/sbin/",
            "/bin/",
            "/library/apple/",
            "c:/windows/",
        ]
    };
    roots.iter().any(|r| s.starts_with(r))
}

/// Süreç güvenle kapatılabilir mi; değilse neden.
fn quit_check(s: &System, pid: u32) -> std::result::Result<(), &'static str> {
    let me = std::process::id();
    let mine = s.process(Pid::from_u32(me));
    if pid <= 1
        || pid == me
        || mine
            .and_then(|m| m.parent())
            .is_some_and(|p| p.as_u32() == pid)
    {
        return Err("Ebb itself or a core system process");
    }
    let p = s.process(Pid::from_u32(pid)).ok_or("not running")?;
    let owner = mine.and_then(|m| m.user_id());
    if owner.is_none() || p.user_id() != owner {
        return Err("owned by another user");
    }
    let exe = p.exe().filter(|e| !e.as_os_str().is_empty());
    let stem = exe.and_then(|e| e.file_stem()).map(|n| n.to_string_lossy());
    if critical_name(&p.name().to_string_lossy()) || stem.is_some_and(|n| critical_name(&n)) {
        return Err("system process");
    }
    match exe {
        None => Err("unknown executable"),
        Some(e) if system_location(e) => Err("system location"),
        Some(_) => Ok(()),
    }
}

/// Grup adı ve yolu: macOS'ta en dıştaki `X.app`, aksi halde çalıştırılabilir dosyanın adı.
fn app_of(name: &std::ffi::OsStr, exe: Option<&Path>) -> (String, Option<PathBuf>) {
    if cfg!(target_os = "macos")
        && let Some(app) = exe.and_then(|e| {
            e.ancestors()
                .filter(|a| a.extension().is_some_and(|x| x.eq_ignore_ascii_case("app")))
                .last()
        })
        && let Some(stem) = app.file_stem()
    {
        return (stem.to_string_lossy().into_owned(), Some(app.to_path_buf()));
    }
    let file = exe.and_then(|e| {
        if cfg!(windows) {
            e.file_stem()
        } else {
            e.file_name()
        }
    });
    let n = file.unwrap_or(name).to_string_lossy();
    let n = n.strip_suffix(".exe").unwrap_or(&n).to_string();
    (n, exe.map(Path::to_path_buf))
}

fn group_apps(s: &System) -> Vec<AppUsage> {
    let mut map: HashMap<String, AppUsage> = HashMap::new();
    for (pid, p) in s.processes() {
        if p.thread_kind().is_some() {
            continue;
        }
        let exe = p.exe().filter(|e| !e.as_os_str().is_empty());
        let (name, path) = app_of(p.name(), exe);
        let ok = quit_check(s, pid.as_u32()).is_ok();
        let a = map.entry(name.clone()).or_insert_with(|| AppUsage {
            name,
            pids: vec![],
            memory: 0,
            cpu: 0.0,
            quittable: true,
            path,
        });
        a.pids.push(pid.as_u32());
        a.memory += p.memory();
        a.cpu += p.cpu_usage();
        a.quittable &= ok;
    }
    let mut v: Vec<AppUsage> = map.into_values().collect();
    for a in &mut v {
        a.pids.sort_unstable();
    }
    v.sort_by(|a, b| b.memory.cmp(&a.memory).then_with(|| a.name.cmp(&b.name)));
    v
}

/// Processes grouped by app (macOS: outermost enclosing .app bundle; Windows/Linux: executable name),
/// heaviest memory first. CPU from a ~300 ms sample.
pub fn top_apps(limit: usize) -> Vec<AppUsage> {
    let mut v = group_apps(&processes(true));
    v.truncate(limit);
    v
}

/// Graceful quit of every pid of an app (SIGTERM on unix, `taskkill /PID` without /F on Windows).
/// Returns how many were signalled. Refuses the whole call if any pid fails the safety rules.
pub fn quit_app(pids: &[u32]) -> Result<usize> {
    if pids.is_empty() {
        return Ok(0);
    }
    let s = processes(false);
    for &pid in pids {
        quit_check(&s, pid).map_err(|why| Error::Guard(format!("process {pid} ({why})")))?;
    }
    Ok(pids.iter().filter(|&&pid| terminate(pid)).count())
}

#[cfg(unix)]
fn terminate(pid: u32) -> bool {
    // u32 -> pid_t taşması negatif olup süreç grubuna sinyal gönderebilirdi; try_from bunu engeller.
    libc::pid_t::try_from(pid).is_ok_and(|p| p > 1 && unsafe { libc::kill(p, libc::SIGTERM) } == 0)
}

#[cfg(windows)]
fn terminate(pid: u32) -> bool {
    cmd("taskkill")
        .args(["/PID", &pid.to_string()])
        .output()
        .is_ok_and(|o| o.status.success())
}

#[cfg(not(any(unix, windows)))]
fn terminate(_pid: u32) -> bool {
    false
}

#[cfg(windows)]
fn cmd(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut c = std::process::Command::new(program);
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: GUI'den çağrılınca konsol yanıp sönmesin.
    c
}

// ---------------------------------------------------------------- başlangıç öğeleri

/// Kapatılan dosya öğelerinin taşındığı alt klasör (macOS LaunchAgents, Windows Startup).
pub const DISABLED_DIR: &str = "Disabled by Ebb";

fn plain_name(file: &str) -> Result<()> {
    if file.is_empty() || Path::new(file).file_name() != Some(std::ffi::OsStr::new(file)) {
        return Err(Error::Guard(format!("{file} (not a plain file name)")));
    }
    Ok(())
}

fn dir_items(
    dir: &Path,
    scope: &'static str,
    kind: &'static str,
    ext: &str,
    enabled: bool,
    name_of: fn(&Path) -> String,
) -> Vec<StartupItem> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut v: Vec<StartupItem> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter_map(|p| {
            let file = p.file_name()?.to_string_lossy().into_owned();
            let ext_ok = ext.is_empty()
                || p.extension()
                    .is_some_and(|x| x.to_string_lossy().eq_ignore_ascii_case(ext));
            if !ext_ok || file.starts_with('.') || file.eq_ignore_ascii_case("desktop.ini") {
                return None;
            }
            let key = if scope == "user" {
                file
            } else {
                p.to_string_lossy().into_owned()
            };
            Some(StartupItem {
                id: format!("{scope}:{kind}:{key}"),
                name: name_of(&p),
                path: p,
                scope,
                kind,
                enabled,
                can_toggle: scope == "user",
            })
        })
        .collect();
    v.sort_by(|a, b| a.id.cmp(&b.id));
    v
}

/// Kullanıcı klasörü + içindeki "Disabled by Ebb" klasörü.
fn user_dir_items(
    dir: &Path,
    kind: &'static str,
    ext: &str,
    name_of: fn(&Path) -> String,
) -> Vec<StartupItem> {
    let mut v = dir_items(dir, "user", kind, ext, true, name_of);
    v.extend(dir_items(
        &dir.join(DISABLED_DIR),
        "user",
        kind,
        ext,
        false,
        name_of,
    ));
    v
}

fn file_stem(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// plist'teki `Label`; ikili plist ya da okunamazsa dosya adı.
fn plist_label(p: &Path) -> String {
    std::fs::read(p)
        .ok()
        .and_then(|b| {
            let t = String::from_utf8_lossy(&b);
            let (_, rest) = t.split_once("<key>Label</key>")?;
            let (gap, rest) = rest.split_once("<string>")?;
            let (label, _) = rest.split_once("</string>")?;
            let label = label.trim();
            (gap.trim().is_empty() && !label.is_empty()).then(|| label.to_string())
        })
        .unwrap_or_else(|| file_stem(p))
}

/// macOS LaunchAgents listesi, açık dizinlerle (testlerde geçici klasör verilir).
/// `user_dir` öğeleri açılıp kapatılabilir; `user_dir/Disabled by Ebb` içindekiler `enabled: false`.
pub fn launch_agents_in(user_dir: &Path, system_dirs: &[PathBuf]) -> Vec<StartupItem> {
    let mut v = user_dir_items(user_dir, "launch_agent", "plist", plist_label);
    for d in system_dirs {
        v.extend(dir_items(
            d,
            "system",
            "launch_agent",
            "plist",
            true,
            plist_label,
        ));
    }
    v
}

/// Dosyayı `dir` ile `dir/Disabled by Ebb` arasında taşır; yeni yolu döndürür. Zaten istenen
/// durumdaysa dokunmaz.
pub fn move_item(dir: &Path, file: &str, enabled: bool) -> Result<PathBuf> {
    plain_name(file)?;
    let on = dir.join(file);
    let off = dir.join(DISABLED_DIR).join(file);
    let (from, to) = if enabled { (off, on) } else { (on, off) };
    let present = |p: &Path| p.symlink_metadata().is_ok();
    match (present(&from), present(&to)) {
        (false, true) => Ok(to),
        (true, true) => Err(Error::Guard(format!(
            "{} (exists in both enabled and disabled folders)",
            to.display()
        ))),
        (false, false) => Err(Error::NotFound(from.display().to_string())),
        (true, false) => {
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&from, &to)?;
            Ok(to)
        }
    }
}

fn desktop_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix(key)?.strip_prefix('='))
        .map(str::trim)
}

fn desktop_name(p: &Path) -> String {
    std::fs::read_to_string(p)
        .ok()
        .and_then(|t| desktop_value(&t, "Name").map(str::to_string))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| file_stem(p))
}

fn desktop_enabled(p: &Path) -> bool {
    let t = std::fs::read_to_string(p).unwrap_or_default();
    desktop_value(&t, "Hidden") != Some("true")
        && desktop_value(&t, "X-GNOME-Autostart-enabled") != Some("false")
}

/// Linux autostart listesi, açık dizinlerle. Kullanıcı dosyası aynı adlı sistem dosyasını gölgeler.
pub fn autostart_in(user_dir: &Path, system_dir: &Path) -> Vec<StartupItem> {
    let mut v = dir_items(user_dir, "user", "autostart", "desktop", true, desktop_name);
    let user_files: Vec<_> = v.iter().filter_map(|i| i.path.file_name()).collect();
    let system: Vec<StartupItem> = dir_items(
        system_dir,
        "system",
        "autostart",
        "desktop",
        true,
        desktop_name,
    )
    .into_iter()
    .filter(|i| !i.path.file_name().is_some_and(|f| user_files.contains(&f)))
    .collect();
    v.extend(system);
    for i in &mut v {
        i.enabled = desktop_enabled(&i.path);
    }
    v
}

/// `.desktop` dosyasında `Hidden=true` yazar ya da kaldırır (açarken GNOME'un kapatma anahtarını da siler).
pub fn set_desktop_hidden(file: &Path, hidden: bool) -> Result<()> {
    let text = std::fs::read_to_string(file)?;
    let mut out: Vec<&str> = text
        .lines()
        .filter(|l| {
            let l = l.trim_start();
            !l.starts_with("Hidden=") && (hidden || !l.starts_with("X-GNOME-Autostart-enabled="))
        })
        .collect();
    if hidden {
        let at = out
            .iter()
            .position(|l| l.trim() == "[Desktop Entry]")
            .map_or(0, |i| i + 1);
        out.insert(at, "Hidden=true");
    }
    std::fs::write(file, out.join("\n") + "\n")?;
    Ok(())
}

pub fn startup_items() -> Vec<StartupItem> {
    platform_startup_items()
}

/// id'si verilen kullanıcı öğesini açar/kapatır. Sistem öğeleri salt okunur (`Error::Guard`).
pub fn set_startup_enabled(id: &str, enabled: bool) -> Result<()> {
    let mut it = id.splitn(3, ':');
    let (Some(scope), Some(kind), Some(key)) = (it.next(), it.next(), it.next()) else {
        return Err(Error::NotFound(id.to_string()));
    };
    if scope != "user" {
        return Err(Error::Guard(format!(
            "{id} (system startup items are read-only)"
        )));
    }
    platform_toggle(kind, key, enabled)
}

#[cfg(target_os = "macos")]
fn platform_startup_items() -> Vec<StartupItem> {
    let Some(home) = dirs::home_dir() else {
        return vec![];
    };
    launch_agents_in(
        &home.join("Library/LaunchAgents"),
        &[
            PathBuf::from("/Library/LaunchAgents"),
            PathBuf::from("/Library/LaunchDaemons"),
        ],
    )
}

#[cfg(target_os = "macos")]
fn platform_toggle(kind: &str, file: &str, enabled: bool) -> Result<()> {
    use std::process::Command;
    if kind != "launch_agent" {
        return Err(Error::NotFound(format!("user:{kind}:{file}")));
    }
    plain_name(file)?;
    let dir = dirs::home_dir()
        .ok_or_else(|| Error::NotFound("home folder".into()))?
        .join("Library/LaunchAgents");
    let domain = format!("gui/{}", unsafe { libc::getuid() });
    if enabled {
        let plist = move_item(&dir, file, true)?;
        // En iyi çaba: yüklenemese bile dosya yerinde, bir sonraki girişte başlar.
        let _ = Command::new("launchctl")
            .arg("bootstrap")
            .arg(&domain)
            .arg(&plist)
            .output();
        return Ok(());
    }
    let plist = dir.join(file);
    if plist.is_file() {
        let o = Command::new("launchctl")
            .arg("bootout")
            .arg(&domain)
            .arg(&plist)
            .output()?;
        let err = String::from_utf8_lossy(&o.stderr);
        let benign = ["No such process", "not loaded", "Could not find"];
        if !o.status.success() && !benign.iter().any(|b| err.contains(b)) {
            return Err(Error::Io(std::io::Error::other(format!(
                "launchctl bootout failed: {}",
                err.trim()
            ))));
        }
    }
    move_item(&dir, file, false).map(|_| ())
}

#[cfg(target_os = "linux")]
fn platform_startup_items() -> Vec<StartupItem> {
    let Some(cfg) = dirs::config_dir() else {
        return vec![];
    };
    autostart_in(&cfg.join("autostart"), Path::new("/etc/xdg/autostart"))
}

#[cfg(target_os = "linux")]
fn platform_toggle(kind: &str, file: &str, enabled: bool) -> Result<()> {
    if kind != "autostart" {
        return Err(Error::NotFound(format!("user:{kind}:{file}")));
    }
    plain_name(file)?;
    let dir = dirs::config_dir()
        .ok_or_else(|| Error::NotFound("config folder".into()))?
        .join("autostart");
    let p = dir.join(file);
    if !p.is_file() {
        return Err(Error::NotFound(p.display().to_string()));
    }
    set_desktop_hidden(&p, !enabled)
}

#[cfg(windows)]
const RUN: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const RUN_OFF: &str = r"HKCU\Software\Ebb\DisabledRun";
#[cfg(windows)]
const RUN_SYSTEM: &str = r"HKLM\Software\Microsoft\Windows\CurrentVersion\Run";

/// `reg query` çıktısı: (ad, tür, veri). "(Default)" atlanır.
#[cfg(any(windows, test))]
fn parse_reg_query(out: &str) -> Vec<(String, String, String)> {
    out.lines()
        .filter_map(|l| {
            let mut parts = l.strip_prefix("    ")?.splitn(3, "    ");
            let (name, ty) = (parts.next()?, parts.next()?);
            let data = parts.next().unwrap_or("");
            (ty.starts_with("REG_") && !name.is_empty() && name != "(Default)")
                .then(|| (name.to_string(), ty.to_string(), data.to_string()))
        })
        .collect()
}

#[cfg(windows)]
fn reg_values(key: &str) -> Vec<(String, String, String)> {
    cmd("reg")
        .args(["query", key])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_reg_query(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

#[cfg(windows)]
fn reg(args: &[&str]) -> Result<()> {
    let o = cmd("reg").args(args).output()?;
    if o.status.success() {
        Ok(())
    } else {
        Err(Error::Io(std::io::Error::other(format!(
            "reg {} failed: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&o.stderr).trim()
        ))))
    }
}

#[cfg(windows)]
fn startup_folder() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(r"Microsoft\Windows\Start Menu\Programs\Startup"))
}

#[cfg(windows)]
fn platform_startup_items() -> Vec<StartupItem> {
    let run = |key: &str, scope: &'static str, enabled: bool| {
        reg_values(key)
            .into_iter()
            .map(move |(name, _, data)| StartupItem {
                id: format!("{scope}:run_key:{name}"),
                name,
                path: PathBuf::from(data),
                scope,
                kind: "run_key",
                enabled,
                can_toggle: scope == "user",
            })
    };
    let mut v: Vec<StartupItem> = run(RUN, "user", true)
        .chain(run(RUN_OFF, "user", false))
        .collect();
    if let Some(dir) = startup_folder() {
        v.extend(user_dir_items(&dir, "startup_folder", "", file_stem));
    }
    v.extend(run(RUN_SYSTEM, "system", true));
    v
}

#[cfg(windows)]
fn platform_toggle(kind: &str, key: &str, enabled: bool) -> Result<()> {
    match kind {
        "startup_folder" => {
            let dir = startup_folder().ok_or_else(|| Error::NotFound("Startup folder".into()))?;
            move_item(&dir, key, enabled).map(|_| ())
        }
        "run_key" => {
            let (from, to) = if enabled {
                (RUN_OFF, RUN)
            } else {
                (RUN, RUN_OFF)
            };
            let Some((name, ty, data)) = reg_values(from).into_iter().find(|v| v.0 == key) else {
                // Zaten istenen durumdaysa sorun yok.
                return if reg_values(to).iter().any(|v| v.0 == key) {
                    Ok(())
                } else {
                    Err(Error::NotFound(format!("user:run_key:{key}")))
                };
            };
            reg(&["add", to, "/v", &name, "/t", &ty, "/d", &data, "/f"])?;
            reg(&["delete", from, "/v", &name, "/f"])
        }
        _ => Err(Error::NotFound(format!("user:{kind}:{key}"))),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn platform_startup_items() -> Vec<StartupItem> {
    vec![]
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn platform_toggle(kind: &str, key: &str, _enabled: bool) -> Result<()> {
    Err(Error::NotFound(format!("user:{kind}:{key}")))
}

// ---------------------------------------------------------------- sağlık

const OS: &str = if cfg!(target_os = "macos") {
    "macOS"
} else if cfg!(windows) {
    "Windows"
} else {
    "Linux"
};
const MACHINE: &str = if cfg!(target_os = "macos") {
    "Mac"
} else if cfg!(windows) {
    "PC"
} else {
    "computer"
};
const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
const DAY: u64 = 86_400;

fn gb(b: u64) -> String {
    format!("{:.1} GB", b as f64 / GIB)
}

fn tip(
    id: &'static str,
    severity: &'static str,
    title: String,
    body: String,
    action: Option<&'static str>,
) -> Tip {
    Tip {
        id,
        severity,
        title,
        body,
        action,
    }
}

pub fn health() -> Health {
    let memory = memory();
    let disk_free_ratio = crate::ops::disk_usage()
        .filter(|d| d.total > 0)
        .map_or(1.0, |d| d.free as f32 / d.total as f32);
    let uptime_secs = System::uptime();
    let startup = startup_items();
    let startup_enabled = startup.iter().filter(|i| i.enabled).count();
    let user_enabled = startup.iter().filter(|i| i.enabled && i.can_toggle).count();
    let heaviest = group_apps(&processes(false))
        .into_iter()
        .find(|a| a.quittable);

    let mut score: i32 = 100;
    let mut tips = vec![];

    let free_pct = (disk_free_ratio * 100.0).floor() as u32;
    if disk_free_ratio < 0.10 {
        score -= if disk_free_ratio < 0.05 { 40 } else { 25 };
        tips.push(tip(
            "disk",
            if disk_free_ratio < 0.05 { "bad" } else { "warn" },
            "Low disk space".into(),
            format!(
                "Only {free_pct}% of your disk is free. {OS} slows down when it gets below about 10%."
            ),
            Some("clean"),
        ));
    } else if disk_free_ratio < 0.15 {
        score -= 10;
        tips.push(tip(
            "disk",
            "info",
            "Disk is getting full".into(),
            format!(
                "{free_pct}% of your disk is free. Clearing caches keeps {OS} comfortably above 10%."
            ),
            Some("clean"),
        ));
    }

    let p = memory.pressure;
    if p >= 0.8 {
        score -= if p >= 0.9 { 30 } else { 15 };
        tips.push(tip(
            "memory",
            if p >= 0.9 { "bad" } else { "warn" },
            "Memory is under pressure".into(),
            format!(
                "About {}% of memory is in use, so {OS} has to swap to disk. Quitting apps you are not using frees memory right away.",
                (p * 100.0).round() as u32
            ),
            Some("apps"),
        ));
    } else if p >= 0.7 {
        score -= 5;
    }

    if let Some(a) = heaviest.filter(|a| {
        a.memory as f64 >= 1.5 * GIB || (memory.total > 0 && a.memory >= memory.total / 5)
    }) {
        tips.push(tip(
            "top_app",
            if p >= 0.8 { "warn" } else { "info" },
            format!("{} is using {} of memory", a.name, gb(a.memory)),
            format!(
                "It is the heaviest app right now. If you are not using it, quitting {} frees that memory.",
                a.name
            ),
            Some("apps"),
        ));
    }

    let swap_ratio = if memory.total == 0 {
        0.0
    } else {
        memory.swap_used as f64 / memory.total as f64
    };
    if swap_ratio >= 0.25 {
        score -= if swap_ratio >= 0.5 { 10 } else { 5 };
        tips.push(tip(
            "swap",
            "info",
            format!("{} of memory is swapped to disk", gb(memory.swap_used)),
            "That is normal after heavy use, but swapped memory is slower. If things feel sluggish, quit heavy apps or restart.".into(),
            Some("restart"),
        ));
    }

    if startup_enabled >= 10 {
        score -= if startup_enabled >= 25 { 10 } else { 5 };
        let you_can = if user_enabled < startup_enabled {
            format!(" {user_enabled} of them can be turned off here.")
        } else {
            String::new()
        };
        tips.push(tip(
            "startup",
            "info",
            format!("{startup_enabled} items start with your {MACHINE}"),
            format!(
                "They launch at login and run in the background. Turn off the ones you don't need.{you_can}"
            ),
            Some("startup"),
        ));
    }

    let days = uptime_secs / DAY;
    if days >= 7 {
        score -= if days >= 30 { 10 } else { 5 };
        tips.push(tip(
            "uptime",
            if days >= 30 { "warn" } else { "info" },
            format!("Your {MACHINE} has been running for {days} days"),
            "A restart clears out leftover background processes and finishes installing updates."
                .into(),
            Some("restart"),
        ));
    }

    if !tips
        .iter()
        .any(|t| t.severity == "warn" || t.severity == "bad")
    {
        let body = if tips.is_empty() {
            "Plenty of free disk space and memory. Nothing needs attention right now."
        } else {
            "No urgent problems. The suggestions below are optional."
        };
        tips.insert(
            0,
            tip(
                "healthy",
                "good",
                format!("Your {MACHINE} looks healthy"),
                body.into(),
                None,
            ),
        );
    }
    let rank = |s: &str| match s {
        "bad" => 0,
        "warn" => 1,
        "good" => 2,
        _ => 3,
    };
    tips.sort_by_key(|t| rank(t.severity));

    Health {
        score: score.clamp(0, 100) as u8,
        memory,
        disk_free_ratio,
        uptime_secs,
        startup_enabled,
        tips,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_reg_query() {
        let out = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\r\n    (Default)    REG_SZ    \r\n    Spotify    REG_SZ    \"C:\\Users\\a\\Spotify.exe\" /minimized\r\n    Tool X    REG_EXPAND_SZ    %LOCALAPPDATA%\\x.exe\r\n\r\n";
        let v = super::parse_reg_query(out);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].0, "Spotify");
        assert_eq!(v[0].2, "\"C:\\Users\\a\\Spotify.exe\" /minimized");
        assert_eq!(
            (v[1].0.as_str(), v[1].1.as_str()),
            ("Tool X", "REG_EXPAND_SZ")
        );
    }
}
