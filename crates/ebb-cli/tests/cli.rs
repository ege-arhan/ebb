//! Derlenmiş `ebb` ikilisini, geçici bir HOME ile uçtan uca çalıştırır. Gerçek kullanıcı verisine dokunmaz.
use std::{fs, path::Path, process::Command};

fn ebb(home: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_ebb"))
        .args(args)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("EBB_DATA_DIR", home.join("data"))
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

// Windows: `dirs` ignores USERPROFILE (Known Folder API), so HOME cannot be isolated there; run these on Unix.
#[cfg(not(windows))]
#[test]
fn connect_writes_preserves_and_removes() {
    let h = tempfile::tempdir().unwrap();
    let cfg = h.path().join(".cursor/mcp.json");
    fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    fs::write(&cfg, r#"{"mcpServers":{"other":{"command":"x"}},"keep":1}"#).unwrap();

    let (ok, out) = ebb(h.path(), &["connect", "cursor"]);
    assert!(ok, "{out}");
    assert!(out.contains("answers with 15 tools"), "{out}");
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&cfg).unwrap()).unwrap();
    assert_eq!(
        v["mcpServers"]["other"]["command"], "x",
        "mevcut sunucu korunmalı"
    );
    assert_eq!(v["mcpServers"]["ebb"]["args"][0], "mcp");
    assert_eq!(v["keep"], 1);
    assert!(h.path().join(".cursor/mcp.json.ebb-backup").exists());

    let (_, list) = ebb(h.path(), &["connect", "--list", "--json"]);
    let l: serde_json::Value = serde_json::from_str(&list).unwrap();
    assert!(
        l.as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "cursor" && c["connected"] == true)
    );

    let (ok, out) = ebb(h.path(), &["connect", "--remove", "cursor"]);
    assert!(ok, "{out}");
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&cfg).unwrap()).unwrap();
    assert!(v["mcpServers"].get("ebb").is_none());
    assert_eq!(v["mcpServers"]["other"]["command"], "x");
}

// Windows: `dirs` ignores USERPROFILE (Known Folder API), so HOME cannot be isolated there; run these on Unix.
#[cfg(not(windows))]
#[test]
fn connect_refuses_invalid_json_without_touching_it() {
    let h = tempfile::tempdir().unwrap();
    let cfg = h.path().join(".cursor/mcp.json");
    fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    let original = "{ // comment\n\"mcpServers\": {} }";
    fs::write(&cfg, original).unwrap();
    let (ok, out) = ebb(h.path(), &["connect", "cursor"]);
    assert!(out.contains("not valid JSON") || !ok, "{out}");
    assert_eq!(fs::read_to_string(&cfg).unwrap(), original);
}

#[test]
fn clean_never_plans_protected_paths() {
    let h = tempfile::tempdir().unwrap();
    let (ok, out) = ebb(h.path(), &["clean", "--paths", "/", "/usr", "--json"]);
    assert!(ok, "{out}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["items"].as_array().unwrap().len(), 0, "{out}");
    assert_eq!(v["skipped"].as_array().unwrap().len(), 2);
}

#[test]
fn scan_json_matches_fixture() {
    let h = tempfile::tempdir().unwrap();
    let d = h.path().join("proj");
    fs::create_dir_all(d.join("sub")).unwrap();
    fs::write(d.join("a.bin"), vec![0u8; 200_000]).unwrap();
    fs::write(d.join("sub/b.bin"), vec![0u8; 50_000]).unwrap();
    let (ok, out) = ebb(
        h.path(),
        &["scan", d.to_str().unwrap(), "--json", "--depth", "2"],
    );
    assert!(ok, "{out}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["files"], 2);
    assert!(v["logical"].as_u64().unwrap() >= 250_000);
}
