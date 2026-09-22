use super::*;

pub fn verify_rules_editor(
    window: &AliasRulesEditor,
    settings: &crate::macos::ui::settings::SettingsWindow,
    target: &AnyObject,
    mtm: MainThreadMarker,
    saved: impl Fn() -> Config,
) {
    let host = &settings.window;
    assert_eq!(window.view().window().as_ref(), Some(host));
    window.add(target, mtm);
    assert!(window.candidate().unwrap().is_empty());
    let first = window.rows.borrow()[0].clone();
    assert_eq!(window.selected.get(), Some(0));
    assert!(!first.view.isHidden());
    first.set_application(ApplicationTarget {
        bundle_id: "com.example.editor".into(),
        path: "/Applications/Example Editor.app".into(),
        name: "Editor".into(),
    });
    first.alias.setStringValue(ns_string!("ck"));
    first.title.setStringValue(ns_string!("ckb"));
    first.notify_changed();
    assert_eq!(saved().alias_rules, window.candidate().unwrap());
    let valid = saved();
    first.alias.setStringValue(ns_string!("ABC"));
    first.notify_changed();
    assert_eq!(
        saved(),
        valid,
        "invalid drafts must not replace saved rules"
    );
    assert!(window.candidate().is_err());
    first.alias.setStringValue(ns_string!("ck"));
    window.add(target, mtm);
    let second = window.rows.borrow()[1].clone();
    assert!(first.view.isHidden());
    assert!(!second.view.isHidden());
    second.set_application(first.application.borrow().clone().unwrap());
    second.alias.setStringValue(ns_string!("ck"));
    second.title.setStringValue(ns_string!("rust"));
    second.notify_changed();
    assert_eq!(
        saved(),
        valid,
        "duplicate aliases must not replace saved rules"
    );
    second.alias.setStringValue(ns_string!("ru"));
    second.notify_changed();
    assert_eq!(saved().alias_rules.len(), 2);
    window.add(target, mtm);
    first.title.setStringValue(ns_string!(" ckb project "));
    first.notify_changed();
    assert_eq!(
        saved().alias_rules.len(),
        2,
        "incomplete new rows remain drafts"
    );
    assert_eq!(saved().alias_rules[0].title_contains, "ckb project");
    let translated = AliasRulesEditor::new(target, mtm);
    translated.restore_draft(window.draft(), target, mtm);
    assert_eq!(translated.draft(), window.draft());
    assert_eq!(translated.rows.borrow().len(), 3);
    assert_eq!(translated.selected.get(), window.selected.get());
    assert_eq!(translated.candidate().unwrap(), window.candidate().unwrap());
    // Switching rules commits the active field before hiding its editor.
    window.select(0);
    unsafe { first.title.selectText(None) };
    let editor = host
        .firstResponder()
        .unwrap()
        .downcast::<NSTextView>()
        .unwrap();
    editor.setString(ns_string!("ckb project edited"));
    unsafe {
        second.navigation.sendAction_to(
            second.navigation.action(),
            second.navigation.target().as_deref(),
        );
    }
    assert_eq!(window.selected.get(), Some(1));
    assert!(first.view.isHidden());
    assert_eq!(saved().alias_rules[0].title_contains, "ckb project edited");
    window.select(0);
    first.alias.setStringValue(ns_string!("ABC"));
    window.select(1);
    window.select(0);
    assert_eq!(
        first.alias.stringValue().to_string(),
        "ABC",
        "invalid drafts survive selection"
    );
    first.alias.setStringValue(ns_string!("ck"));
    unsafe {
        assert!(
            first
                .remove
                .sendAction_to(first.remove.action(), first.remove.target().as_deref())
        );
    }
    assert_eq!(saved().alias_rules.len(), 1);
    assert_eq!(saved().alias_rules[0].alias, "ru");
    assert_eq!(window.rows.borrow()[0].remove.tag(), 0);
    // A row can bind a Winlane command instead of an app.
    window.add(target, mtm);
    let command_row = window.rows.borrow().last().unwrap().clone();
    command_row.alias.setStringValue(ns_string!("cl"));
    command_row.set_kind_command(true);
    command_row.select_command(CommandId::Clipboard);
    command_row.notify_changed();
    assert!(command_row.is_command());
    assert!(command_row.choose.isHidden() && command_row.title.isHidden());
    assert!(!command_row.command.isHidden());
    let saved_command = saved()
        .alias_rules
        .into_iter()
        .find(|rule| rule.alias == "cl")
        .expect("command alias saves");
    assert_eq!(saved_command.command, Some(CommandId::Clipboard));
    assert_eq!(saved_command.application, None);
    // The command target survives a draft round-trip.
    let round_trip = AliasRulesEditor::new(target, mtm);
    round_trip.restore_draft(window.draft(), target, mtm);
    assert_eq!(round_trip.draft(), window.draft());
    for view in command_row.view.subviews().iter() {
        let frame = view.frame();
        assert!(frame.origin.x >= 0.0 && frame.origin.y >= 0.0);
        assert!(
            frame.origin.x + frame.size.width <= command_row.view.frame().size.width
                && frame.origin.y + frame.size.height <= command_row.view.frame().size.height
        );
    }
    // Switching back to an app clears the command binding.
    command_row.set_kind_command(false);
    command_row.notify_changed();
    assert!(
        saved()
            .alias_rules
            .iter()
            .all(|rule| rule.command.is_none())
    );
    unsafe {
        command_row.remove.sendAction_to(
            command_row.remove.action(),
            command_row.remove.target().as_deref(),
        );
    }
    assert_eq!(saved().alias_rules.len(), 1);
    for row in window.rows.borrow().iter() {
        assert!(row.alias.cell().unwrap().sendsActionOnEndEditing());
        assert!(row.title.cell().unwrap().sendsActionOnEndEditing());
        for view in row.view.subviews().iter() {
            assert!(view.frame().origin.x >= 0.0 && view.frame().origin.y >= 0.0);
            assert!(view.frame().origin.x + view.frame().size.width <= row.view.frame().size.width);
            assert!(
                view.frame().origin.y + view.frame().size.height <= row.view.frame().size.height
            );
        }
    }
    if let Ok(dir) = std::env::var("WINLANE_PREVIEW_DIR") {
        let root = window.view();
        // A solid backing makes offscreen AppKit text readable in the preview.
        let appearance = unsafe { NSAppearance::appearanceNamed(NSAppearanceNameAqua) };
        host.setAppearance(appearance.as_deref());
        let backing = NSBox::initWithFrame(NSBox::alloc(mtm), root.bounds());
        backing.setBoxType(NSBoxType::Custom);
        backing.setFillColor(&NSColor::whiteColor());
        backing.setBorderWidth(0.0);
        root.addSubview_positioned_relativeTo(&backing, NSWindowOrderingMode::Below, None);
        root.layoutSubtreeIfNeeded();
        let bitmap = root
            .bitmapImageRepForCachingDisplayInRect(root.bounds())
            .unwrap();
        root.cacheDisplayInRect_toBitmapImageRep(root.bounds(), &bitmap);
        let png = unsafe {
            bitmap.representationUsingType_properties(
                NSBitmapImageFileType::PNG,
                &objc2_foundation::NSDictionary::new(),
            )
        }
        .unwrap();
        assert!(
            png.writeToFile_atomically(
                &NSString::from_str(&format!("{dir}/alias-rules.png")),
                true
            )
        );
    }
    window.select(0);
    let selected = window.rows.borrow()[0].clone();
    unsafe { selected.title.selectText(None) };
    let field_editor = host
        .firstResponder()
        .unwrap()
        .downcast::<NSTextView>()
        .unwrap();
    field_editor.setString(ns_string!("saved on tab change"));
    settings.select_tab(4);
    assert_eq!(saved().alias_rules[0].title_contains, "saved on tab change");
    settings.select_tab(5);
    assert_eq!(
        window.rows.borrow()[0].title.stringValue().to_string(),
        "saved on tab change"
    );
    assert!(!host.isVisible());
}
