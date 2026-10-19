//! Minimal MCP sunucusu (JSON-RPC 2.0, stdio, satır başına bir mesaj).
//! Yalnızca okuma ve plan araçları sunar; `apply` bilinçli olarak yoktur:
//! yıkıcı uygulama GUI onayı ya da `ebb apply --yes` gerektirir.
use ebb_core::{
    agents, catalog,
    journal::Journal,
    ops,
    plan::{Plan, Select},
};
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::PathBuf;

const PROTOCOL: &str = "2025-06-18";

fn tools() -> Value {
    let path = json!({"type":"object","properties":{"path":{"type":"string","description":"Absolute path"}},"required":["path"]});
    json!([
        {"name":"ebb_scan","description":"Scan a folder; returns total size and its largest direct children.","inputSchema":{"type":"object","properties":{"path":{"type":"string"},"limit":{"type":"integer","default":20}},"required":["path"]}},
        {"name":"ebb_top","description":"Largest folders (direct children) and largest files under a path.","inputSchema":{"type":"object","properties":{"path":{"type":"string"},"n":{"type":"integer","default":15}},"required":["path"]}},
        {"name":"ebb_why","description":"What grew since the previous scan of this path (first call stores the baseline).","inputSchema":{"type":"object","properties":{"path":{"type":"string"},"top":{"type":"integer","default":10}},"required":["path"]}},
        {"name":"ebb_explain_path","description":"Size, catalog match and protection status of a path.","inputSchema":path},
        {"name":"ebb_findings","description":"Cleanable caches found on this machine (catalog), largest first.","inputSchema":{"type":"object","properties":{}}},
        {"name":"ebb_agents_storage","description":"Disk used by AI coding agents and local model stores (Claude Code, Codex, Cursor, Ollama, LM Studio, Hugging Face).","inputSchema":{"type":"object","properties":{}}},
        {"name":"ebb_plan_cleanup","description":"Create (never apply) a cleanup plan. Use entry_ids from ebb_findings (safe+review) or explicit paths. Defaults to all safe findings. A human applies it with `ebb apply <plan_id> --yes` or in the app.","inputSchema":{"type":"object","properties":{"entry_ids":{"type":"array","items":{"type":"string"}},"paths":{"type":"array","items":{"type":"string"}}}}},
        {"name":"ebb_duplicates","description":"Duplicate files (identical content) under a path, largest reclaimable first. Read-only; optimizing them needs the user.","inputSchema":{"type":"object","properties":{"path":{"type":"string"},"min_mb":{"type":"integer","default":1}}}},
        {"name":"ebb_large_old_files","description":"Large files not opened for months, plus old installers in Downloads/Desktop. Read-only.","inputSchema":{"type":"object","properties":{"path":{"type":"string"},"min_mb":{"type":"integer","default":200},"days":{"type":"integer","default":180}}}},
        {"name":"ebb_system_health","description":"Health score, memory pressure, heavy apps and startup items with plain-English tips. Read-only.","inputSchema":{"type":"object","properties":{}}},
        {"name":"ebb_plan_status","description":"Status of a plan, or the recent plans when plan_id is omitted.","inputSchema":{"type":"object","properties":{"plan_id":{"type":"string"}}}},
    ])
}

