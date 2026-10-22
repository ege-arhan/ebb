//! Finder'da görünmeyen "gizli alan" eylemleri (TM yerel anlık görüntüleri, journal, paket önbellekleri).
//! Her eylem önce tam komutuyla gösterilir; yalnızca açık istekle çalışır. Argümanlar sabittir, kabuk yok.
use crate::{Error, Result, connect::find_cli, ops::disk_usage, scan};
use serde::Serialize;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Serialize, Clone, Debug)]
pub struct HiddenItem {
    pub id: &'static str,
    pub label: String,
    pub description: String,
    /// Kullanıcıya gösterilen tam komut metni.
    pub command: String,
    pub estimate: Option<u64>,
    pub available: bool,
    pub needs_admin: bool,
    pub safety: &'static str,
}

#[derive(Serialize, Clone, Debug)]
pub struct HiddenResult {
    pub id: String,
    pub ok: bool,
    pub output: String,
    pub before: u64,
    pub after: u64,
}

struct Def {
    id: &'static str,
    label: &'static str,
    description: &'static str,
    os: &'static str,
    bin: &'static str,
    args: &'static [&'static str],
    admin: bool,
    safety: &'static str,
    /// Önbellek klasörleri (tahmin için ölçülür).
    dirs: &'static [&'static str],
}

/// thinlocalsnapshots tavanı: 50 GB. Araç yalnızca gereken kadarını inceltir.
pub const THIN_BYTES: &str = "50000000000";

const DEFS: &[Def] = &[
    Def {
        id: "tm-snapshots",
        label: "Time Machine local snapshots",
        description: "Thins local APFS snapshots that hold purgeable space Finder does not show. macOS keeps the newest.",
        os: "macos",
        bin: "tmutil",
        args: &["thinlocalsnapshots", "/", THIN_BYTES, "4"],
        admin: false,
        safety: "review",
        dirs: &[],
    },
    Def {
        id: "journal-vacuum",
        label: "systemd journal logs",
        description: "Shrinks the systemd journal to 200 MB, dropping the oldest log entries.",
        os: "linux",
        bin: "journalctl",
        args: &["--vacuum-size=200M"],
        admin: true,
        safety: "safe",
        dirs: &["/var/log/journal"],
    },
    Def {
        id: "apt-clean",
        label: "APT package cache",
        description: "Deletes downloaded .deb files. Installed packages are untouched.",
        os: "linux",
        bin: "apt-get",
        args: &["clean"],
        admin: true,
        safety: "safe",
        dirs: &["/var/cache/apt/archives"],
    },
    Def {
        id: "dnf-clean",
        label: "DNF package cache",
        description: "Deletes cached DNF metadata and packages.",
        os: "linux",
        bin: "dnf",
        args: &["clean", "all"],
        admin: true,
        safety: "safe",
        dirs: &["/var/cache/dnf"],
    },
    Def {
        id: "pacman-cache",
        label: "pacman package cache",
        description: "Deletes cached packages that are no longer installed.",
        os: "linux",
        bin: "pacman",
        args: &["-Sc", "--noconfirm"],
        admin: true,
        safety: "review",
        dirs: &["/var/cache/pacman/pkg"],
    },
    Def {
        id: "flatpak-unused",
        label: "Unused Flatpak runtimes",
        description: "Removes user-installed Flatpak runtimes no app needs any more.",
        os: "linux",
        bin: "flatpak",
        args: &["uninstall", "--user", "--unused", "-y"],
        admin: false,
        safety: "safe",
        dirs: &[],
    },
];

fn is_root() -> bool {
    #[cfg(unix)]
    {
        // SAFETY: geteuid argümansız, yan etkisiz.
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn base(bin: &Path, args: &[&str]) -> Command {
    let mut c = Command::new(bin);
    c.args(args)
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// Zaman aşımlı çalıştırma: (başarı, çıktı). Süre dolarsa çocuk öldürülür.
fn run_cmd(mut c: Command, timeout: Duration) -> (bool, String) {
    let Ok(mut child) = c.spawn() else {
        return (false, "could not start".into());
    };
    let drain = |s: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut b = String::new();
            if let Some(mut s) = s {
                let _ = s.read_to_string(&mut b);
            }
            b
        })
    };
    let out = drain(child.stdout.take().map(|s| Box::new(s) as _));
    let err = drain(child.stderr.take().map(|s| Box::new(s) as _));
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s.success()),
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let mut text = out.join().unwrap_or_default();
    text.push_str(&err.join().unwrap_or_default());
    if status.is_none() {
        text.push_str(&format!("\ntimed out after {} s", timeout.as_secs()));
    }
    (status == Some(true), text)
}

fn capture(bin: &str, args: &[&str]) -> Option<String> {
    let (ok, out) = run_cmd(base(&find_cli(bin)?, args), Duration::from_secs(5));
    ok.then_some(out)
}

