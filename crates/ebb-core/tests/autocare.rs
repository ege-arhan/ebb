use ebb_core::autocare::*;
use ebb_core::{journal::Journal, trashbin::TrashEntry};
use std::fs::{File, FileTimes};
use std::time::{Duration, SystemTime};

fn rule(schedule: Schedule, last_run: Option<i64>) -> Rule {
    Rule {
        id: "r".into(),
        name: "r".into(),
        kind: RuleKind::SafeCaches,
        schedule,
        enabled: true,
        auto_apply: false,
        last_run,
        last_result: None,
    }
}

// 2024-01-01 00:00:00 UTC = Pazartesi.
const MON: i64 = 1_704_067_200;
const DAY: i64 = 86_400;

#[test]
fn due_daily() {
    assert!(is_due(&rule(Schedule::Daily, None), MON, 50.0));
    assert!(!is_due(
        &rule(Schedule::Daily, Some(MON - 20 * 3600 + 1)),
        MON,
        50.0
    ));
    assert!(is_due(
        &rule(Schedule::Daily, Some(MON - 20 * 3600)),
        MON,
        50.0
    ));
}

#[test]
fn due_weekly() {
    let w = |d| Schedule::Weekly { weekday: d };
    assert!(is_due(&rule(w(0), None), MON, 50.0));
    assert!(is_due(&rule(w(1), None), MON + DAY, 50.0));
    assert!(is_due(&rule(w(6), None), MON + 6 * DAY + 86_399, 50.0));
    assert!(!is_due(&rule(w(1), None), MON, 50.0)); // yanlış gün
    assert!(!is_due(&rule(w(0), Some(MON - 6 * DAY + 1)), MON, 50.0));
    assert!(is_due(&rule(w(0), Some(MON - 6 * DAY)), MON, 50.0));
}

#[test]
fn due_low_disk() {
    let s = Schedule::OnLowDisk { free_percent: 10 };
    assert!(is_due(&rule(s.clone(), None), MON, 10.0));
    assert!(is_due(&rule(s.clone(), None), MON, 3.5));
    assert!(!is_due(&rule(s.clone(), None), MON, 10.1));
    assert!(!is_due(
        &rule(s.clone(), Some(MON - 6 * 3600 + 1)),
        MON,
        1.0
    ));
    assert!(is_due(&rule(s, Some(MON - 6 * 3600)), MON, 1.0));
}

#[test]
fn disabled_never_due() {
    let mut r = rule(Schedule::Daily, None);
    r.enabled = false;
    assert!(!is_due(&r, MON, 0.0));
}

#[test]
fn defaults_all_disabled() {
    let d = default_rules();
    assert!(d.len() >= 4);
    assert!(d.iter().all(|r| !r.enabled && !r.auto_apply));
    let mut ids: Vec<_> = d.iter().map(|r| r.id.clone()).collect();
    ids.dedup();
    assert_eq!(ids.len(), d.len());
}

#[test]
fn rules_crud_roundtrip_and_serde() {
    let dir = tempfile::tempdir().unwrap();
    let j = Journal::open(&dir.path().join("t.db")).unwrap();
    assert!(j.rules().unwrap().is_empty());
    let mut r = rule(Schedule::Weekly { weekday: 2 }, None);
    r.kind = RuleKind::LargeOld {
        min_mb: 500,
        min_age_days: 90,
    };
    j.save_rule(&r).unwrap();
    r.enabled = false;
    j.save_rule(&r).unwrap(); // upsert
    let got = j.rules().unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].kind, r.kind);
    assert!(!got[0].enabled);
    let js = serde_json::to_string(&got[0].schedule).unwrap();
    assert_eq!(js, r#"{"type":"weekly","weekday":2}"#);
    j.delete_rule("r").unwrap();
    assert!(j.rules().unwrap().is_empty());
}

#[test]
fn large_old_never_auto_applies() {
    let mut r = rule(Schedule::Daily, None);
    r.kind = RuleKind::LargeOld {
        min_mb: 1,
        min_age_days: 1,
    };
    r.auto_apply = true;
    assert!(!effective_auto_apply(&r));
    r.kind = RuleKind::SafeCaches;
    assert!(effective_auto_apply(&r));
}

#[test]
fn installers_respect_age_and_extension() {
    let d = tempfile::tempdir().unwrap();
    let old = d.path().join("old.dmg");
    let new = d.path().join("new.pkg");
    let doc = d.path().join("notes.txt");
    for p in [&old, &new, &doc] {
        std::fs::write(p, vec![0u8; 1000]).unwrap();
    }
    let t = SystemTime::now() - Duration::from_secs(100 * 86_400);
    File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_times(FileTimes::new().set_accessed(t).set_modified(t))
        .unwrap();
    let dirs = vec![d.path().to_path_buf()];
    let items = installers_in(&dirs, 30);
    assert_eq!(items.len(), 1);
    assert!(items[0].path.ends_with("old.dmg"));
    assert_eq!(items[0].size, 1000);
    assert_eq!(installers_in(&dirs, 0).len(), 2);
    assert!(installers_in(&dirs, 365).is_empty());
}

#[test]
fn large_old_collects_only_in_given_root() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("big.bin"), vec![0u8; 2_000_000]).unwrap();
    std::fs::write(d.path().join("small.bin"), vec![0u8; 10]).unwrap();
    let items = large_old_in(d.path(), 1, 0).unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].path.ends_with("big.bin"));
}

#[test]
fn old_trash_filters_by_age() {
    let e = |id: &str, at: Option<i64>| TrashEntry {
        id: id.into(),
        name: id.into(),
        size: 1,
        deleted_at: at,
        original: None,
        by_ebb: false,
        plan_id: None,
        restorable: false,
    };
    let now = MON;
    let all = vec![
        e("old", Some(now - 31 * DAY)),
        e("edge", Some(now - 30 * DAY)),
        e("new", Some(now - 29 * DAY)),
        e("unknown", None),
    ];
    let ids: Vec<_> = old_trash(&all, 30, now)
        .iter()
        .map(|e| e.id.clone())
        .collect();
    assert_eq!(ids, ["old", "edge"]);
}
