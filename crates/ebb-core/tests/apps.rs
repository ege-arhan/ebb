use ebb_core::apps::*;
use std::fs;
use std::path::PathBuf;

fn mk(root: &std::path::Path, rel: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, b"x").unwrap();
}

fn roots(t: &std::path::Path) -> LeftoverRoots {
    LeftoverRoots {
        dirs: vec![
            (t.join("support"), "support"),
            (t.join("prefs"), "preferences"),
            (t.join("groups"), "container"),
        ],
    }
}

#[test]
fn finds_exact_and_ignores_lookalikes() {
    let t = tempfile::tempdir().unwrap();
    let r = t.path();
    for f in [
        "support/Slack/a.db",
        "support/SlackBot/b.db",
        "support/Slack Helper/c",
        "support/Google/Chrome/x",
        "support/Google/Slack/y",
        "prefs/com.tinyspeck.slackmacgap.plist",
        "prefs/com.tinyspeck.slackmacgap.helper.plist",
        "prefs/com.tinyspeck.slackmacgapX.plist",
        "groups/ABCDE.com.tinyspeck.slackmacgap/z",
        "groups/other.group/z",
    ] {
        mk(r, f);
    }
    let l = leftovers_in(&roots(r), "Slack", Some("com.tinyspeck.slackmacgap"));
    let names: Vec<String> = l
        .iter()
        .map(|x| {
            x.path
                .strip_prefix(r)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    let has = |s: &str| names.iter().any(|n| n == s);
    assert!(has("support/Slack") && has("prefs/com.tinyspeck.slackmacgap.plist"));
    assert!(has("prefs/com.tinyspeck.slackmacgap.helper.plist"));
    assert!(has("groups/ABCDE.com.tinyspeck.slackmacgap"));
    assert!(!has("support/SlackBot") && !has("support/Slack Helper"));
    assert!(!has("prefs/com.tinyspeck.slackmacgapX.plist"));
    assert!(!has("support") && !has("support/Google") && !has("support/Google/Slack"));
    let slack = l
        .iter()
        .find(|x| x.path.ends_with("support/Slack"))
        .unwrap();
    assert_eq!(
        (slack.confidence.as_str(), slack.kind.as_str()),
        ("medium", "support")
    );
    let p = l
        .iter()
        .find(|x| x.path.ends_with("com.tinyspeck.slackmacgap.plist"))
        .unwrap();
    assert_eq!(p.confidence, "high");
}

#[test]
fn generic_names_and_apple_ids_are_not_matched() {
    let t = tempfile::tempdir().unwrap();
    for f in [
        "support/Data/a",
        "support/App/a",
        "support/Mail/a",
        "prefs/com.apple.mail.plist",
    ] {
        mk(t.path(), f);
    }
    assert!(leftovers_in(&roots(t.path()), "Data", None).is_empty());
    assert!(leftovers_in(&roots(t.path()), "App", None).is_empty());
    assert!(leftovers_in(&roots(t.path()), "Mail", Some("com.apple.mail")).is_empty());
}

fn app(path: &str) -> InstalledApp {
    InstalledApp {
        id: "id".into(),
        name: "Foo Bar".into(),
        path: PathBuf::from(path),
        bundle_id: None,
        version: None,
        size: 0,
        running: false,
        last_used_days: None,
        system: false,
    }
}

#[test]
fn uninstall_paths_order_and_medium() {
    let t = tempfile::tempdir().unwrap();
    let lo = |n: &str, c: &str| Leftover {
        path: t.path().join(n),
        size: 1,
        kind: "support".into(),
        confidence: c.into(),
    };
    let p = UninstallPreview {
        app: app("/x/Foo Bar.app"),
        leftovers: vec![lo("hi", "high"), lo("med", "medium")],
        total: 0,
        warnings: vec![],
    };
    let high = uninstall_paths(&p, false);
    assert_eq!(
        high,
        vec![PathBuf::from("/x/Foo Bar.app"), t.path().join("hi")]
    );
    let all = uninstall_paths(&p, true);
    assert_eq!(all.len(), 3);
    assert_eq!(all[2], t.path().join("med"));
}

#[cfg(unix)]
#[test]
fn unwritable_leftovers_are_skipped() {
    let p = UninstallPreview {
        app: app("/x/Foo Bar.app"),
        leftovers: vec![Leftover {
            path: PathBuf::from("/usr/bin/zz-ebb-test"),
            size: 1,
            kind: "launch".into(),
            confidence: "high".into(),
        }],
        total: 0,
        warnings: vec![],
    };
    // Yalnız uygulama kalır (root olmayan kullanıcı /usr/bin'e yazamaz).
    if unsafe { libc::geteuid() } != 0 {
        assert_eq!(uninstall_paths(&p, true).len(), 1);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn macos_list_and_system_refused() {
    let apps = installed_apps();
    assert!(!apps.is_empty());
    let mut ids: Vec<_> = apps.iter().map(|a| a.id.clone()).collect();
    ids.sort();
    let n = ids.len();
    ids.dedup();
    assert_eq!(n, ids.len());
    assert!(apps.windows(2).all(|w| w[0].size >= w[1].size));
    if let Some(sys) = apps.iter().find(|a| a.system) {
        assert!(matches!(preview(&sys.id), Err(ebb_core::Error::Guard(_))));
    }
    assert!(matches!(
        preview("/nope.app"),
        Err(ebb_core::Error::NotFound(_))
    ));
}
