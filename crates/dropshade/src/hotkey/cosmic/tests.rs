use super::*;

const COMMAND: &str = "/opt/dropshade/dropshade toggle";

/// The file COSMIC Settings wrote for a custom F12 shortcut named Dropshade.
const SETTINGS_FILE: &str = r#"{
    (
        modifiers: [],
        key: "F12",
        description: Some("Dropshade"),
    ): Spawn("/home/me/bin/dropshade toggle"),
}"#;

const OTHERS: &str = r#"{
    (modifiers: [Super, Shift], key: "Return"): Spawn("alacritty"),
    (modifiers: [Super], key: "p"): Disable,
    (modifiers: [Ctrl, Alt], keycode: Some(119), description: Some("Mute")): System(VolumeMute),
}"#;

const DEFAULTS: &str = r#"{
    (modifiers: [Super], key: "q"): Close,
    (modifiers: [Super], key: "Escape"): System(LockScreen),
}"#;

fn keys(text: &str) -> Accelerator {
    Accelerator::parse(text).expect(text)
}

fn planned(custom: &str, wanted: &str) -> (Change, String) {
    plan(custom, Some(DEFAULTS), keys(wanted), COMMAND).expect("plan")
}

fn refused(custom: &str, wanted: &str) -> String {
    plan(custom, Some(DEFAULTS), keys(wanted), COMMAND).expect_err("refused")
}

#[test]
fn the_settings_entry_gets_the_new_keys_and_keeps_its_command() {
    let (change, text) = planned(SETTINGS_FILE, "<Super>grave");
    assert_eq!(change, Change::Updated);
    assert_eq!(
        text,
        r#"{
    (
        modifiers: [Super],
        key: "grave",
        description: Some("Dropshade"),
    ): Spawn("/home/me/bin/dropshade toggle"),
}"#
    );
}

#[test]
fn the_same_keys_leave_the_file_alone() {
    let (change, text) = planned(SETTINGS_FILE, "F12");
    assert_eq!(change, Change::Unchanged);
    assert_eq!(text, SETTINGS_FILE);
}

#[test]
fn a_missing_or_empty_file_gets_a_new_entry() {
    for custom in ["", "  \n", "{}"] {
        let (change, text) = planned(custom, "<Control><Alt>t");
        assert_eq!(change, Change::Added);
        assert_eq!(
            text,
            r#"{
    (
        modifiers: [Ctrl, Alt],
        key: "t",
        description: Some("Dropshade"),
    ): Spawn("/opt/dropshade/dropshade toggle"),
}"#
        );
    }
}

#[test]
fn other_shortcuts_survive_exactly() {
    let (change, text) = planned(OTHERS, "F12");
    assert_eq!(change, Change::Added);
    let before = parse(OTHERS).expect("others");
    let after = parse(&text).expect("new file");
    assert_eq!(after[..before.len()], before[..]);
    assert_eq!(after.len(), before.len() + 1);
    assert!(text.contains("keycode: Some(119)"), "{text}");
    assert!(text.contains(": System(VolumeMute),"), "{text}");
    assert!(text.contains(": Disable,"), "{text}");
}

