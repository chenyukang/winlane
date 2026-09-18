pub fn verify_editor(target: &AnyObject, mtm: MainThreadMarker) {
    use std::rc::Rc;
    let stored = Rc::new(RefCell::new(Vec::new()));
    let saved = stored.clone();
    let windows_before = NSApplication::sharedApplication(mtm).windows().len();
    let editor = SnippetEditor::new(Vec::new(), Box::new(move |snippets| { saved.replace(snippets); Ok(()) }), mtm);
    assert_eq!(NSApplication::sharedApplication(mtm).windows().len(), windows_before, "the snippet editor must not create a window");
    let settings = crate::settings::SettingsWindow::new(target, mtm);
    let host = &settings.window;
    settings.embed_snippets(editor.view());
    settings.select_tab(6);
    assert_eq!(editor.view().window().unwrap(), *host);
    let size = host.contentView().unwrap().frame().size;
    for child in editor.view().subviews() {
        let frame = child.frame();
        assert!(frame.origin.x >= 0.0 && frame.origin.y >= 0.0);
        assert!(frame.origin.x + frame.size.width <= editor.view().bounds().size.width + 1.0, "editor control too wide: {frame:?}");
        assert!(frame.origin.y + frame.size.height <= editor.view().bounds().size.height + 1.0, "editor control too tall: {frame:?}");
    }
    assert!(!editor.ui().name.isEnabled());
    editor.add(sel!(addSnippet:), None);
    assert!(stored.borrow().is_empty());
    editor.ui().name.setStringValue(ns_string!("Greeting"));
    editor.ui().body.setString(ns_string!("Hello {argument name=\"Name\"}, today is {date}.\n{clipboard}"));
    editor.changed();
    assert_eq!(stored.borrow().len(), 1);
    let initial = stored.borrow().clone();
    settings.select_tab(0);
    assert_eq!(host.contentView().unwrap().frame().size, size);
    settings.select_tab(6);
    assert_eq!(editor.ui().name.stringValue().to_string(), "Greeting");
    assert_eq!(editor.ui().body.string().to_string(), initial[0].body);
    assert_eq!(editor.view().window().unwrap(), *host);

    assert!(editor.ui().preview.string().to_string().contains("⟨Name⟩"));
    assert_eq!(editor.ivars().selected.get(), Some(0));
    editor.ui().body.setString(ns_string!("{argument name=bad}"));
    editor.changed();
    assert_eq!(*stored.borrow(), initial, "invalid draft must retain the last saved body");
    editor.add(sel!(addSnippet:), None);
    editor.ui().name.setStringValue(ns_string!("Second"));
    editor.ui().body.setString(ns_string!("Another snippet"));
    editor.changed();
    assert_eq!(stored.borrow().len(), 2, "invalid draft must not block saving other snippets");
    editor.delete(sel!(deleteSnippet:), None);
    assert_eq!(stored.borrow().len(), 1);
    editor.ui().body.setString(&NSString::from_str(&initial[0].body));
    editor.changed();
    let duplicate = SnippetEditor::new(stored.borrow().clone(), Box::new(|_| Ok(())), mtm);
    assert_eq!(duplicate.ui().body.string().to_string(), initial[0].body);
    editor.ui().body.setSelectedRange(objc2_foundation::NSRange::new(0, 0));
    editor.ui().placeholder.selectItemAtIndex(1);
    editor.insert_placeholder(sel!(insertPlaceholder:), None);
    assert!(stored.borrow()[0].body.starts_with("{clipboard}"));
    assert_eq!(editor.ui().placeholder.indexOfSelectedItem(), 0);
    assert!(!editor.ui().body.isAutomaticQuoteSubstitutionEnabled());
    let body_before_builder = editor.ui().body.string().to_string();
    let windows_before_builder = NSApplication::sharedApplication(mtm).windows().len();
    editor.ui().body.setSelectedRange(objc2_foundation::NSRange::new(0, 0));
    editor.ui().placeholder.selectItemAtIndex(2);
    editor.insert_placeholder(sel!(insertPlaceholder:), None);
    assert!(!editor.ui().builder.view.isHidden());
    crate::snippet_placeholder::verify_dates(&editor.ui().builder);
    editor.cancel_placeholder(sel!(cancelSnippetPlaceholder:), None);
    assert!(editor.ui().builder.view.isHidden());
    assert_eq!(editor.ui().body.string().to_string(), body_before_builder);
    editor.ui().placeholder.selectItemAtIndex(3);
    editor.insert_placeholder(sel!(insertPlaceholder:), None);
    let token = crate::snippet_placeholder::verify_fields(&editor.ui().builder);
    editor.commit_placeholder(sel!(commitSnippetPlaceholder:), None);
    assert!(editor.ui().builder.view.isHidden());
    assert_eq!(stored.borrow()[0].body, format!("{token}{body_before_builder}"));
    assert_eq!(NSApplication::sharedApplication(mtm).windows().len(), windows_before_builder);
    verify_typed_arguments(mtm);
    assert!(!&host.isVisible());

    let result = Rc::new(RefCell::new(None));
    let output = result.clone();
    let form = SnippetArguments::new("Greeting", Template::parse("Hi {argument name=\"Name\"}! {clipboard} {argument name=\"Name\"}").unwrap(), "copied text".into(), Box::new(move |text| { output.replace(Some(text)); Ok(()) }), mtm);
    assert_eq!(form.ivars().fields.borrow().len(), 1);
    assert!(!form.ivars().confirm.get().unwrap().isEnabled());
    let ArgumentControl::Text(field) = &form.ivars().fields.borrow()[0] else { panic!("expected a text field") };
    field.setStringValue(ns_string!("Ada {date}"));
    form.update();
    assert!(form.ivars().confirm.get().unwrap().isEnabled());
    assert_eq!(form.value().unwrap(), "Hi Ada {date}! copied text Ada {date}");
    crate::settings::verify_escape_close(form.window());
    form.cancel(sel!(cancelSnippet:), None);
    assert!(result.borrow().is_none(), "cancel must not paste");
    form.paste(sel!(pasteSnippet:), None);
    assert_eq!(result.borrow().as_deref(), Some("Hi Ada {date}! copied text Ada {date}"));
    crate::settings::verify_escape_close(host);
    if let Ok(directory) = std::env::var("WINLANE_PREVIEW_DIR") {
        for (name, window) in [("snippet-editor", &**host), ("snippet-arguments", form.window())] {
            let root = window.contentView().unwrap();
            root.layoutSubtreeIfNeeded();
            let bitmap = root.bitmapImageRepForCachingDisplayInRect(root.bounds()).unwrap();
            root.cacheDisplayInRect_toBitmapImageRep(root.bounds(), &bitmap);
            let png = unsafe { bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &objc2_foundation::NSDictionary::new()) }.unwrap();
            std::fs::create_dir_all(&directory).unwrap();
            assert!(png.writeToFile_atomically(&NSString::from_str(&format!("{directory}/{name}.png")), true));
        }
    }
    assert!(!&host.isVisible());
    println!("Snippet editor, autosave, invalid drafts, repeated arguments, preview and Escape checks passed.");
}

