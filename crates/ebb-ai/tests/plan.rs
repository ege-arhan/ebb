use serde_json::json;

// Tek test: EBB_DATA_DIR'i bu süreçte yalnızca bu test ayarlar.
#[test]
fn plan_cleanup_protected_root_is_empty_and_not_applied() {
    let d = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("EBB_DATA_DIR", d.path()) };
    let r = ebb_ai::run_tool("plan_cleanup", &json!({"paths":["/"]})).unwrap();
    assert_eq!(r["item_count"], 0);
    assert_eq!(r["skipped"], 1);
    let id = r["plan_id"].as_str().unwrap();
    let j = ebb_core::journal::Journal::open_default().unwrap();
    let (_, status) = j.plan(id).unwrap();
    assert_ne!(status, "applied");
}
