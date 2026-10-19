//! Modelin görebildiği araçlar: yalnızca okuma + plan oluşturma. Uygulama/silme/geri alma YOK.
use ebb_core::{
    agents, catalog,
    journal::Journal,
    ops,
    plan::{Plan, Select},
};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

/// Modele giden araç çıktısı üst sınırı (bayt).
pub const MAX_RESULT: usize = 3000;

pub fn tools_schema() -> Value {
    let f = |name: &str, desc: &str, props: Value, req: &[&str]| {
        json!({"type":"function","function":{"name":name,"description":desc,
            "parameters":{"type":"object","properties":props,"required":req}}})
    };
    let path = json!({"path":{"type":"string","description":"Absolute path"}});
    json!([
        f(
            "disk_usage",
            "Total and free space of the startup disk.",
            json!({}),
            &[]
        ),
        f(
            "scan_path",
            "Size of a folder and its largest direct children.",
            path.clone(),
            &["path"]
        ),
        f(
            "findings",
            "Cleanable caches found on this machine (catalog), largest first.",
            json!({}),
            &[]
        ),
        f(
            "ai_storage",
            "Disk used by AI coding agents and local model stores.",
            json!({}),
            &[]
        ),
        f(
            "large_old_files",
            "Large files not modified for a long time, plus old installers.",
            json!({"min_mb":{"type":"integer"},"days":{"type":"integer"}}),
            &[]
        ),
        f(
            "duplicates",
            "Duplicate files (identical content) under a path, largest reclaimable first.",
            path.clone(),
            &[]
        ),
        f(
            "explain_path",
            "Size, catalog match and protection status of a path.",
            path,
            &["path"]
        ),
        f(
            "plan_cleanup",
            "Create a cleanup plan (NEVER applies it; a human applies it). Use entry_ids from findings or explicit paths.",
            json!({"entry_ids":{"type":"array","items":{"type":"string"}},"paths":{"type":"array","items":{"type":"string"}}}),
            &[]
        ),
    ])
}

fn strings(a: &Value, key: &str) -> Result<Option<Vec<String>>, String> {
    match a.get(key) {
        None => Ok(None),
        Some(Value::Array(v)) => v
            .iter()
            .map(|x| {
                x.as_str()
                    .map(String::from)
                    .ok_or(format!("{key} must contain only strings"))
            })
            .collect::<Result<_, _>>()
            .map(Some),
        Some(_) => Err(format!("{key} must be an array of strings")),
    }
}

fn path_arg(a: &Value) -> Result<PathBuf, String> {
    a.get("path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "path is required".into())
}

fn home_or(a: &Value) -> Result<PathBuf, String> {
    a.get("path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(PathBuf::from)
        })
        .ok_or_else(|| "no home folder".into())
}

pub fn run_tool(name: &str, a: &Value) -> Result<Value, String> {
    let e = |e: ebb_core::Error| e.to_string();
    let ser = |v: Result<Value, serde_json::Error>| v.map_err(|x| x.to_string());
    let n = |k: &str, d: u64| a.get(k).and_then(Value::as_u64).unwrap_or(d);
    match name {
        "disk_usage" => ser(serde_json::to_value(ops::disk_usage())),
        "scan_path" => {
            let node = ops::scan_path(&path_arg(a)?).map_err(e)?;
            let (dirs, _) = ops::top(&node, 10);
            Ok(json!({"path": node.path, "size": node.size, "files": node.files, "children": dirs}))
        }
        "findings" => {
            let f = catalog::findings(&catalog::load(None).map_err(e)?, false);
            // Küçük çıktı: ilk 15, yalnızca gerekli alanlar.
            let v: Vec<Value> = f
                .iter()
                .take(15)
                .map(|x| json!({"entry_id":x.entry_id,"label":x.label,"safety":x.safety,"size":x.size}))
                .collect();
            Ok(json!({"count": f.len(), "top": v}))
        }
        "ai_storage" => ser(serde_json::to_value(agents::ai_storage())),
        "large_old_files" => {
            let root = home_or(&json!({}))?;
            let large = ebb_core::finders::large_old_files(
                &root,
                n("min_mb", 200) * 1_000_000,
                n("days", 180),
                20,
                &AtomicBool::new(false),
            )
            .map_err(e)?;
            let inst: Vec<_> = ebb_core::finders::old_installers(14)
                .into_iter()
                .take(10)
                .collect();
            Ok(json!({"large": large, "installers": inst}))
        }
        "duplicates" => {
            let mut g = ebb_core::dupes::find_duplicates(
                &home_or(a)?,
                1_000_000,
                &AtomicBool::new(false),
                |_| {},
            )
            .map_err(e)?;
            let total: u64 = g.iter().map(|x| x.reclaimable).sum();
            g.truncate(8);
            // Yalnızca yollar ve boyutlar; içerik özeti (hash) modele gerekmez.
            let v: Vec<Value> = g
                .iter()
                .map(|x| json!({"size":x.size,"files":x.files,"reclaimable":x.reclaimable}))
                .collect();
            Ok(json!({"reclaimable": total, "groups": v}))
        }
        "explain_path" => ser(serde_json::to_value(ops::explain(&path_arg(a)?))),
        "plan_cleanup" => {
            let ids = strings(a, "entry_ids")?;
            let paths = strings(a, "paths")?;
            let plan = if let Some(paths) = paths {
                if paths.is_empty() {
                    return Err("paths must not be empty".into());
                }
                // from_paths koruma listesini (guard::check) zaten uygular.
                Plan::from_paths(&paths.into_iter().map(PathBuf::from).collect::<Vec<_>>())
            } else {
                let f = catalog::findings(&catalog::load(None).map_err(e)?, false);
                Plan::from_findings(&f, &ids.map_or(Select::Safe, Select::Ids))
            };
            Journal::open_default()
                .and_then(|j| j.save_plan(&plan))
                .map_err(e)?;
            let items: Vec<Value> = plan
                .items
                .iter()
                .take(10)
                .map(|i| json!({"path":i.path,"size":i.size}))
                .collect();
            Ok(json!({
                "plan_id": plan.id, "item_count": plan.items.len(), "total": plan.total,
                "items": items, "skipped": plan.skipped.len(),
                "next": "Nothing was deleted. The user reviews and applies the plan themselves."
            }))
        }
        _ => Err(format!("unknown tool: {name}")),
    }
}

/// Sonucu modele göndermek için ~3 KB'a kırpar (UTF-8 sınırında).
pub fn truncate(mut s: String, max: usize) -> String {
    if s.len() > max {
        let mut i = max;
        while !s.is_char_boundary(i) {
            i -= 1;
        }
        s.truncate(i);
        s.push_str("...[truncated]");
    }
    s
}