fn verify_typed_arguments(mtm: MainThreadMarker) {
    let template = Template::parse(r#"{argument name="Notes" type="multiline" required="true" default="Line 1\nLine 2"} / {argument name="Tone" type="choice" options="Friendly\nFormal"} / {argument name="Extra" required="false" default="Optional"}"#).unwrap();
    let form = SnippetArguments::new("Typed fields", template, String::new(), Box::new(|_| panic!("no real paste in native tests")), mtm);
    assert!(!form.ivars().confirm.get().unwrap().isEnabled());
    assert!(form.ivars().preview.get().unwrap().string().to_string().contains("⟨Tone⟩"));
    assert_eq!(form.ivars().confirm.get().unwrap().keyEquivalentModifierMask(), NSEventModifierFlags::Command);
    {
        let fields = form.ivars().fields.borrow();
        let ArgumentControl::Multiline(notes) = &fields[0] else { panic!("expected multiline") };
        assert_eq!(notes.string().to_string(), "Line 1\nLine 2");
        notes.setString(ns_string!("first\nsecond"));
        let ArgumentControl::Choice { popup, .. } = &fields[1] else { panic!("expected dropdown") };
        popup.selectItemAtIndex(2);
        let ArgumentControl::Text(extra) = &fields[2] else { panic!("expected optional text") };
        extra.setStringValue(ns_string!(""));
    }
    form.choice_changed(sel!(snippetArgumentChanged:), None);
    assert!(form.ivars().confirm.get().unwrap().isEnabled());
    assert_eq!(form.value().unwrap(), "first\nsecond / Formal / ");
    let tokens: Vec<_> = (0..8).map(|index| format!("{{argument name=\"Field {index}\" type=\"multiline\"}}")).collect();
    let large = SnippetArguments::new("Large form", Template::parse(&tokens.join("\n")).unwrap(), String::new(), Box::new(|_| unreachable!()), mtm);
    assert_eq!(large.window().contentView().unwrap().frame().size.height, 600.0);
    let root = large.window().contentView().unwrap();
    let scroll = root.subviews().iter().find_map(|view| view.downcast::<NSScrollView>().ok().filter(|view| view.frame().origin.y == 236.0)).unwrap();
    assert_eq!(scroll.documentView().unwrap().frame().size.height, 704.0);
    assert!(scroll.contentView().bounds().origin.y > 0.0);
    if let Ok(directory) = std::env::var("WINLANE_PREVIEW_DIR") {
        let root = form.window().contentView().unwrap();
        let bitmap = root.bitmapImageRepForCachingDisplayInRect(root.bounds()).unwrap();
        root.cacheDisplayInRect_toBitmapImageRep(root.bounds(), &bitmap);
        let png = unsafe { bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &objc2_foundation::NSDictionary::new()) }.unwrap();
        std::fs::create_dir_all(&directory).unwrap();
        assert!(png.writeToFile_atomically(&NSString::from_str(&format!("{directory}/snippet-typed-fields.png")), true));
    }
}
