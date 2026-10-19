//! Menü çubuğu (macOS) / sistem tepsisi (Windows, Linux): ne gösterileceği kullanıcıya bağlı.
use ebb_core::{catalog, journal::Journal, ops, system, trashbin};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tauri::{
    AppHandle, Emitter,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

/// Seçilebilir değerler: "free", "safe", "memory", "trash", "health". Başlık ayrıca "none" olabilir.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TrayConfig {
    pub title: String,
    pub lines: Vec<String>,
}

impl Default for TrayConfig {
    fn default() -> Self {
        Self {
            title: "free".into(),
            lines: vec!["free".into(), "safe".into(), "trash".into()],
        }
    }
}

fn path() -> PathBuf {
    ebb_core::data_dir().join("tray.json")
}

pub fn load() -> TrayConfig {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(c: &TrayConfig) -> std::io::Result<()> {
    std::fs::create_dir_all(ebb_core::data_dir())?;
    std::fs::write(path(), serde_json::to_string_pretty(c).unwrap_or_default())
}

/// Pahalı değerler (katalog taraması, Çöp boyutu) arka planda hesaplanıp burada saklanır.
#[derive(Default, Clone)]
struct Cache {
    safe: Option<u64>,
    trash: Option<(u64, usize)>,
}

fn cache() -> &'static Mutex<Cache> {
    static C: OnceLock<Mutex<Cache>> = OnceLock::new();
    C.get_or_init(Default::default)
}

