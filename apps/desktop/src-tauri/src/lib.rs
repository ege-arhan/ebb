use ebb_core::{
    agents::{self, AiStore},
    catalog::{self, Finding},
    connect::{self, ClientInfo, Outcome},
    journal::{EntryRow, Journal, PlanSummary, Report},
    ops,
    plan::{Plan, Select},
    scan::{self, Node, Progress, ScanOptions},
};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::{
    Emitter, Manager, State,
    ipc::Channel,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

#[derive(Default)]
struct AppState {
    cancel: Arc<AtomicBool>,
}

type R<T> = Result<T, String>;

fn err(e: impl ToString) -> String {
    e.to_string()
}

#[tauri::command]
fn locations() -> Vec<ops::Location> {
    ops::locations()
}

/// Yerel klasör seçici (yol yazmaya gerek kalmaz).
#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog().file().pick_folder(move |p| {
        let _ = tx.send(p.map(|f| f.to_string()));
    });
    tauri::async_runtime::spawn_blocking(move || rx.recv().ok().flatten())
        .await
        .ok()
        .flatten()
}

#[tauri::command]
fn home_dir() -> String {
    dirs::home_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

#[tauri::command]
fn disk_usage() -> Option<ops::DiskUsage> {
    ops::disk_usage()
}

#[tauri::command]
async fn scan(
    path: Option<String>,
    on_progress: Channel<Progress>,
    state: State<'_, AppState>,
) -> R<Node> {
    state.cancel.store(false, Ordering::Relaxed);
    let cancel = state.cancel.clone();
    let root = path
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .ok_or("yol yok")?;
    let opts = ScanOptions {
        max_children: 60,
        ..Default::default()
    };
    tauri::async_runtime::spawn_blocking(move || {
        scan::scan(&root, &opts, &cancel, |p| {
            let _ = on_progress.send(p);
        })
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
fn cancel_scan(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::Relaxed);
}

#[tauri::command]
async fn findings(tr: bool) -> R<Vec<Finding>> {
    tauri::async_runtime::spawn_blocking(move || {
        catalog::load(None).map(|c| catalog::findings(&c, tr))
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
async fn ai_storage() -> R<Vec<AiStore>> {
    tauri::async_runtime::spawn_blocking(agents::ai_storage)
        .await
        .map_err(err)
}

/// Seçili katalog id'leri ya da yollar için plan üretir ve günlüğe kaydeder. Hiçbir şey silmez.
#[tauri::command]
async fn make_plan(findings: Vec<Finding>, ids: Vec<String>, paths: Vec<String>) -> R<Plan> {
    tauri::async_runtime::spawn_blocking(move || {
        let plan = if paths.is_empty() {
            Plan::from_findings(&findings, &Select::Ids(ids))
        } else {
            Plan::from_paths(&paths.into_iter().map(PathBuf::from).collect::<Vec<_>>())
        };
        Journal::open_default()
            .and_then(|j| j.save_plan(&plan))
            .map(|()| plan)
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
async fn apply_plan(plan_id: String) -> R<Report> {
    tauri::async_runtime::spawn_blocking(move || {
        Journal::open_default().and_then(|j| j.apply(&plan_id))
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
async fn undo_plan(plan_id: String) -> R<Report> {
    tauri::async_runtime::spawn_blocking(move || {
        Journal::open_default().and_then(|j| j.undo(&plan_id))
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
fn journal_list() -> R<Vec<PlanSummary>> {
    Journal::open_default()
        .and_then(|j| j.list(50))
        .map_err(err)
}

#[tauri::command]
fn explain(path: String) -> ops::Explain {
    ops::explain(std::path::Path::new(&path))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DupResult {
    groups: Vec<ebb_core::dupes::DupGroup>,
    clone_supported: bool,
}

/// Kopyaları bulur ve Ebb'in daha önce klonladıklarını "optimize" olarak işaretler.
#[tauri::command]
async fn find_duplicates(root: Option<String>, on_progress: Channel<ebb_core::dupes::DupProgress>, state: State<'_, AppState>) -> R<DupResult> {
    state.cancel.store(false, Ordering::Relaxed);
    let cancel = state.cancel.clone();
    let root = root.map(PathBuf::from).or_else(dirs::home_dir).ok_or("no folder")?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut groups = ebb_core::dupes::find_duplicates(&root, 1_000_000, &cancel, |p| {
            let _ = on_progress.send(p);
        })?;
        ebb_core::optimize::mark_optimized(&Journal::open_default()?, &mut groups);
        Ok::<_, ebb_core::Error>(DupResult { groups, clone_supported: ebb_core::optimize::clone_supported(&root) })
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
async fn optimize_clones(groups: Vec<ebb_core::dupes::DupGroup>) -> R<ebb_core::optimize::OptimizeReport> {
    tauri::async_runtime::spawn_blocking(move || Journal::open_default().and_then(|j| ebb_core::optimize::dedupe_with_clones(&j, &groups)))
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
async fn compress(path: String) -> R<ebb_core::optimize::CompressReport> {
    tauri::async_runtime::spawn_blocking(move || ebb_core::optimize::compress(std::path::Path::new(&path))).await.map_err(err)?.map_err(err)
}

#[tauri::command]
async fn decompress(path: String) -> R<ebb_core::optimize::CompressReport> {
    tauri::async_runtime::spawn_blocking(move || ebb_core::optimize::decompress(std::path::Path::new(&path))).await.map_err(err)?.map_err(err)
}

#[tauri::command]
async fn large_old(root: Option<String>) -> R<Vec<ebb_core::finders::FoundFile>> {
    let root = root.map(PathBuf::from).or_else(dirs::home_dir).ok_or("no folder")?;
    tauri::async_runtime::spawn_blocking(move || ebb_core::finders::large_old_files(&root, 200_000_000, 180, 60, &AtomicBool::new(false)))
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
async fn old_installers() -> Vec<ebb_core::finders::FoundFile> {
    tauri::async_runtime::spawn_blocking(|| ebb_core::finders::old_installers(14)).await.unwrap_or_default()
}

#[tauri::command]
async fn health() -> R<ebb_core::system::Health> {
    tauri::async_runtime::spawn_blocking(ebb_core::system::health).await.map_err(err)
}

#[tauri::command]
async fn top_apps() -> R<Vec<ebb_core::system::AppUsage>> {
    tauri::async_runtime::spawn_blocking(|| ebb_core::system::top_apps(15)).await.map_err(err)
}

#[tauri::command]
fn quit_app(pids: Vec<u32>) -> R<usize> {
    ebb_core::system::quit_app(&pids).map_err(err)
}

#[tauri::command]
async fn startup_items() -> R<Vec<ebb_core::system::StartupItem>> {
    tauri::async_runtime::spawn_blocking(ebb_core::system::startup_items).await.map_err(err)
}

#[tauri::command]
fn set_startup(id: String, enabled: bool) -> R<()> {
    ebb_core::system::set_startup_enabled(&id, enabled).map_err(err)
}

/// `ebb` ikilisinin yolu: uygulamanın yanında, yoksa PATH'te.
fn ebb_path() -> Option<PathBuf> {
    let name = if cfg!(windows) { "ebb.exe" } else { "ebb" };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .filter(|p| p.exists())
        .or_else(|| connect::find_cli("ebb"))
}

/// MCP istemcilerine yapıştırılacak komut.
#[tauri::command]
fn mcp_command() -> String {
    ebb_path()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "ebb".into())
}

#[tauri::command]
async fn connect_clients() -> Vec<ClientInfo> {
    tauri::async_runtime::spawn_blocking(connect::clients)
        .await
        .unwrap_or_default()
}

#[tauri::command]
async fn connect_client(id: String) -> R<Outcome> {
    let exe = ebb_path().ok_or("the `ebb` command was not found next to the app or on PATH (build it with `cargo build -p ebb-cli`)")?;
    tauri::async_runtime::spawn_blocking(move || connect::connect(&id, &exe))
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
async fn disconnect_client(id: String) -> R<Outcome> {
    tauri::async_runtime::spawn_blocking(move || connect::disconnect(&id))
        .await
        .map_err(err)?
        .map_err(err)
}

/// `ebb mcp`'yi gerçekten başlatıp araç listesini ister; araç sayısını döner.
#[tauri::command]
async fn connect_test() -> R<usize> {
    let exe = ebb_path().ok_or("the `ebb` command was not found")?;
    tauri::async_runtime::spawn_blocking(move || connect::selftest(&exe))
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
fn journal_items(plan_id: String) -> R<Vec<EntryRow>> {
    Journal::open_default()
        .and_then(|j| j.items(&plan_id))
        .map_err(err)
}

#[tauri::command]
fn reveal(path: String) -> R<()> {
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(err)
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
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

/// Menü çubuğu (macOS) / sistem tepsisi (Windows, Linux): disk durumu, hızlı eylemler, Çık.
fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let handle = app.handle().clone();
    let free = MenuItem::with_id(app, "free", "Reading disk…", false, None::<&str>)?;
    let safe = MenuItem::with_id(app, "safe", "Looking for caches…", false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Ebb", true, None::<&str>)?;
    let clean = MenuItem::with_id(app, "clean", "Review cleanup…", true, None::<&str>)?;
    let scan = MenuItem::with_id(app, "scan", "Scan home folder", true, None::<&str>)?;
    let connect = MenuItem::with_id(app, "connect", "Connect AI assistants…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Ebb", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(
        app,
        &[
            &free,
            &safe,
            &PredefinedMenuItem::separator(app)?,
            &open,
            &clean,
            &scan,
            &connect,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    let template = cfg!(target_os = "macos");
    let icon = tauri::image::Image::from_bytes(if template {
        include_bytes!("../icons/tray-template.png")
    } else {
        include_bytes!("../icons/tray-color.png")
    })?;
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(template)
        .tooltip("Ebb")
        .menu(&menu)
        // macOS: sol tık menüyü açar (menü çubuğu alışkanlığı). Windows: sol tık pencereyi açar, sağ tık menü.
        .show_menu_on_left_click(template)
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            id @ ("clean" | "scan" | "connect") => {
                show_main(app);
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
                show_main(tray.app_handle());
            }
        })
        .build(app)?;

    // Disk durumu her dakika; güvenle temizlenebilir alan (pahalı tarama) 10 dakikada bir.
    std::thread::spawn(move || {
        let mut tick = 0u32;
        loop {
            if let Some(d) = ops::disk_usage() {
                let line = format!("{} free of {}", human(d.free), human(d.total));
                let _ = free.set_text(&line);
                if let Some(tray) = handle.tray_by_id("main") {
                    #[cfg(target_os = "macos")]
                    let _ = tray.set_title(Some(human(d.free)));
                    let _ = tray.set_tooltip(Some(format!("Ebb · {line}")));
                }
            }
            if tick.is_multiple_of(30)
                && let Ok(c) = catalog::load(None)
            {
                let f = catalog::findings(&c, false);
                let s: u64 = f
                    .iter()
                    .filter(|x| x.safety == catalog::Safety::Safe)
                    .map(|x| x.size)
                    .sum();
                let _ = safe.set_text(if s > 0 {
                    format!("↓ {} can ebb safely", human(s))
                } else {
                    "Nothing safe to clean".into()
                });
            }
            tick += 1;
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    });
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            build_tray(app)?;
            Ok(())
        })
        // Pencereyi kapatmak uygulamayı bitirmez, tepsiye indirir; çıkış tepsi menüsünden ya da Cmd/Ctrl+Q ile.
        .on_window_event(|w, ev| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = ev {
                api.prevent_close();
                let _ = w.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            home_dir,
            locations,
            pick_folder,
            disk_usage,
            scan,
            cancel_scan,
            findings,
            ai_storage,
            make_plan,
            apply_plan,
            undo_plan,
            journal_list,
            journal_items,
            explain,
            mcp_command,
            connect_clients,
            connect_client,
            disconnect_client,
            connect_test,
            reveal,
            find_duplicates,
            optimize_clones,
            compress,
            decompress,
            large_old,
            old_installers,
            health,
            top_apps,
            quit_app,
            startup_items,
            set_startup
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // macOS: Dock simgesine tıklanınca gizli pencere geri gelir.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                show_main(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}