fn sudo_ok() -> bool {
    capture("sudo", &["-n", "true"]).is_some()
}

fn tail(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.trim().to_string();
    }
    let mut i = s.len() - max;
    while !s.is_char_boundary(i) {
        i += 1;
    }
    s[i..].trim().to_string()
}

/// `tmutil listlocalsnapshots /` çıktısından anlık görüntü tarihleri ("2024-01-02-123456").
pub fn parse_snapshots(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix("com.apple.TimeMachine.")?;
            let date = rest.split('.').next()?;
            (!date.is_empty()).then(|| date.to_string())
        })
        .collect()
}

/// `diskutil info -plist` içinden ilk bulunan anahtarın tamsayı değeri (XML'i savunmacı biçimde tarar).
pub fn plist_int(xml: &str, key: &str) -> Option<u64> {
    let tag = format!("<key>{key}</key>");
    let rest = xml[xml.find(&tag)? + tag.len()..].trim_start();
    let rest = rest.strip_prefix("<integer>")?;
    rest[..rest.find("</integer>")?].trim().parse().ok()
}

/// Temizlenebilir (purgeable) alan; macOS'un bildirdiği anahtarlar yoksa None. Tavan: 50 GB.
pub fn purgeable_bytes(xml: &str) -> Option<u64> {
    [
        "APFSPurgeableSpace",
        "PurgeableSpace",
        "APFSContainerPurgeable",
    ]
    .iter()
    .find_map(|k| plist_int(xml, k))
    .map(|v| v.min(50_000_000_000))
}

fn shown(d: &Def) -> String {
    let sudo = if d.admin { "sudo " } else { "" };
    format!("{sudo}{} {}", d.bin, d.args.join(" "))
}

fn item(d: &Def) -> Option<HiddenItem> {
    find_cli(d.bin)?;
    let mut description = d.description.to_string();
    let mut estimate = None;
    let mut available = !d.admin || is_root() || sudo_ok();
    if d.id == "tm-snapshots" {
        let snaps = capture("tmutil", &["listlocalsnapshots", "/"])
            .map(|o| parse_snapshots(&o))
            .unwrap_or_default();
        available = !snaps.is_empty();
        description = format!(
            "{description} Found {} snapshot(s){}.",
            snaps.len(),
            snaps
                .iter()
                .min()
                .map(|d| format!(", oldest {d}"))
                .unwrap_or_default()
        );
        estimate = capture("diskutil", &["info", "-plist", "/System/Volumes/Data"])
            .and_then(|x| purgeable_bytes(&x))
            .filter(|_| available);
    } else if !d.dirs.is_empty() {
        let n: u64 = d.dirs.iter().map(|p| scan::measure(Path::new(p)).0).sum();
        estimate = Some(n);
        available &= n > 0;
    }
    Some(HiddenItem {
        id: d.id,
        label: d.label.into(),
        description,
        command: shown(d),
        estimate,
        available,
        needs_admin: d.admin,
        safety: d.safety,
    })
}

fn defs() -> impl Iterator<Item = &'static Def> {
    DEFS.iter().filter(|d| d.os == std::env::consts::OS)
}

/// Bu işletim sistemine uygun eylemler (araç kuruluysa). Windows'ta boş.
pub fn list() -> Vec<HiddenItem> {
    defs().filter_map(item).collect()
}

pub fn run(id: &str) -> Result<HiddenResult> {
    let d = defs()
        .find(|d| d.id == id)
        .ok_or_else(|| Error::NotFound(format!("hidden-space action `{id}`")))?;
    let bin =
        find_cli(d.bin).ok_or_else(|| Error::NotFound(format!("`{}` command not found", d.bin)))?;
    let mut c = base(&bin, d.args);
    if d.admin && !is_root() {
        if !sudo_ok() {
            return Err(Error::Guard(format!(
                "needs administrator rights; run it yourself: {}",
                shown(d)
            )));
        }
        let sudo = find_cli("sudo").ok_or_else(|| Error::NotFound("`sudo` not found".into()))?;
        let mut a = vec!["-n", d.bin];
        a.extend_from_slice(d.args);
        c = base(&sudo, &a);
    }
    let free = || disk_usage().map_or(0, |u| u.free);
    let before = free();
    let (ok, output) = run_cmd(c, Duration::from_secs(120));
    Ok(HiddenResult {
        id: id.into(),
        ok,
        output: tail(&output, 2048),
        before,
        after: free(),
    })
}

/// Sabit argüman vektörleri (testler kullanıcı girdisi içermediğini doğrular).
pub fn fixed_commands() -> Vec<(&'static str, Vec<&'static str>)> {
    DEFS.iter().map(|d| (d.bin, d.args.to_vec())).collect()
}