fn human(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let (mut v, mut i) = (b as f64, 0);
    while v >= 1000.0 && i < 4 {
        v /= 1000.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else if v >= 100.0 {
        format!("{v:.0} {}", U[i])
    } else {
        format!("{v:.1} {}", U[i])
    }
}

/// (kısa başlık metni, menü satırı) — değer henüz yoksa None.
fn value(id: &str, cfg: &TrayConfig) -> Option<(String, String)> {
    let c = cache().lock().ok()?.clone();
    match id {
        "free" => ops::disk_usage().map(|d| {
            (
                human(d.free),
                format!("{} free of {}", human(d.free), human(d.total)),
            )
        }),
        "safe" => c.safe.map(|s| {
            if s > 0 {
                (
                    format!("↓{}", human(s)),
                    format!("↓ {} can ebb safely", human(s)),
                )
            } else {
                ("0 B".into(), "Nothing safe to clean".into())
            }
        }),
        "memory" => {
            let m = system::memory();
            Some((
                format!("RAM {:.0}%", m.pressure * 100.0),
                format!(
                    "Memory {} of {} in use ({:.0}% pressure)",
                    human(m.used),
                    human(m.total),
                    m.pressure * 100.0
                ),
            ))
        }
        "trash" => c.trash.map(|(b, n)| {
            (
                format!("Trash {}", human(b)),
                format!("Trash: {} ({n} items)", human(b)),
            )
        }),
        "health" => {
            let _ = cfg;
            let h = system::health();
            Some((
                format!("{}/100", h.score),
                format!("Health {}/100", h.score),
            ))
        }
        _ => None,
    }
}

/// Menüyü ve başlığı güncel ayara göre yeniden kurar.
pub fn update(app: &AppHandle) {
    let cfg = load();
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    let mut items: Vec<MenuItem<tauri::Wry>> = vec![];
    let mut tip = String::from("Ebb");
    for id in &cfg.lines {
        let text = value(id, &cfg).map(|v| v.1).unwrap_or_else(|| "…".into());
        if let Ok(i) = MenuItem::with_id(app, format!("info-{id}"), &text, false, None::<&str>) {
            items.push(i);
        }
    }
    let mk = |id: &str, label: &str| MenuItem::with_id(app, id, label, true, None::<&str>).ok();
    let actions: Vec<MenuItem<tauri::Wry>> = [
        mk("open", "Open Ebb"),
        mk("clean", "Review cleanup…"),
        mk("trash", "Open Trash…"),
        mk("speed", "Speed up…"),
        mk("scan", "Scan a folder"),
        mk("connect", "Connect AI assistants…"),
    ]
    .into_iter()
    .flatten()
    .collect();
    let quit = MenuItem::with_id(app, "quit", "Quit Ebb", true, Some("CmdOrCtrl+Q")).ok();
    let sep1 = PredefinedMenuItem::separator(app).ok();
    let sep2 = PredefinedMenuItem::separator(app).ok();
    let mut refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = vec![];
    items.iter().for_each(|i| refs.push(i));
    if !items.is_empty()
        && let Some(s) = &sep1
    {
        refs.push(s);
    }
    actions.iter().for_each(|i| refs.push(i));
    if let Some(s) = &sep2 {
        refs.push(s);
    }
    if let Some(q) = &quit {
        refs.push(q);
    }
    if let Ok(menu) = Menu::with_items(app, &refs) {
        let _ = tray.set_menu(Some(menu));
    }
    let title = if cfg.title == "none" {
        None
    } else {
        value(&cfg.title, &cfg).map(|v| v.0)
    };
    if let Some(t) = &title {
        tip = format!("Ebb · {t}");
    }
    #[cfg(target_os = "macos")]
    let _ = tray.set_title(title);
    let _ = tray.set_tooltip(Some(tip));
}

fn refresh_slow() {
    let safe = catalog::load(None).ok().map(|c| {
        catalog::findings(&c, false)
            .iter()
            .filter(|x| x.safety == catalog::Safety::Safe)
            .map(|x| x.size)
            .sum::<u64>()
    });
    let trash = Journal::open_default()
        .ok()
        .and_then(|j| trashbin::summary(&j).ok())
        .map(|s| (s.size, s.items));
    if let Ok(mut c) = cache().lock() {
        c.safe = safe.or(c.safe);
        c.trash = trash.or(c.trash);
    }
}

pub fn build(app: &tauri::App) -> tauri::Result<()> {
    let handle = app.handle().clone();
    let template = cfg!(target_os = "macos");
    let icon = tauri::image::Image::from_bytes(if template {
        include_bytes!("../icons/tray-template.png")
    } else {
        include_bytes!("../icons/tray-color.png")
    })?;
    let menu = Menu::with_items(
        app,
        &[&MenuItem::with_id(
            app,
            "quit",
            "Quit Ebb",
            true,
            Some("CmdOrCtrl+Q"),
        )?],
    )?;
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(template)
        .tooltip("Ebb")
        .menu(&menu)
        // macOS: sol tık menüyü açar (menü çubuğu alışkanlığı). Windows: sol tık pencereyi açar, sağ tık menü.
        .show_menu_on_left_click(template)
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "open" => super::show_main(app),
            "quit" => app.exit(0),
            id @ ("clean" | "scan" | "connect" | "trash" | "speed") => {
                super::show_main(app);
                let _ = app.emit("ebb://nav", id);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = ev
                && !cfg!(target_os = "macos")
            {
                super::show_main(tray.app_handle());
            }
        })
        .build(app)?;

    std::thread::spawn(move || {
        let mut tick = 0u32;
        loop {
            if tick.is_multiple_of(30) {
                update(&handle); // ilk anda eldeki değerlerle çiz, pahalı hesap sürerken boş kalmasın
                refresh_slow();
            }
            if let Some(d) = ops::disk_usage() {
                // Boş alan geçmişi (saatte bir örnek; fazlası yok sayılır).
                if tick.is_multiple_of(10)
                    && let Ok(j) = Journal::open_default()
                {
                    let _ = j.record_disk(d.total, d.free);
                }
                // Auto Care: zamanı gelen kurallar saatte bir kontrol edilir.
                if tick.is_multiple_of(60)
                    && let Ok(j) = Journal::open_default()
                {
                    let free_pct = d.free as f32 / d.total.max(1) as f32 * 100.0;
                    for o in ebb_core::autocare::run_due(&j, ebb_core::plan::now(), free_pct) {
                        let _ = handle.emit("ebb://care", &o);
                        use tauri_plugin_notification::NotificationExt;
                        let _ = handle
                            .notification()
                            .builder()
                            .title("Ebb · Auto Care")
                            .body(&o.message)
                            .show();
                    }
                }
            }
            update(&handle);
            tick += 1;
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    });
    Ok(())
}
