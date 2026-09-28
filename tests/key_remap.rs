use winlane::core::key_remap::{
    KeyRemap, KeyRemapper, MAX_REMAPS, key_code, modifier_flag, validate,
};
use winlane::core::shortcuts::{COMMAND, CONTROL, KEY_DOWN, KEY_UP, OPTION, SHIFT};

fn rule(
    id: &str,
    from: &str,
    from_modifiers: &[&str],
    to: &str,
    to_modifiers: &[&str],
) -> KeyRemap {
    KeyRemap {
        id: id.into(),
        enabled: true,
        from_key: from.into(),
        from_modifiers: from_modifiers
            .iter()
            .map(|value| value.to_string())
            .collect(),
        allow_extra_modifiers: false,
        to_key: to.into(),
        to_modifiers: to_modifiers.iter().map(|value| value.to_string()).collect(),
        except_apps: Vec::new(),
    }
}

/// Modifier flags of an event, without the device-dependent side bits.
fn modifiers(flags: u64) -> u64 {
    flags & (SHIFT | CONTROL | OPTION | COMMAND)
}

fn with_extra(mut remap: KeyRemap) -> KeyRemap {
    remap.allow_extra_modifiers = true;
    remap
}

fn excluding(mut remap: KeyRemap, apps: &[&str]) -> KeyRemap {
    remap.except_apps = apps.iter().map(|value| value.to_string()).collect();
    remap
}

#[test]
fn windows_style_control_shortcuts_become_command_shortcuts() {
    let mut remapper = KeyRemapper::new(&[with_extra(rule(
        "save",
        "s",
        &["control"],
        "s",
        &["command"],
    ))]);
    let down = remapper.rewrite(KEY_DOWN, key_code("s").unwrap(), CONTROL, None);
    let (key, flags) = down.expect("Ctrl+S must be rewritten");
    assert_eq!(key, key_code("s").unwrap());
    assert_eq!(
        modifiers(flags),
        COMMAND,
        "the control in must not stay held"
    );
    assert_eq!(
        flags & 0x0000_0008,
        0x0000_0008,
        "a synthetic command must look like the left-hand key"
    );

    // Auto-repeat and the release carry the same rewritten combination, even
    // though the physical key code on the release is the original one.
    assert_eq!(
        remapper.rewrite(KEY_DOWN, key_code("s").unwrap(), CONTROL, None),
        Some((key_code("s").unwrap(), flags)),
        "repeat must keep the rewritten combination"
    );
    assert_eq!(
        remapper.rewrite(KEY_UP, key_code("s").unwrap(), CONTROL, None),
        Some((key_code("s").unwrap(), flags)),
        "release must match the press, not the physical key"
    );
    assert_eq!(
        remapper.rewrite(KEY_UP, key_code("s").unwrap(), CONTROL, None),
        None,
        "a release without a press is left alone"
    );
    assert_eq!(
        remapper.rewrite(KEY_DOWN, key_code("s").unwrap(), 0, None),
        None,
        "plain S stays plain S"
    );
}

#[test]
fn a_rule_can_require_the_exact_modifier_set() {
    let end = key_code("end").unwrap();
    let right = key_code("right_arrow").unwrap();

    let mut exact = KeyRemapper::new(&[rule("end", "end", &[], "right_arrow", &["command"])]);
    let (key, flags) = exact
        .rewrite(KEY_DOWN, end, 0, None)
        .expect("a bare key rule matches the bare key");
    assert_eq!(key, right);
    assert_eq!(modifiers(flags), COMMAND);
    exact.rewrite(KEY_UP, end, 0, None);
    assert_eq!(
        exact.rewrite(KEY_DOWN, end, SHIFT, None),
        None,
        "Shift+End keeps the native selection behaviour"
    );

    let mut loose = KeyRemapper::new(&[with_extra(rule(
        "end",
        "end",
        &[],
        "right_arrow",
        &["command"],
    ))]);
    assert_eq!(
        loose.rewrite(KEY_DOWN, end, SHIFT, None),
        Some((right, flags)),
        "a rule that allows extra modifiers fires anyway"
    );
}

