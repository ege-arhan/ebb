//! Sistem modülü: gerçek hiçbir uygulama kapatılmaz, gerçek hiçbir başlangıç öğesi değiştirilmez.
use ebb_core::{Error, system};
use std::time::Instant;

#[test]
fn memory_values_are_sane() {
    let m = system::memory();
    assert!(m.total > 0);
    assert!(m.used <= m.total && m.available <= m.total);
    assert!(m.swap_used <= m.swap_total);
    assert!((0.0..=1.0).contains(&m.pressure));
}

#[test]
fn top_apps_sorted_and_fast() {
    let t = Instant::now();
    let apps = system::top_apps(20);
    assert!(t.elapsed().as_millis() < 1500, "{:?}", t.elapsed());
    assert!(!apps.is_empty() && apps.len() <= 20);
    assert!(apps.windows(2).all(|w| w[0].memory >= w[1].memory));
    assert!(
        apps.iter()
            .all(|a| !a.pids.is_empty() && !a.name.is_empty())
    );
    // Test sürecinin kendisi asla kapatılabilir görünmemeli.
    let me = std::process::id();
    let all = system::top_apps(usize::MAX);
    let own = all.iter().find(|a| a.pids.contains(&me)).unwrap();
    assert!(!own.quittable);
}

#[test]
fn quit_refuses_self_and_init() {
    let me = std::process::id();
    for pids in [vec![me], vec![1], vec![0], vec![u32::MAX], vec![1, me]] {
        assert!(
            matches!(system::quit_app(&pids), Err(Error::Guard(_))),
            "{pids:?}"
        );
    }
    assert_eq!(system::quit_app(&[]).unwrap(), 0);
}

#[test]
fn launch_agents_round_trip_in_temp_dirs() {
    let d = tempfile::tempdir().unwrap();
    let user = d.path().join("LaunchAgents");
    let sys = d.path().join("LaunchDaemons");
    std::fs::create_dir_all(&user).unwrap();
    std::fs::create_dir_all(&sys).unwrap();
    std::fs::write(
        user.join("com.test.agent.plist"),
        "<plist><dict><key>Label</key>\n\t<string>com.test.agent</string></dict></plist>",
    )
    .unwrap();
    std::fs::write(user.join("notes.txt"), "x").unwrap();
    std::fs::write(sys.join("com.vendor.daemon.plist"), "bplist00...").unwrap();

    let list = |label: &str| {
        let items = system::launch_agents_in(&user, std::slice::from_ref(&sys));
        assert_eq!(items.len(), 2, "{label}: {items:?}");
        items
    };
    let items = list("initial");
    let a = items.iter().find(|i| i.scope == "user").unwrap().clone();
    assert_eq!(a.name, "com.test.agent");
    assert!(a.enabled && a.can_toggle);
    let s = items.iter().find(|i| i.scope == "system").unwrap();
    assert_eq!(s.name, "com.vendor.daemon"); // ikili plist: dosya adı
    assert!(!s.can_toggle);
    // Sistem öğesi salt okunur; reddedilir, hiçbir şey çalıştırılmaz.
    assert!(matches!(
        system::set_startup_enabled(&s.id, false),
        Err(Error::Guard(_))
    ));

    let file = a.id.rsplit(':').next().unwrap();
    let off = system::move_item(&user, file, false).unwrap();
    assert!(off.ends_with("Disabled by Ebb/com.test.agent.plist"));
    let b = list("disabled")
        .into_iter()
        .find(|i| i.scope == "user")
        .unwrap();
    assert_eq!(b.id, a.id, "id stays stable");
    assert!(!b.enabled);
    // İkinci kez kapatmak sorun değil.
    system::move_item(&user, file, false).unwrap();

    system::move_item(&user, file, true).unwrap();
    let c = list("enabled")
        .into_iter()
        .find(|i| i.scope == "user")
        .unwrap();
    assert!(c.enabled && c.id == a.id);
    assert!(user.join("com.test.agent.plist").is_file());

    // Yol geçişi reddedilir.
    assert!(system::move_item(&user, "../x.plist", false).is_err());
    assert!(matches!(
        system::set_startup_enabled("user:launch_agent:../../x.plist", false),
        Err(Error::Guard(_)) | Err(Error::NotFound(_))
    ));
}

#[test]
fn autostart_hidden_toggle() {
    let d = tempfile::tempdir().unwrap();
    let user = d.path().join("autostart");
    let sys = d.path().join("xdg");
    std::fs::create_dir_all(&user).unwrap();
    std::fs::create_dir_all(&sys).unwrap();
    let f = user.join("app.desktop");
    std::fs::write(
        &f,
        "[Desktop Entry]\nName=My App\nExec=app\nX-GNOME-Autostart-enabled=false\n",
    )
    .unwrap();
    std::fs::write(sys.join("app.desktop"), "[Desktop Entry]\nName=Shadowed\n").unwrap();
    std::fs::write(sys.join("other.desktop"), "[Desktop Entry]\nName=Other\n").unwrap();

    let items = system::autostart_in(&user, &sys);
    assert_eq!(items.len(), 2, "user file shadows the system one");
    let a = items.iter().find(|i| i.scope == "user").unwrap();
    assert_eq!(a.name, "My App");
    assert!(!a.enabled);

    system::set_desktop_hidden(&f, false).unwrap();
    assert!(system::autostart_in(&user, &sys)[0].enabled);
    system::set_desktop_hidden(&f, true).unwrap();
    let text = std::fs::read_to_string(&f).unwrap();
    assert!(text.starts_with("[Desktop Entry]\nHidden=true\n"));
    assert!(!system::autostart_in(&user, &sys)[0].enabled);
}

#[test]
fn health_is_bounded_and_has_tips() {
    let h = system::health();
    assert!(h.score <= 100);
    assert!(!h.tips.is_empty());
    assert!((0.0..=1.0).contains(&h.disk_free_ratio));
    for t in &h.tips {
        assert!(["good", "info", "warn", "bad"].contains(&t.severity));
    }
}
