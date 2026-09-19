use super::*;

pub fn verify_rules_editor(
    window: &AliasRulesWindow,
    target: &AnyObject,
    mtm: MainThreadMarker,
    saved: impl Fn() -> Config,
) {
    window.add(target, mtm);
    assert!(window.candidate().unwrap().is_empty());
    let first = window.rows.borrow()[0].clone();
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
    let translated = AliasRulesWindow::new(target, mtm);
    translated.copy_draft_from(window, target, mtm);
    assert_eq!(translated.rows.borrow().len(), 3);
    assert_eq!(translated.candidate().unwrap(), window.candidate().unwrap());
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
    for row in window.rows.borrow().iter() {
        assert!(row.alias.cell().unwrap().sendsActionOnEndEditing());
        assert!(row.title.cell().unwrap().sendsActionOnEndEditing());
        for view in row.view.subviews().iter() {
            assert!(view.frame().origin.x >= 0.0 && view.frame().origin.y >= 0.0);
            assert!(view.frame().origin.x + view.frame().size.width <= row.view.frame().size.width);
        }
    }
    if let Ok(dir) = std::env::var("WINLANE_PREVIEW_DIR") {
        let root = window.window.contentView().unwrap();
        // A solid backing makes offscreen AppKit text readable in the preview.
        let appearance = unsafe { NSAppearance::appearanceNamed(NSAppearanceNameAqua) };
        window.window.setAppearance(appearance.as_deref());
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
    let row = window.rows.borrow()[0].clone();
    // SAFETY: The retained field belongs to this hidden native window.
    unsafe { row.title.selectText(None) };
    let editor = window
        .window
        .firstResponder()
        .unwrap()
        .downcast::<NSTextView>()
        .unwrap();
    editor.setString(ns_string!("compiler"));
    window
        .window
        .sendEvent(&crate::macos::ui::settings::tests::escape_event(
            &window.window,
            NSEventModifierFlags::empty(),
        ));
    assert_eq!(
        saved().alias_rules[0].title_contains,
        "compiler",
        "Escape must finish and save the active rule edit"
    );
    assert!(!window.window.isVisible());
}