#[test]
fn excluded_applications_keep_the_native_combination() {
    let mut remapper = KeyRemapper::new(&[excluding(
        with_extra(rule("close", "w", &["control"], "w", &["command"])),
        &["com.apple.Terminal", "com.googlecode.iterm2"],
    )]);
    assert_eq!(
        remapper.rewrite(
            KEY_DOWN,
            key_code("w").unwrap(),
            CONTROL,
            Some("com.apple.Terminal")
        ),
        None,
        "terminals need Ctrl+W for the shell"
    );
    assert!(
        remapper
            .rewrite(
                KEY_DOWN,
                key_code("w").unwrap(),
                CONTROL,
                Some("dev.warp.Warp-Stable")
            )
            .is_some(),
        "every other application is remapped"
    );
}

#[test]
fn disabled_rules_and_unusable_names_are_ignored() {
    let mut disabled = rule("save", "s", &["control"], "s", &["command"]);
    disabled.enabled = false;
    let mut remapper = KeyRemapper::new(&[disabled]);
    assert!(remapper.is_empty());
    assert_eq!(
        remapper.rewrite(KEY_DOWN, key_code("s").unwrap(), CONTROL, None),
        None
    );

    // A rule that never passed validation must not crash the tap.
    let mut broken = rule("broken", "s", &["control"], "s", &["command"]);
    broken.to_key = "not_a_key".into();
    let remapper = KeyRemapper::new(&[broken]);
    assert!(remapper.is_empty());
}

#[test]
fn validation_reports_unusable_rules_before_they_replace_the_config() {
    assert!(validate(&[rule("ok", "s", &["control"], "s", &["command"])]).is_ok());
    assert!(validate(&[]).is_ok());

    let mut unknown_key = rule("bad", "s", &["control"], "s", &["command"]);
    unknown_key.to_key = "f20".into();
    assert!(validate(&[unknown_key]).is_err());

    let mut unknown_modifier = rule("bad", "s", &["hyper"], "s", &["command"]);
    unknown_modifier.from_modifiers = vec!["hyper".into()];
    assert!(validate(&[unknown_modifier]).is_err());

    let mut empty_id = rule("", "s", &["control"], "s", &["command"]);
    empty_id.id = "  ".into();
    assert!(validate(&[empty_id]).is_err());

    let mut blank_exclusion = rule("bad", "s", &["control"], "s", &["command"]);
    blank_exclusion.except_apps = vec!["  ".into()];
    assert!(validate(&[blank_exclusion]).is_err());

    let duplicate = rule("one", "s", &["control"], "s", &["command"]);
    let mut copy = rule("two", "s", &["control"], "w", &["command"]);
    copy.from_key = "s".into();
    assert!(
        validate(&[duplicate, copy]).is_err(),
        "a second rule for the same combination is dead configuration"
    );

    let many: Vec<_> = (0..=MAX_REMAPS)
        .map(|index| {
            rule(
                &format!("rule-{index}"),
                "s",
                &["control"],
                "s",
                &["command"],
            )
        })
        .collect();
    assert!(validate(&many).is_err());
}

#[test]
fn reset_forgets_keys_that_were_held_when_the_tap_stopped() {
    let mut remapper = KeyRemapper::new(&[with_extra(rule(
        "save",
        "s",
        &["control"],
        "s",
        &["command"],
    ))]);
    assert!(
        remapper
            .rewrite(KEY_DOWN, key_code("s").unwrap(), CONTROL, None)
            .is_some()
    );
    remapper.reset();
    assert_eq!(
        remapper.rewrite(KEY_UP, key_code("s").unwrap(), CONTROL, None),
        None,
        "the release after a restart must not synthesise a stray command"
    );
}

#[test]
fn key_and_modifier_names_accept_karabiners_spelling() {
    assert_eq!(key_code("End"), key_code("end"));
    assert_eq!(key_code("page-up"), key_code("page_up"));
    assert_eq!(key_code("delete_or_backspace"), key_code("backspace"));
    assert_eq!(key_code("return_or_enter"), key_code("enter"));
    assert!(key_code("f7").is_some());
    assert!(key_code("unknown").is_none());
    assert_eq!(modifier_flag("left_control"), modifier_flag("control"));
    assert_eq!(modifier_flag("left_command"), modifier_flag("command"));
    assert_eq!(modifier_flag("left_option"), modifier_flag("option"));
    assert_eq!(modifier_flag("left_shift"), modifier_flag("shift"));
    assert!(modifier_flag("any").is_none());
}

