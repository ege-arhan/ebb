use ebb_core::snaps::*;

#[test]
fn parses_tmutil_output() {
    let s = "Snapshots for disk /:\ncom.apple.TimeMachine.2024-01-02-123456.local\ncom.apple.TimeMachine.2024-01-03-010203.local\n";
    assert_eq!(
        parse_snapshots(s),
        ["2024-01-02-123456", "2024-01-03-010203"]
    );
    assert!(parse_snapshots("Snapshots for disk /:\n").is_empty());
    assert!(parse_snapshots("garbage\n\n").is_empty());
}

#[test]
fn parses_purgeable() {
    let x = "<dict>\n\t<key>APFSContainerFree</key>\n\t<integer>100</integer>\n\t<key>APFSPurgeableSpace</key>\n\t<integer>12345</integer>\n</dict>";
    assert_eq!(plist_int(x, "APFSContainerFree"), Some(100));
    assert_eq!(purgeable_bytes(x), Some(12345));
    let big = "<key>PurgeableSpace</key><integer>900000000000</integer>";
    assert_eq!(purgeable_bytes(big), Some(50_000_000_000));
    assert_eq!(purgeable_bytes("<key>X</key><integer>1</integer>"), None);
    assert_eq!(plist_int("<key>A</key><string>x</string>", "A"), None);
}

#[test]
fn list_is_sane() {
    for i in list() {
        assert!(!i.command.is_empty() && !i.label.is_empty());
        assert!(i.safety == "safe" || i.safety == "review");
    }
}

#[test]
fn unknown_id_errors() {
    assert!(run("nope").is_err());
    assert!(run("").is_err());
}

#[test]
fn commands_are_fixed() {
    for (bin, args) in fixed_commands() {
        assert!(!bin.contains(' ') && !bin.contains('/'));
        assert!(
            args.iter()
                .all(|a| !a.contains(';') && !a.contains('|') && !a.contains('$'))
        );
        assert!(args.iter().all(|a| !a.contains("deletelocalsnapshots")));
    }
}