/// Anahtar yoksa `None`; varsa dizi-of-string olmalı, aksi halde hata (sessizce yok sayılmaz).
fn str_list(a: &Value, key: &str) -> Result<Option<Vec<String>>, String> {
    match a.get(key) {
        None => Ok(None),
        Some(Value::Array(v)) => v
            .iter()
            .map(|x| {
                x.as_str()
                    .map(String::from)
                    .ok_or_else(|| format!("{key} must contain only strings"))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        Some(_) => Err(format!("{key} must be an array of strings")),
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn arg_path(a: &Value) -> Result<PathBuf, String> {
    a.get("path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "path is required".into())
}

fn call(name: &str, a: &Value) -> Result<Value, String> {
    let e = |e: ebb_core::Error| e.to_string();
    let n = |k: &str, d: u64| a.get(k).and_then(Value::as_u64).unwrap_or(d) as usize;
    match name {
        "ebb_scan" => {
            let node = ops::scan_path(&arg_path(a)?).map_err(e)?;
            let (dirs, _) = ops::top(&node, n("limit", 20));
            Ok(
                json!({"path": node.path, "size": node.size, "logical": node.logical, "files": node.files, "children": dirs}),
            )
        }
        "ebb_top" => {
            let node = ops::scan_path(&arg_path(a)?).map_err(e)?;
            let (dirs, files) = ops::top(&node, n("n", 15));
            Ok(json!({"folders": dirs, "files": files}))
        }
        "ebb_why" => {
            let j = Journal::open_default().map_err(e)?;
            serde_json::to_value(ops::why(&j, &arg_path(a)?, n("top", 10)).map_err(e)?)
                .map_err(|x| x.to_string())
        }
        "ebb_explain_path" => {
            serde_json::to_value(ops::explain(&arg_path(a)?)).map_err(|x| x.to_string())
        }
        "ebb_findings" => {
            let c = catalog::load(None).map_err(e)?;
            serde_json::to_value(catalog::findings(&c, false)).map_err(|x| x.to_string())
        }
        "ebb_agents_storage" => {
            serde_json::to_value(agents::ai_storage()).map_err(|x| x.to_string())
        }
        "ebb_plan_cleanup" => {
            let ids = str_list(a, "entry_ids")?;
            let paths = str_list(a, "paths")?;
            let plan = if let Some(paths) = paths {
                if paths.is_empty() {
                    return Err("paths must not be empty".into());
                }
                Plan::from_paths(&paths.into_iter().map(PathBuf::from).collect::<Vec<_>>())
            } else {
                let f = catalog::findings(&catalog::load(None).map_err(e)?, false);
                // Seçim hiç verilmediyse varsayılan güvenli küme; verildiyse (boş dahil) yalnızca o.
                Plan::from_findings(&f, &ids.map_or(Select::Safe, Select::Ids))
            };
            Journal::open_default()
                .and_then(|j| j.save_plan(&plan))
                .map_err(e)?;
            let mut v = serde_json::to_value(&plan).map_err(|x| x.to_string())?;
            v["next"] = json!(format!(
                "Nothing was deleted. A human applies it: ebb apply {} --yes",
                plan.id
            ));
            Ok(v)
        }
        "ebb_plan_status" => {
            let j = Journal::open_default().map_err(e)?;
            match a.get("plan_id").and_then(Value::as_str) {
                Some(id) => {
                    let (plan, status) = j.plan(id).map_err(e)?;
                    Ok(json!({"status": status, "plan": plan}))
                }
                None => serde_json::to_value(j.list(20).map_err(e)?).map_err(|x| x.to_string()),
            }
        }
        "ebb_duplicates" => {
            let root = a
                .get("path")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .or_else(home)
                .ok_or("no home folder")?;
            let mut g = ebb_core::dupes::find_duplicates(
                &root,
                n("min_mb", 1) as u64 * 1_000_000,
                &std::sync::atomic::AtomicBool::new(false),
                |_| {},
            )
            .map_err(e)?;
            if let Ok(j) = Journal::open_default() {
                ebb_core::optimize::mark_optimized(&j, &mut g);
            }
            let total: u64 = g
                .iter()
                .filter(|x| !x.optimized)
                .map(|x| x.reclaimable)
                .sum();
            g.truncate(30);
            Ok(
                json!({"reclaimable": total, "groups": g, "next": "A human can turn copies into space-sharing clones with `ebb optimize` or the app's Optimize screen."}),
            )
        }
        "ebb_large_old_files" => {
            let root = a
                .get("path")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .or_else(home)
                .ok_or("no home folder")?;
            let large = ebb_core::finders::large_old_files(
                &root,
                n("min_mb", 200) as u64 * 1_000_000,
                n("days", 180) as u64,
                40,
                &std::sync::atomic::AtomicBool::new(false),
            )
            .map_err(e)?;
            Ok(json!({"large": large, "installers": ebb_core::finders::old_installers(14)}))
        }
        "ebb_system_health" => {
            Ok(json!({"health": ebb_core::system::health(), "apps": ebb_core::system::top_apps(8)}))
        }
        _ => Err(format!("unknown tool: {name}")),
    }
}

fn handle(req: &Value) -> Option<Value> {
    let invalid = |id: Value, why: &str| {
        Some(json!({"jsonrpc":"2.0","id":id,"error":{"code":-32600,"message":why}}))
    };
    if !req.is_object() {
        return invalid(Value::Null, "request must be an object");
    }
    // Bildirim (id yok): yanıt verilmez. id yalnızca string, sayı ya da null olabilir.
    let id = match req.get("id") {
        None => return None,
        Some(v) if v.is_string() || v.is_number() || v.is_null() => v.clone(),
        Some(_) => return invalid(Value::Null, "id must be a string, number or null"),
    };
    if req.get("jsonrpc") != Some(&json!("2.0")) {
        return invalid(id, "jsonrpc must be \"2.0\"");
    }
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let ok = |r: Value| json!({"jsonrpc":"2.0","id":id,"result":r});
    Some(match method {
        "initialize" => ok(json!({
            "protocolVersion": PROTOCOL,
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "ebb", "version": env!("CARGO_PKG_VERSION")},
            "instructions": "Ebb inspects disk usage and plans cleanups. It never deletes on its own: ebb_plan_cleanup only creates a plan that a human applies."
        })),
        "ping" => ok(json!({})),
        "tools/list" => ok(json!({"tools": tools()})),
        "tools/call" => {
            let p = req.get("params").cloned().unwrap_or_default();
            let name = p.get("name").and_then(Value::as_str).unwrap_or("");
            let args = p.get("arguments").cloned().unwrap_or_else(|| json!({}));
            if !args.is_object() {
                return Some(ok(
                    json!({"content":[{"type":"text","text":"arguments must be an object"}],"isError":true}),
                ));
            }
            match call(name, &args) {
                Ok(v) => ok(
                    json!({"content":[{"type":"text","text": serde_json::to_string_pretty(&v).unwrap_or_default()}],"isError":false}),
                ),
                Err(m) => ok(json!({"content":[{"type":"text","text": m}],"isError":true})),
            }
        }
        _ => {
            json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":format!("method not found: {method}")}})
        }
    })
}

/// stdin'den satır satır okur, stdout'a yazar. stdout yalnızca protokole ayrılmıştır.
pub fn serve_stdio() -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Value>(&line) {
            Ok(req) => handle(&req),
            Err(e) => Some(
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":e.to_string()}}),
            ),
        };
        if let Some(r) = resp {
            writeln!(out, "{r}")?;
            out.flush()?;
        }
    }
    Ok(())
}