#[test]
fn files_cosmic_would_not_load_are_left_alone() {
    for (custom, why) in [
        (
            r#"{ (modifiers: [], key: "F11", repeat: true): Close }"#,
            "unknown field",
        ),
        (
            r#"{ (modifiers: [Hyper], key: "F11"): Close }"#,
            "unknown modifier",
        ),
        (
            r#"{ (modifiers: [Super], key: "NotAKey"): Close }"#,
            "unknown key",
        ),
        (r#"{ (key: "F11"): Close }"#, "missing modifiers"),
        (r"[1, 2]", "not a map"),
        (r#"{ (modifiers: [], key: "F11"): Close"#, "truncated"),
        ("garbage", "not RON"),
    ] {
        let error = refused(custom, "F12");
        assert!(
            error.contains("not in the expected format"),
            "{why}: {error}"
        );
    }
}

#[test]
fn several_dropshade_entries_are_left_alone() {
    let custom = r#"{
        (modifiers: [], key: "F12", description: Some("Dropshade")): Spawn("dropshade toggle"),
        (modifiers: [Super], key: "grave", description: Some("dropshade")): Spawn("dropshade toggle"),
    }"#;
    assert!(refused(custom, "F11").contains("several"));
}

#[test]
fn a_dropshade_entry_that_runs_no_command_is_left_alone() {
    let custom = r#"{ (modifiers: [], key: "F12", description: Some("Dropshade")): Close }"#;
    assert!(refused(custom, "F11").contains("does not run a command"));
}

#[test]
fn keys_taken_by_another_custom_shortcut_are_refused() {
    let error = refused(OTHERS, "<Super><Shift>Return");
    assert_eq!(
        error,
        "Super+Shift+Return is already a custom COSMIC shortcut for Spawn(\"alacritty\")"
    );
    let error = refused(OTHERS, "<Super>p");
    assert!(
        error.contains("Disable"),
        "a disabled default counts: {error}"
    );
}

#[test]
fn keys_of_cosmic_defaults_are_refused() {
    let error = refused(SETTINGS_FILE, "<Super>q");
    assert_eq!(error, "COSMIC already uses Super+q for Close");
}

#[test]
fn unreadable_defaults_do_not_block_binding() {
    let (change, _) = plan(SETTINGS_FILE, Some("not ron"), keys("<Super>q"), COMMAND)
        .expect("defaults are only a check");
    assert_eq!(change, Change::Updated);
}

#[test]
fn the_installed_cosmic_defaults_load() {
    let Some(text) = defaults() else {
        return;
    };
    let entries = parse(&text).expect("COSMIC's own defaults load");
    assert!(entries.len() > 10);
}

#[test]
fn commands_are_written_so_the_shell_gets_them_back() {
    let command = "'/home/o neil/dropshade' toggle";
    let (_, text) = plan("", None, keys("F12"), command).expect("plan");
    let entries = parse(&text).expect("valid");
    assert_eq!(spawned(&entries[0].action).as_deref(), Some(command));
}

#[test]
fn the_current_entry_is_described() {
    let entries = parse(SETTINGS_FILE).expect("valid");
    let ours = ours(&entries).expect("one").expect("present");
    assert_eq!(ours.binding.to_string(), "F12");
    assert_eq!(
        spawned(&ours.action).as_deref(),
        Some("/home/me/bin/dropshade toggle")
    );
}

#[test]
fn replacing_stops_when_the_file_changed_meanwhile() {
    let dir = std::env::temp_dir().join(format!("dropshade-cosmic-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("custom");
    std::fs::write(&path, "{}").expect("write");
    let error = replace(&path, "new", Some("something else")).expect_err("changed");
    assert!(error.contains("meanwhile"), "{error}");
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "{}");
    replace(&path, "new", Some("{}")).expect("unchanged file is replaced");
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "new");
    assert!(!dir.join(".custom.dropshade-tmp").exists());
    std::fs::remove_dir_all(&dir).expect("clean up");
}

#[test]
fn text_that_reads_back_differently_is_never_written() {
    let expected = parse(OTHERS).expect("others");
    let error = verify(SETTINGS_FILE, &expected, keys("F12")).expect_err("differs");
    assert!(error.contains("did not read back"), "{error}");
    let error = verify(
        "{ (modifiers: [], keycode: 1): Close }",
        &expected,
        keys("F12"),
    )
    .expect_err("does not load");
    assert!(error.contains("would not load"), "{error}");
}

#[test]
#[ignore = "reads this user's COSMIC shortcuts; writes nothing"]
fn the_real_shortcuts_can_be_planned_without_writing() {
    let path = custom_path().expect("home");
    let custom = read(&path).expect("readable").unwrap_or_default();
    let defaults = defaults();
    for wanted in ["F12", "<Super>grave", "<Control><Alt>t"] {
        let (change, text) =
            plan(&custom, defaults.as_deref(), keys(wanted), COMMAND).expect(wanted);
        println!("{wanted}: {change:?}\n{text}\n");
    }
    assert_eq!(read(&path).expect("readable").unwrap_or_default(), custom);
}
