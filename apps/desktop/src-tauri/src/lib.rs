mod tray;

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
use tauri::{Manager, State, ipc::Channel};

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
async fn find_duplicates(
    root: Option<String>,
    on_progress: Channel<ebb_core::dupes::DupProgress>,
    state: State<'_, AppState>,
) -> R<DupResult> {
    state.cancel.store(false, Ordering::Relaxed);
    let cancel = state.cancel.clone();
    let root = root
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .ok_or("no folder")?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut groups = ebb_core::dupes::find_duplicates(&root, 1_000_000, &cancel, |p| {
            let _ = on_progress.send(p);
        })?;
        ebb_core::optimize::mark_optimized(&Journal::open_default()?, &mut groups);
        Ok::<_, ebb_core::Error>(DupResult {
            groups,
            clone_supported: ebb_core::optimize::clone_supported(&root),
        })
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
async fn optimize_clones(
    groups: Vec<ebb_core::dupes::DupGroup>,
) -> R<ebb_core::optimize::OptimizeReport> {
    tauri::async_runtime::spawn_blocking(move || {
        Journal::open_default().and_then(|j| ebb_core::optimize::dedupe_with_clones(&j, &groups))
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
async fn compress(path: String) -> R<ebb_core::optimize::CompressReport> {
    tauri::async_runtime::spawn_blocking(move || {
        ebb_core::optimize::compress(std::path::Path::new(&path))
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
async fn decompress(path: String) -> R<ebb_core::optimize::CompressReport> {
    tauri::async_runtime::spawn_blocking(move || {
        ebb_core::optimize::decompress(std::path::Path::new(&path))
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
async fn large_old(root: Option<String>) -> R<Vec<ebb_core::finders::FoundFile>> {
    let root = root
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .ok_or("no folder")?;
    tauri::async_runtime::spawn_blocking(move || {
        ebb_core::finders::large_old_files(&root, 200_000_000, 180, 60, &AtomicBool::new(false))
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
async fn old_installers() -> Vec<ebb_core::finders::FoundFile> {
    tauri::async_runtime::spawn_blocking(|| ebb_core::finders::old_installers(14))
        .await
        .unwrap_or_default()
}

#[tauri::command]
async fn health() -> R<ebb_core::system::Health> {
    tauri::async_runtime::spawn_blocking(ebb_core::system::health)
        .await
        .map_err(err)
}

#[tauri::command]
async fn top_apps() -> R<Vec<ebb_core::system::AppUsage>> {
    tauri::async_runtime::spawn_blocking(|| ebb_core::system::top_apps(15))
        .await
        .map_err(err)
}

#[tauri::command]
fn quit_app(pids: Vec<u32>) -> R<usize> {
    ebb_core::system::quit_app(&pids).map_err(err)
}

#[tauri::command]
async fn startup_items() -> R<Vec<ebb_core::system::StartupItem>> {
    tauri::async_runtime::spawn_blocking(ebb_core::system::startup_items)
        .await
        .map_err(err)
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
async fn trash_list() -> R<Vec<ebb_core::trashbin::TrashEntry>> {
    tauri::async_runtime::spawn_blocking(|| {
        Journal::open_default().and_then(|j| ebb_core::trashbin::list(&j))
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
async fn trash_restore(ids: Vec<String>) -> R<usize> {
    tauri::async_runtime::spawn_blocking(move || {
        Journal::open_default().and_then(|j| ebb_core::trashbin::restore(&j, &ids))
    })
    .await
    .map_err(err)?
    .map_err(err)
}

/// Kalıcı silme: yalnızca kullanıcı arayüzde onayladıktan sonra çağrılır.
#[tauri::command]
async fn trash_purge(ids: Vec<String>) -> R<u64> {
    tauri::async_runtime::spawn_blocking(move || {
        Journal::open_default().and_then(|j| ebb_core::trashbin::purge(&j, &ids))
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
async fn trash_empty() -> R<u64> {
    tauri::async_runtime::spawn_blocking(|| {
        Journal::open_default().and_then(|j| ebb_core::trashbin::empty(&j))
    })
    .await
    .map_err(err)?
    .map_err(err)
}

/// macOS: Gizlilik → Tam Disk Erişimi ayarını açar (Çöp Kutusu'nu okumak için gerekebilir).
#[tauri::command]
fn open_privacy_settings() -> R<()> {
    #[cfg(target_os = "macos")]
    return tauri_plugin_opener::open_url(
        "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles",
        None::<&str>,
    )
    .map_err(err);
    #[cfg(not(target_os = "macos"))]
    Ok(())
}

#[tauri::command]
fn plan_get(id: String) -> R<Plan> {
    Journal::open_default()
        .and_then(|j| j.plan(&id))
        .map(|(p, _)| p)
        .map_err(err)
}

#[tauri::command]
async fn apps_list() -> Vec<ebb_core::apps::InstalledApp> {
    tauri::async_runtime::spawn_blocking(ebb_core::apps::installed_apps)
        .await
        .unwrap_or_default()
}

#[tauri::command]
async fn app_preview(id: String) -> R<ebb_core::apps::UninstallPreview> {
    tauri::async_runtime::spawn_blocking(move || ebb_core::apps::preview(&id))
        .await
        .map_err(err)?
        .map_err(err)
}

/// Uygulama + artıkları için plan üretir ve kaydeder; uygulama normal plan akışıyla (onay) yapılır.
#[tauri::command]
async fn app_uninstall_plan(id: String, include_medium: bool) -> R<Plan> {
    tauri::async_runtime::spawn_blocking(move || {
        let pv = ebb_core::apps::preview(&id)?;
        let plan = Plan::from_paths(&ebb_core::apps::uninstall_paths(&pv, include_medium));
        Journal::open_default()
            .and_then(|j| j.save_plan(&plan))
            .map(|()| plan)
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
async fn project_artifacts(
    idle_days: u64,
    state: State<'_, AppState>,
) -> R<Vec<ebb_core::projects::Artifact>> {
    state.cancel.store(false, Ordering::Relaxed);
    let cancel = state.cancel.clone();
    tauri::async_runtime::spawn_blocking(move || {
        ebb_core::projects::find_artifacts(&ebb_core::projects::default_roots(), idle_days, &cancel)
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
async fn prune_list() -> Vec<ebb_core::prune::PruneAction> {
    tauri::async_runtime::spawn_blocking(ebb_core::prune::available)
        .await
        .unwrap_or_default()
}

#[tauri::command]
async fn prune_run(id: String) -> R<ebb_core::prune::PruneResult> {
    tauri::async_runtime::spawn_blocking(move || ebb_core::prune::run(&id))
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
fn care_rules() -> R<Vec<ebb_core::autocare::Rule>> {
    let j = Journal::open_default().map_err(err)?;
    let mut rules = j.rules().map_err(err)?;
    if rules.is_empty() {
        for r in ebb_core::autocare::default_rules() {
            j.save_rule(&r).map_err(err)?;
        }
        rules = j.rules().map_err(err)?;
    }
    Ok(rules)
}

#[tauri::command]
fn care_save(rule: ebb_core::autocare::Rule) -> R<()> {
    Journal::open_default()
        .and_then(|j| j.save_rule(&rule))
        .map_err(err)
}

#[tauri::command]
async fn care_run(id: String) -> R<ebb_core::autocare::RuleOutcome> {
    tauri::async_runtime::spawn_blocking(move || {
        let j = Journal::open_default()?;
        let rule = j
            .rules()?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| ebb_core::Error::NotFound(format!("rule {id}")))?;
        ebb_core::autocare::run_rule(&j, &rule)
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
async fn ai_providers() -> Vec<ebb_ai::Provider> {
    tauri::async_runtime::spawn_blocking(ebb_ai::detect)
        .await
        .unwrap_or_default()
}

#[tauri::command]
async fn ai_chat(
    provider: ebb_ai::Provider,
    model: String,
    history: Vec<ebb_ai::ChatMessage>,
    message: String,
) -> R<ebb_ai::Reply> {
    tauri::async_runtime::spawn_blocking(move || {
        ebb_ai::chat(&provider, &model, &history, &message)
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
fn tray_get() -> tray::TrayConfig {
    tray::load()
}

#[tauri::command]
async fn tray_set(app: tauri::AppHandle, config: tray::TrayConfig) -> R<()> {
    tray::save(&config).map_err(err)?;
    tauri::async_runtime::spawn_blocking(move || tray::update(&app))
        .await
        .map_err(err)
}

#[tauri::command]
fn disk_history(days: u32) -> R<Vec<ebb_core::history::DiskSample>> {
    Journal::open_default()
        .and_then(|j| j.disk_history(days))
        .map_err(err)
}

/// Yalnızca Ebb'in kendi sürüm sayfasını tarayıcıda açar.
#[tauri::command]
fn open_release(url: String) -> R<()> {
    if !url.starts_with("https://github.com/ege-arhan/ebb/releases") {
        return Err("only Ebb release pages can be opened".into());
    }
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(err)
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

pub(crate) fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState::default())
        .setup(|app| {
            tray::build(app)?;
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
            disk_history,
            tray_get,
            tray_set,
            plan_get,
            apps_list,
            app_preview,
            app_uninstall_plan,
            project_artifacts,
            prune_list,
            prune_run,
            care_rules,
            care_save,
            care_run,
            ai_providers,
            ai_chat,
            trash_list,
            trash_restore,
            trash_purge,
            trash_empty,
            open_privacy_settings,
            open_release,
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