/// The rules migrated from `~/.config/karabiner/karabiner.json`. Keeping them
/// here means the migration stays verifiable: each one is asserted on the
/// combination it was written for.
#[test]
fn the_migrated_karabiner_rules_map_the_same_way() {
    let terminals = [
        "com.apple.Terminal",
        "com.googlecode.iterm2",
        "net.kovidgoyal.kitty",
    ];
    let remaps = vec![
        excluding(
            with_extra(rule("save", "s", &["left_control"], "s", &["left_command"])),
            &terminals,
        ),
        excluding(
            with_extra(rule(
                "close",
                "w",
                &["left_control"],
                "w",
                &["left_command"],
            )),
            &terminals,
        ),
        excluding(
            with_extra(rule("new-tab", "t", &["left_control"], "t", &["command"])),
            &terminals,
        ),
        with_extra(rule("undo", "z", &["left_control"], "z", &["command"])),
        excluding(
            rule("end-of-line", "end", &[], "right_arrow", &["command"]),
            &terminals,
        ),
        with_extra(rule("page-up", "u", &["left_command"], "page_up", &[])),
        with_extra(rule("page-down", "i", &["left_command"], "page_down", &[])),
        with_extra(rule("left", "h", &["left_command"], "left_arrow", &[])),
        with_extra(rule("down", "j", &["left_command"], "down_arrow", &[])),
        with_extra(rule("up", "k", &["left_command"], "up_arrow", &[])),
        with_extra(rule("up-alt", "p", &["left_command"], "up_arrow", &[])),
        with_extra(rule("right", "l", &["left_command"], "right_arrow", &[])),
        with_extra(rule(
            "delete-word",
            "3",
            &["left_command"],
            "delete_or_backspace",
            &["option"],
        )),
        with_extra(rule(
            "top",
            "1",
            &["left_command"],
            "up_arrow",
            &["command"],
        )),
        with_extra(rule(
            "bottom",
            "2",
            &["left_command"],
            "down_arrow",
            &["command"],
        )),
        rule("f7", "f7", &[], "7", &["control"]),
    ];
    validate(&remaps).expect("the migrated rules must be valid");
    let mut remapper = KeyRemapper::new(&remaps);

    let cases: [(&str, u64, &str, u64); 8] = [
        ("s", CONTROL, "s", COMMAND),
        ("w", CONTROL, "w", COMMAND),
        ("t", CONTROL, "t", COMMAND),
        ("z", CONTROL, "z", COMMAND),
        ("h", COMMAND, "left_arrow", 0),
        ("3", COMMAND, "delete_or_backspace", OPTION),
        ("2", COMMAND, "down_arrow", COMMAND),
        ("f7", 0, "7", CONTROL),
    ];
    for (from, from_flags, to, to_flags) in cases {
        let (key, flags) = remapper
            .rewrite(
                KEY_DOWN,
                key_code(from).unwrap(),
                from_flags,
                Some("com.microsoft.VSCode"),
            )
            .unwrap_or_else(|| panic!("{from} must be remapped"));
        assert_eq!(key, key_code(to).unwrap(), "{from} maps to {to}");
        assert_eq!(modifiers(flags), to_flags, "{from} carries {to_flags:#x}");
        remapper.rewrite(KEY_UP, key_code(from).unwrap(), from_flags, None);
    }
    assert_eq!(
        remapper.rewrite(
            KEY_DOWN,
            key_code("end").unwrap(),
            0,
            Some("com.apple.Terminal")
        ),
        None,
        "End keeps its native meaning in a terminal"
    );
    assert_eq!(
        remapper.rewrite(KEY_DOWN, key_code("u").unwrap(), CONTROL, None),
        None,
        "Ctrl+U is not part of the migration"
    );
}

#[test]
fn an_unknown_frontmost_application_keeps_excluded_rules_off() {
    let mut remapper = KeyRemapper::new(&[excluding(
        with_extra(rule("save", "s", &["control"], "s", &["command"])),
        &["com.apple.Terminal"],
    )]);
    assert_eq!(
        remapper.rewrite(KEY_DOWN, key_code("s").unwrap(), CONTROL, None),
        None,
        "until the tap knows what is in front, an exclusion list keeps the rule off"
    );
    assert!(
        remapper
            .rewrite(
                KEY_DOWN,
                key_code("s").unwrap(),
                CONTROL,
                Some("dev.warp.Warp-Stable")
            )
            .is_some()
    );

    // A rule without exclusions does not depend on knowing the frontmost app.
    let mut plain = KeyRemapper::new(&[with_extra(rule(
        "save",
        "s",
        &["control"],
        "s",
        &["command"],
    ))]);
    assert!(
        plain
            .rewrite(KEY_DOWN, key_code("s").unwrap(), CONTROL, None)
            .is_some()
    );
}
