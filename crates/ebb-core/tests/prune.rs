//! Prune eylemleri: gerçek bir temizlik ASLA çalıştırılmaz.
use ebb_core::{Error, prune};

#[test]
fn available_lists_only_installed_and_fixed_commands() {
    let a = prune::available();
    for x in &a {
        assert!(x.installed && !x.command.is_empty());
        assert!(matches!(x.safety, "safe" | "review"));
        // Kullanıcı verisini silen komutlar hiçbir zaman yer almaz.
        for bad in ["system prune", "volume", "-a ", "rm ", "gc"] {
            assert!(!x.command.contains(bad) || x.id == "cargo", "{}", x.command);
        }
        assert!(
            !x.command
                .contains(&*std::env::var("HOME").unwrap_or("/nonexistent".into()))
        );
    }
    let mut ids: Vec<_> = a.iter().map(|x| x.id).collect();
    ids.dedup();
    assert_eq!(ids.len(), a.len());
}

#[test]
fn unknown_id_is_error() {
    assert!(matches!(prune::run("unknown"), Err(Error::NotFound(_))));
    assert!(matches!(
        prune::run("docker; rm -rf /"),
        Err(Error::NotFound(_))
    ));
}
