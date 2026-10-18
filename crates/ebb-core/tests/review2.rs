//! Independent review. Run `cargo build -p ebb-cli` before this integration suite.
//! Every case runs in a child with isolated HOME/data/temp and an empty PATH.
use ebb_core::{
    catalog, connect,
    journal::Journal,
    ops,
    plan::{Plan, Select},
    scan,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

fn run_case(case: &str) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    for name in ["home", "data", "tmp", "config", "cache", "local"] {
        fs::create_dir(root.join(name)).unwrap();
    }
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "fixture", "--nocapture"])
        .env("EBB_REVIEW2_CASE", case)
        .env("HOME", root.join("home"))
        .env("USERPROFILE", root.join("home"))
        .env("EBB_DATA_DIR", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_DATA_HOME", root.join("local"))
        .env("APPDATA", root.join("config"))
        .env("LOCALAPPDATA", root.join("local"))
        .env("TMPDIR", root.join("tmp"))
        .env("TMP", root.join("tmp"))
        .env("TEMP", root.join("tmp"))
        .env("PATH", "")
        .env("NO_COLOR", "1")
        .current_dir(&root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(25) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("{case}: isolated fixture exceeded 25s");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{case}\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn ebb() -> PathBuf {
    let path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(if cfg!(windows) { "ebb.exe" } else { "ebb" });
    assert!(
        path.is_file(),
        "first run: cargo build -p ebb-cli ({})",
        path.display()
    );
    path
}

fn cli(args: &[&str]) -> String {
    let out = Command::new(ebb())
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn rpc_lines(input: &str) -> Vec<Value> {
    let mut child = Command::new(ebb())
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}

fn rpc(name: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":name,"arguments":arguments}});
    rpc_lines(&(request.to_string() + "\n")).remove(0)
}

fn config(home: &Path, text: &str) -> PathBuf {
    let p = home.join(".cursor/mcp.json");
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, text).unwrap();
    p
}

fn scan_tree(path: &Path, cap: usize) -> scan::Node {
    scan::scan(
        path,
        &scan::ScanOptions {
            max_children: cap,
            ..Default::default()
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap()
}

fn check_totals(n: &scan::Node) {
    if !n.is_dir {
        return;
    }
    // Directory metadata also occupies space; children alone must not include it.
    let meta = fs::symlink_metadata(&n.path).unwrap();
    #[cfg(unix)]
    let own_size = {
        use std::os::unix::fs::MetadataExt;
        meta.blocks() * 512
    };
    #[cfg(not(unix))]
    let own_size = meta.len();
    assert_eq!(n.files, n.children.iter().map(|c| c.files).sum::<u64>());
    assert_eq!(
        n.size,
        own_size + n.children.iter().map(|c| c.size).sum::<u64>()
    );
    assert_eq!(
        n.logical,
        meta.len() + n.children.iter().map(|c| c.logical).sum::<u64>()
    );
    for c in &n.children {
        check_totals(c);
    }
}

#[test]
fn fixture() {
    let Ok(case) = std::env::var("EBB_REVIEW2_CASE") else {
        return;
    };
    let root = std::env::current_dir().unwrap();
    let home = root.join("home");
    assert_eq!(dirs::home_dir().unwrap().canonicalize().unwrap(), home);
    assert_eq!(
        PathBuf::from(std::env::var_os("USERPROFILE").unwrap()),
        home
    );
    assert_eq!(ebb_core::data_dir(), root.join("data"));
    assert_eq!(std::env::var("PATH").unwrap(), "");
    let command = root.join("bin with spaces/ebb ' $();&`");
    match case.as_str() {
        "json" => {
            let before =
                r#"{"z":1,"mcpServers":{"z":{"command":"old"},"a":{"args":["a b"]}},"a":2}"#;
            let cfg = config(&home, before);
            let out = connect::connect("cursor", &command).unwrap();
            assert_eq!(fs::read_to_string(out.backup.unwrap()).unwrap(), before);
            let v: Value = serde_json::from_str(&fs::read_to_string(&cfg).unwrap()).unwrap();
            assert_eq!(
                v.as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                ["z", "mcpServers", "a"]
            );
            assert_eq!(
                v["mcpServers"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                ["z", "a", "ebb"]
            );
            assert_eq!(v["mcpServers"]["ebb"]["command"], command.to_str().unwrap());
            assert_eq!(v["mcpServers"]["ebb"]["args"], json!(["mcp"]));
            connect::disconnect("cursor").unwrap();
            let after: Value = serde_json::from_str(&fs::read_to_string(cfg).unwrap()).unwrap();
            assert_eq!(after, serde_json::from_str::<Value>(before).unwrap());
        }
        "invalid_json" => {
            for before in ["{ // comment\n}", "[]", "null", r#"{"mcpServers":[]}"#] {
                let cfg = config(&home, before);
                assert!(connect::connect("cursor", &command).is_err());
                assert_eq!(fs::read_to_string(cfg).unwrap(), before);
            }
        }
        "backup_error" => {
            let before = r#"{"keep":1}"#;
            let cfg = config(&home, before);
            fs::create_dir(home.join(".cursor/mcp.json.ebb-backup")).unwrap();
            assert!(connect::connect("cursor", &command).is_err());
            assert_eq!(fs::read_to_string(cfg).unwrap(), before);
        }
        "backup_retained" => {
            let before = r#"{"keep":1}"#;
            config(&home, before);
            let backup = connect::connect("cursor", &command)
                .unwrap()
                .backup
                .unwrap();
            connect::connect("cursor", &command).unwrap();
            assert_eq!(
                fs::read_to_string(backup).unwrap(),
                before,
                "reconnecting overwrote the only pre-Ebb backup"
            );
        }
        "foreign_connect" | "foreign_disconnect" => {
            let before = r#"{"mcpServers":{"ebb":{"command":"/other/ebb","args":["custom"],"env":{"TOKEN":"keep"}}}}"#;
            let cfg = config(&home, before);
            if case == "foreign_connect" {
                let _ = connect::connect("cursor", &command);
            } else {
                let _ = connect::disconnect("cursor");
            }
            assert_eq!(
                fs::read_to_string(cfg).unwrap(),
                before,
                "an unowned ebb entry was replaced/removed without confirmation"
            );
        }
        #[cfg(unix)]
        "config_symlink" | "backup_symlink" => {
            use std::os::unix::fs::symlink;
            let cfg = config(&home, r#"{"keep":1}"#);
            let target = root.join("unrelated.json");
            let before = r#"{"unrelated":"do not modify"}"#;
            fs::write(&target, before).unwrap();
            if case == "config_symlink" {
                fs::remove_file(&cfg).unwrap();
                symlink(&target, &cfg).unwrap();
            } else {
                symlink(&target, home.join(".cursor/mcp.json.ebb-backup")).unwrap();
            }
            let result = connect::connect("cursor", &command);
            assert_eq!(
                fs::read_to_string(&target).unwrap(),
                before,
                "followed a symlink for writing"
            );
            assert!(result.is_err());
        }
        "cli_plans" => {
            let target = root.join("cache-item");
            fs::write(&target, "keep").unwrap();
            for json_flag in [false, true] {
                let mut args = vec!["clean", "--paths", target.to_str().unwrap()];
                if json_flag {
                    args.push("--json");
                }
                cli(&args);
                assert_eq!(fs::read_to_string(&target).unwrap(), "keep");
            }
            let journal = Journal::open_default().unwrap();
            let plan = journal.list(10).unwrap().remove(0);
            for flags in [vec![], vec!["--json"]] {
                let mut args = vec!["apply", plan.id.as_str()];
                args.extend(flags);
                cli(&args);
                assert_eq!(journal.plan(&plan.id).unwrap().1, "planned");
                assert!(journal.items(&plan.id).unwrap().is_empty());
                assert_eq!(fs::read_to_string(&target).unwrap(), "keep");
            }
            let protected = home.join(".ssh/key");
            fs::create_dir_all(protected.parent().unwrap()).unwrap();
            fs::write(&protected, "secret").unwrap();
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(home.join(".ssh"), root.join("alias")).unwrap();
                let alias = root.join("alias/key");
                let result: Value = serde_json::from_str(&cli(&[
                    "clean",
                    "--paths",
                    alias.to_str().unwrap(),
                    "--json",
                ]))
                .unwrap();
                assert!(result["items"].as_array().unwrap().is_empty());
                assert_eq!(result["skipped"].as_array().unwrap().len(), 1);
            }
            let result: Value = serde_json::from_str(&cli(&[
                "clean",
                "--paths",
                home.to_str().unwrap(),
                protected.to_str().unwrap(),
                "--json",
            ]))
            .unwrap();
            assert!(result["items"].as_array().unwrap().is_empty());
            assert_eq!(result["skipped"].as_array().unwrap().len(), 2);
            let finding = catalog::Finding {
                entry_id: "never-fixture".into(),
                label: "never".into(),
                category: "review".into(),
                safety: catalog::Safety::Never,
                path: target,
                size: 4,
                files: 1,
                note: None,
            };
            assert!(
                Plan::from_findings(&[finding], &Select::Ids(vec!["never-fixture".into()]))
                    .items
                    .is_empty()
            );
            let result: Value =
                serde_json::from_str(&cli(&["clean", "--ids", "docker-desktop-data", "--json"]))
                    .unwrap();
            assert!(result["items"].as_array().unwrap().is_empty());
        }
        "rpc_malformed" => {
            let mut input = String::from("{\nnull\n[]\n7\n{\"id\":3,\"params\":[]}\n");
            input.push_str(&"[".repeat(140));
            input.push('0');
            input.push_str(&"]".repeat(140));
            input.push('\n');
            input.push_str("{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"ping\"}\n");
            let replies = rpc_lines(&input);
            assert!(replies.iter().any(|r| r["error"]["code"] == -32700));
            assert_eq!(replies.last().unwrap()["id"], 99);
            assert_eq!(replies.last().unwrap()["result"], json!({}));
        }
        "rpc_plan" => {
            let target = root.join("cache-item");
            fs::write(&target, "keep").unwrap();
            let reply = rpc("ebb_plan_cleanup", json!({"paths":[target]}));
            assert_eq!(reply["result"]["isError"], false);
            let plan: Value =
                serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap())
                    .unwrap();
            let id = plan["id"].as_str().unwrap();
            let j = Journal::open_default().unwrap();
            assert_eq!(j.plan(id).unwrap().1, "planned");
            assert!(j.items(id).unwrap().is_empty());
            assert_eq!(fs::read_to_string(target).unwrap(), "keep");
            assert_eq!(
                rpc("ebb_apply", json!({"plan_id":id}))["result"]["isError"],
                true
            );
        }
        "rpc_invalid_arguments" => {
            let mut accepted = vec![];
            for args in [
                Value::Null,
                json!([]),
                json!({"paths":"/something"}),
                json!({"paths":[17]}),
                json!({"entry_ids":[true]}),
            ] {
                if rpc("ebb_plan_cleanup", args.clone())["result"]["isError"] != true {
                    accepted.push(args);
                }
            }
            assert!(
                accepted.is_empty(),
                "invalid arguments created plans: {accepted:?}"
            );
        }
        "rpc_envelope" => {
            let replies = rpc_lines("{\"jsonrpc\":\"1.0\",\"id\":[],\"method\":\"ping\"}\n");
            assert_eq!(
                replies[0]["error"]["code"], -32600,
                "invalid JSON-RPC version/id was accepted"
            );
        }
        #[cfg(target_os = "macos")]
        "rpc_undo" | "rpc_undo_conflict" | "rpc_undo_ancestor" => {
            let original = if case == "rpc_undo_ancestor" {
                let parent = root.join("old-parent");
                fs::create_dir_all(parent.join("nested")).unwrap();
                fs::create_dir_all(root.join("unrelated/nested")).unwrap();
                let original = parent.join("nested/restored");
                fs::remove_dir(parent.join("nested")).unwrap();
                fs::remove_dir(&parent).unwrap();
                std::os::unix::fs::symlink(root.join("unrelated"), &parent).unwrap();
                original
            } else {
                root.join("restored")
            };
            let source = root.join("fake-trash-item");
            fs::write(&source, "old content").unwrap();
            let j = Journal::open_default().unwrap();
            let p = Plan::from_paths(&[]);
            j.save_plan(&p).unwrap();
            let db = rusqlite::Connection::open(ebb_core::db_path()).unwrap();
            db.execute("UPDATE plans SET status='applied'", []).unwrap();
            db.execute("INSERT INTO entries(plan_id,original,trashed,size,status,at) VALUES(?1,?2,?3,11,'applied',0)",
                rusqlite::params![p.id, original.to_str().unwrap(), source.to_str().unwrap()]).unwrap();
            if case == "rpc_undo_conflict" {
                fs::write(&original, "new content").unwrap();
            }
            rpc("ebb_undo", json!({"plan_id":p.id}));
            if case == "rpc_undo_conflict" {
                assert_eq!(fs::read_to_string(original).unwrap(), "new content");
                assert_eq!(j.plan(&p.id).unwrap().1, "applied");
            } else if case == "rpc_undo_ancestor" {
                assert!(
                    !root.join("unrelated/nested/restored").exists(),
                    "MCP undo followed a replaced ancestor symlink"
                );
            } else {
                assert!(
                    !original.exists(),
                    "MCP moved a file without human confirmation"
                );
            }
            assert!(source.exists());
        }
        "scan" => {
            let tree = root.join("tree");
            for folder in ["", "a", "a/deep", "b"] {
                let dir = tree.join(folder);
                fs::create_dir_all(&dir).unwrap();
                for i in 0..48 {
                    fs::write(dir.join(format!("f{i}")), vec![1; (i + 1) * 1024]).unwrap();
                }
            }
            #[cfg(unix)]
            {
                fs::hard_link(tree.join("f0"), tree.join("a/hardlink")).unwrap();
                std::os::unix::fs::symlink("missing", tree.join("dangling")).unwrap();
                fs::File::create(tree.join("sparse"))
                    .unwrap()
                    .set_len(8_000_000)
                    .unwrap();
            }
            for cap in [0, 1, 2, 8, 32, 200] {
                let n = scan_tree(&tree, cap);
                assert_eq!(scan::measure(&tree), (n.size, n.files));
                check_totals(&n);
                let (dirs, files) = ops::top(&n, 1000);
                assert!(dirs.iter().chain(&files).all(|c| c.path != n.path));
            }
        }
        #[cfg(unix)]
        "find_cli" => {
            // Discovery only: never execute claude/codex, even a fixture with that name.
            fs::write(root.join("claude"), "not executable").unwrap();
            let found = connect::find_cli("claude");
            assert_ne!(
                found,
                Some(PathBuf::from("claude")),
                "empty PATH trusted cwd"
            );
        }
        "selftest_ok" => {
            let bin = root.join(if cfg!(windows) {
                "ebb with spaces.exe"
            } else {
                "ebb with spaces ' $;"
            });
            fs::copy(ebb(), &bin).unwrap();
            assert_eq!(connect::selftest(&bin).unwrap(), 8);
        }
        #[cfg(unix)]
        "selftest_timeout" | "selftest_descendant" => {
            use std::os::unix::fs::PermissionsExt;
            let script = root.join("silent-server");
            let body = if case == "selftest_timeout" {
                "#!/bin/sh\necho $$ > \"$HOME/pid\"\nexec /bin/sleep 30\n"
            } else {
                "#!/bin/sh\n/bin/sleep 30 &\necho $! > \"$HOME/pid\"\nwait\n"
            };
            fs::write(&script, body).unwrap();
            fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
            let start = Instant::now();
            let result = connect::selftest(&script);
            let pid = fs::read_to_string(home.join("pid")).unwrap();
            let alive = Command::new("/bin/kill")
                .args(["-0", pid.trim()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success();
            // Clean up our own descendant before a regression assertion can fail.
            if alive {
                Command::new("/bin/kill")
                    .args(["-KILL", pid.trim()])
                    .status()
                    .unwrap();
            }
            assert!(result.is_err());
            assert!(start.elapsed() >= Duration::from_secs(7));
            assert!(start.elapsed() < Duration::from_secs(12));
            assert!(!alive, "selftest left a running descendant/zombie");
        }
        _ => panic!("unknown case: {case}"),
    }
}

#[test]
fn json_preserves_order_and_literal_command_path() {
    run_case("json");
}
#[test]
fn invalid_json_and_schema_are_not_overwritten() {
    run_case("invalid_json");
}
#[test]
fn backup_failure_aborts_config_write() {
    run_case("backup_error");
}
#[test]
fn cli_without_yes_only_saves_and_protects_paths_and_never_ids() {
    run_case("cli_plans");
}
#[test]
fn malformed_rpc_does_not_crash_server() {
    run_case("rpc_malformed");
}
#[test]
fn mcp_plans_do_not_move_files_and_apply_tool_is_absent() {
    run_case("rpc_plan");
}
#[test]
fn scan_buckets_preserve_bytes_logical_sizes_and_counts() {
    run_case("scan");
}
#[test]
fn selftest_accepts_executable_paths_with_spaces() {
    run_case("selftest_ok");
}
#[cfg(unix)]
#[test]
fn selftest_timeout_kills_and_reaps_direct_child() {
    run_case("selftest_timeout");
}
#[cfg(target_os = "macos")]
#[test]
fn mcp_undo_preserves_existing_destination() {
    run_case("rpc_undo_conflict");
}

#[test]
fn reconnect_preserves_original_backup() {
    run_case("backup_retained");
}
#[test]
fn connect_preserves_foreign_ebb_entry() {
    run_case("foreign_connect");
}
#[test]
fn disconnect_preserves_foreign_ebb_entry() {
    run_case("foreign_disconnect");
}
#[cfg(unix)]
#[test]
fn config_symlink_is_rejected() {
    run_case("config_symlink");
}
#[cfg(unix)]
#[test]
fn backup_symlink_is_rejected() {
    run_case("backup_symlink");
}
#[cfg(unix)]
#[test]
fn find_cli_rejects_implicit_cwd_and_non_executable_file() {
    run_case("find_cli");
}
#[test]
fn mcp_rejects_invalid_plan_arguments() {
    run_case("rpc_invalid_arguments");
}
#[test]
fn mcp_rejects_invalid_rpc_envelope() {
    run_case("rpc_envelope");
}
#[cfg(target_os = "macos")]
#[test]
fn mcp_undo_requires_confirmation_before_moving_files() {
    run_case("rpc_undo");
}
#[cfg(target_os = "macos")]
#[test]
fn mcp_undo_rejects_replaced_ancestor_symlink() {
    run_case("rpc_undo_ancestor");
}
#[cfg(unix)]
#[test]
fn selftest_timeout_cleans_up_descendants() {
    run_case("selftest_descendant");
}
