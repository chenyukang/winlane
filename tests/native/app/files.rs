use super::*;
use objc2::ClassType;
use objc2_foundation::NSDictionary;

pub(crate) fn verify_files(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let app = delegate.ivars();
    app.catalog_checked.set(Some(Instant::now()));
    app.mode.set(Some(PanelMode::Search));
    delegate.sync_displays();
    app.query.replace("files".into());
    delegate.filter();
    assert_eq!(delegate.selected_command(), Some(CommandId::Files));
    assert!(
        app.files.borrow().service.is_none(),
        "matching a command does no disk access"
    );
    let home = home();
    let entries: Vec<_> = ["Report.pdf", "Report-final.pdf"]
        .iter()
        .map(|name| Entry {
            path: home.join("Documents").join(name),
            name: name.to_string(),
            directory: false,
            modified: 1,
        })
        .collect();
    app.files.borrow_mut().recent = entries.clone();
    delegate.activate_selected();
    assert!(delegate.searching_files());
    assert_eq!(
        delegate.match_count(),
        2,
        "cached rows render before the worker starts"
    );
    assert!(app.files.borrow().service.is_none());
    assert!(app.files.borrow().loading);
    assert!(app.files.borrow().timer.is_some());
    let ui = delegate.panels()[0].clone();
    assert!(!ui.project_progress.isHidden());
    assert!(
        ui.rows
            .borrow()
            .iter()
            .all(|row| matches!(row.content, Some(RowContent::File(_))))
    );
    let rows = ui.rows.borrow();
    assert_eq!(rows[0].app.stringValue().to_string(), "Report.pdf");
    assert_eq!(rows[0].title.stringValue().to_string(), "~/Documents");
    assert!(
        (rows[0].app.frame().origin.y + rows[0].app.frame().size.height)
            <= rows[0].title.frame().origin.y
            || (rows[0].title.frame().origin.y + rows[0].title.frame().size.height)
                <= rows[0].app.frame().origin.y,
        "name and path occupy separate lines"
    );
    drop(rows);
    app.query.replace("report".into());
    delegate.filter();
    let generation = app.files.borrow().generation;
    delegate.apply_files_update(Update::Results {
        generation,
        entries: entries.clone(),
        gathering: false,
        limited: false,
        error: None,
    });
    assert_eq!(delegate.selected_file(), Some(entries[0].clone()));
    delegate.apply_files_update(Update::Results {
        generation,
        entries: Vec::new(),
        gathering: true,
        limited: false,
        error: None,
    });
    assert_eq!(
        delegate.match_count(),
        2,
        "initial empty Spotlight progress must not flash away the cache"
    );
    delegate.move_selection(1);
    delegate.apply_files_update(Update::Results {
        generation,
        entries: entries.iter().rev().cloned().collect(),
        gathering: false,
        limited: false,
        error: None,
    });
    assert_eq!(
        delegate.selected_file(),
        Some(entries[1].clone()),
        "async updates preserve selected path"
    );
    delegate.apply_files_update(Update::Results {
        generation: generation.wrapping_sub(1),
        entries: Vec::new(),
        gathering: false,
        limited: false,
        error: None,
    });
    assert_eq!(
        delegate.match_count(),
        2,
        "stale queries cannot erase current results"
    );
    delegate.apply_files_update(Update::Results {
        generation,
        entries: Vec::new(),
        gathering: false,
        limited: false,
        error: Some("Fixture read failure".into()),
    });
    assert_eq!(
        delegate.match_count(),
        2,
        "a transient error retains usable cache"
    );
    assert!(
        ui.footer
            .stringValue()
            .to_string()
            .contains("Fixture read failure")
    );
    assert!(ui.project_progress.isHidden() != app.files.borrow().loading);
    verify_completion(&delegate, &ui);
    verify_navigation(&delegate, &ui);
    verify_patterns_and_actions(&delegate, &ui);

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("preview.txt");
    std::fs::write(&path, "Winlane file preview fixture").unwrap();
    let entry = Entry::read(path).unwrap();
    delegate.toggle_file_preview(&entry);
    assert!(ui.file_preview.borrow().is_some());
    assert!(ui.scroll.isHidden());
    assert!(!ui.panel.isVisible());
    delegate.toggle_file_preview(&entry);
    assert!(ui.file_preview.borrow().is_none());
    assert!(!ui.scroll.isHidden());

    app.query.borrow_mut().clear();
    delegate.filter();
    let editor = NSTextView::initWithFrame(NSTextView::alloc(mtm), NSRect::ZERO);
    let handled: bool = unsafe {
        msg_send![&delegate, control: &*ui.input, textView: &*editor, doCommandBySelector: sel!(deleteBackward:)]
    };
    assert!(!handled);
    assert!(delegate.searching_files());
    delegate.leave_scoped_search();
    assert_eq!(delegate.selected_command(), Some(CommandId::Files));
    assert!(app.files.borrow().timer.is_none());
    assert!(app.files.borrow().matches.is_empty());
    delegate.activate_selected();
    delegate.end_session();
    let before = app.render_passes.get();
    delegate.apply_files_update(Update::Results {
        generation,
        entries,
        gathering: false,
        limited: false,
        error: None,
    });
    assert_eq!(app.render_passes.get(), before);
    assert!(app.mode.get().is_none());
    assert!(ui.rows.borrow().is_empty());
    assert!(app.files.borrow().timer.is_none());
    verify_worker(&delegate);
    println!(
        "Files: immediate cached rows, async selection, stale results, error retention, two-line layout, Tab completion, inline Quick Look, empty Backspace and dismissal."
    );
}

fn verify_completion(delegate: &Delegate, ui: &PanelUi) {
    use objc2_foundation::{NSNotFound, NSRange};
    let app = delegate.ivars();
    delegate.cancel_input_start();
    assert!(ui.panel.makeFirstResponder(Some(&ui.input)));
    let editor = ui
        .input
        .currentEditor()
        .unwrap()
        .downcast::<NSTextView>()
        .unwrap();
    let source = Source::current(delegate.mtm()).and_then(|s| s.id());
    let key = |flags, repeat| {
        NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::KeyDown, NSPoint::ZERO, flags, 0.0, ui.panel.windowNumber(),
            None, ns_string!("\t"), ns_string!("\t"), repeat, 48,
        ).unwrap()
    };
    let tab = key(NSEventModifierFlags::empty(), false);
    let folder = Entry {
        path: home().join("Documents/工作资料 📂"),
        name: "工作资料 📂".into(),
        directory: true,
        modified: 1,
    };
    let file = Entry {
        path: folder.path.join("report 2026.txt"),
        name: "report 2026.txt".into(),
        directory: false,
        modified: 1,
    };
    app.files.borrow_mut().entries = vec![folder.clone(), file.clone()];
    app.query.replace("工资".into());
    delegate.filter();
    assert_eq!(delegate.selected_file(), Some(folder.clone()));
    ui.panel.sendEvent(&tab);
    let text = folder.completion(&home());
    assert_eq!(*app.query.borrow(), text);
    assert_eq!(editor.string().to_string(), text);
    assert_eq!(
        NSTextInputClient::selectedRange(&*editor),
        NSRange::new(text.encode_utf16().count(), 0)
    );
    assert_eq!(delegate.selected_file(), Some(file.clone()));
    assert!(app.files.borrow().loading);
    assert!(
        app.files.borrow().service.is_none(),
        "completion only schedules disk access"
    );
    assert!(
        ui.input
            .currentEditor()
            .is_some_and(|e| std::ptr::eq(&*e, editor.as_super()))
    );
    assert_eq!(Source::current(delegate.mtm()).and_then(|s| s.id()), source);
    assert!(delegate.handle_files_key(&key(NSEventModifierFlags::empty(), true), false));
    assert_eq!(
        *app.query.borrow(),
        text,
        "holding Tab must not descend repeatedly"
    );

    let handled: bool = unsafe {
        msg_send![delegate, control: &*ui.input, textView: &*editor, doCommandBySelector: sel!(insertTab:)]
    };
    assert!(handled);
    assert_eq!(*app.query.borrow(), file.completion(&home()));
    assert_eq!(delegate.selected_file(), Some(file));
    assert!(delegate.searching_files());
    assert!(app.files.borrow().opening.is_none());
    assert!(!delegate.handle_files_key(&key(NSEventModifierFlags::Shift, false), false));
    assert!(!delegate.handle_files_key(&key(NSEventModifierFlags::Command, false), false));

    app.query.replace("no matching file".into());
    delegate.filter();
    ui.panel.sendEvent(&tab);
    assert_eq!(*app.query.borrow(), "no matching file");
    assert!(delegate.searching_files());

    unsafe {
        NSTextInputClient::setMarkedText_selectedRange_replacementRange(
            &*editor,
            ns_string!("gong"),
            NSRange::new(4, 0),
            NSRange::new(NSNotFound as usize, 0),
        );
    }
    assert!(NSTextInputClient::hasMarkedText(&*editor));
    assert!(!delegate.handle_files_key(&tab, true));
    let handled: bool = unsafe {
        msg_send![delegate, control: &*ui.input, textView: &*editor, doCommandBySelector: sel!(insertTab:)]
    };
    assert!(!handled);
    assert!(NSTextInputClient::hasMarkedText(&*editor));
    NSTextInputClient::unmarkText(&*editor);
    ui.panel.makeFirstResponder(None);
    app.search_scope.set(None);
    assert!(!delegate.handle_files_key(&tab, false));
    assert!(!delegate.complete_selected_file());
    app.search_scope.set(Some(SearchScope::Files));
    assert!(!ui.panel.isVisible());
}

fn verify_navigation(delegate: &Delegate, ui: &PanelUi) {
    use objc2_foundation::NSRange;
    let app = delegate.ivars();
    delegate.cancel_input_start();
    assert!(ui.panel.makeFirstResponder(Some(&ui.input)));
    let editor = ui
        .input
        .currentEditor()
        .unwrap()
        .downcast::<NSTextView>()
        .unwrap();
    let folder = Entry {
        path: home().join("Downloads"),
        name: "Downloads".into(),
        directory: true,
        modified: 1,
    };
    let key = |code, flags, chars: &str| {
        let chars = NSString::from_str(chars);
        NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::KeyDown, NSPoint::ZERO, flags, 0.0, ui.panel.windowNumber(),
            None, &chars, &chars, false, code,
        ).unwrap()
    };
    let control = NSEventModifierFlags::Control;
    for code in [36, 76] {
        app.files.borrow_mut().entries = vec![folder.clone()];
        app.query.replace("dwn".into());
        delegate.filter();
        ui.panel
            .sendEvent(&key(code, NSEventModifierFlags::empty(), "\r"));
        assert_eq!(*app.query.borrow(), "~/Downloads/");
        assert!(delegate.searching_files());
        assert!(app.files.borrow().opening.is_none());
        assert!(
            app.files.borrow().service.is_none(),
            "Enter defers browsing to the worker"
        );
        assert!(delegate.is_files_open_key(&key(code, control, "\r"), false));
        assert!(!delegate.is_files_open_key(&key(code, control, "\r"), true));
        assert!(!delegate.is_files_open_key(
            &key(code, control | NSEventModifierFlags::Shift, "\r"),
            false
        ));
        let repeat = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::KeyDown, NSPoint::ZERO, NSEventModifierFlags::empty(), 0.0,
            ui.panel.windowNumber(), None, ns_string!("\r"), ns_string!("\r"), true, code,
        ).unwrap();
        assert!(delegate.handle_files_key(&repeat, false));
        assert_eq!(*app.query.borrow(), "~/Downloads/");
    }
    app.query.replace("dwn".into());
    delegate.filter();
    let handled: bool = unsafe {
        msg_send![delegate, control: &*ui.input, textView: &*editor, doCommandBySelector: sel!(insertNewline:)]
    };
    assert!(handled);
    assert_eq!(*app.query.borrow(), "~/Downloads/");
    assert!(app.files.borrow().opening.is_none());

    let back = key(51, control, "\u{8}");
    for (query, expected) in [
        ("~/Downloads/", "~/"),
        ("~/Downloads/报告", "~/Downloads/"),
        ("~/工作 📂/报告/", "~/工作 📂/"),
        ("~/", "~/"),
        ("/", "/"),
    ] {
        app.query.replace(query.into());
        delegate.filter();
        ui.panel.sendEvent(&back);
        assert_eq!(*app.query.borrow(), expected);
        assert_eq!(editor.string().to_string(), expected);
        assert_eq!(
            NSTextInputClient::selectedRange(&*editor),
            NSRange::new(expected.encode_utf16().count(), 0)
        );
        assert!(delegate.searching_files());
    }
    app.query.replace("~/Downloads/".into());
    delegate.filter();
    assert!(!delegate.handle_files_key(&back, true));
    assert_eq!(*app.query.borrow(), "~/Downloads/");
    assert!(!delegate.handle_files_key(&key(51, NSEventModifierFlags::empty(), "\u{8}"), false));
    app.query.replace("no match".into());
    delegate.filter();
    assert!(
        !delegate.handle_files_key(&back, false),
        "plain queries keep native text deletion"
    );
    assert!(delegate.handle_files_key(&key(36, control, "\r"), false));
    assert!(
        app.files.borrow().opening.is_none(),
        "Ctrl-Enter without a selection does nothing"
    );
    ui.panel.makeFirstResponder(None);
    assert!(!ui.panel.isVisible());
}

fn verify_patterns_and_actions(delegate: &Delegate, ui: &PanelUi) {
    let app = delegate.ivars();
    let entries: Vec<_> = [
        ("report[12].pdf", false),
        ("report1.pdf", false),
        ("Documents", true),
    ]
    .into_iter()
    .map(|(name, directory)| Entry {
        path: home().join(name),
        name: name.into(),
        directory,
        modified: 1,
    })
    .collect();
    app.files.borrow_mut().entries = entries.clone();
    app.query.replace("report[12].pdf".into());
    delegate.filter();
    assert_eq!(
        delegate.selected_file(),
        Some(entries[0].clone()),
        "cached literal matches are usable before compiling a pattern"
    );
    assert!(app.files.borrow().loading);
    assert_eq!(
        ui.file_controls.view.subviews().len(),
        1,
        "only the actions button remains"
    );
    assert!(!ui.file_controls.view.isHidden());
    assert!(ui.file_controls.actions.isEnabled());
    assert!(
        ui.input.frame().origin.x + ui.input.frame().size.width
            <= ui.file_controls.view.frame().origin.x
    );
    let generation = app.files.borrow().generation;
    delegate.apply_files_update(Update::Results {
        generation,
        entries: entries[..2].to_vec(),
        gathering: false,
        limited: false,
        error: None,
    });
    assert_eq!(
        app.files.borrow().matches,
        entries[..2],
        "worker relevance order survives rendering"
    );
    let menu = delegate.file_actions_menu();
    assert_eq!(menu.numberOfItems(), 5);
    let copy_path = menu.itemAtIndex(4).unwrap();
    assert_eq!(copy_path.keyEquivalent().to_string(), "c");
    assert_eq!(
        copy_path.keyEquivalentModifierMask(),
        NSEventModifierFlags::Command | NSEventModifierFlags::Shift
    );

    app.files.borrow_mut().entries = entries.clone();
    app.query.replace("Doc".into());
    delegate.filter();
    let menu = delegate.file_actions_menu();
    assert_eq!(menu.numberOfItems(), 6);
    assert_eq!(menu.itemAtIndex(0).unwrap().tag(), 1);
    assert_eq!(
        menu.itemAtIndex(1).unwrap().keyEquivalentModifierMask(),
        NSEventModifierFlags::Control
    );
    app.files.borrow_mut().entries = vec![entries[1].clone()];
    app.query.replace("report".into());
    delegate.filter();
    delegate.file_menu_action(&menu.itemAtIndex(0).unwrap());
    assert_eq!(
        *app.query.borrow(),
        "~/Documents/",
        "menu keeps its target during async updates"
    );
    assert!(delegate.searching_files());

    app.query.replace(r"^unmatched\.pdf$".into());
    app.files.borrow_mut().entries = entries;
    delegate.filter();
    assert!(
        delegate.selected_file().is_none(),
        "a changed pattern cannot activate stale results"
    );
    assert!(!ui.file_controls.actions.isEnabled());
    app.query.replace("[".into());
    app.files.borrow_mut().entries.clear();
    delegate.filter();
    let generation = app.files.borrow().generation;
    delegate.apply_files_update(Update::Results {
        generation,
        entries: Vec::new(),
        gathering: false,
        limited: false,
        error: Some("Invalid pattern; searching literal names only".into()),
    });
    assert!(!ui.footer.isHidden());
    assert!(
        ui.footer
            .stringValue()
            .to_string()
            .contains("Invalid pattern")
    );
    assert_eq!(*app.query.borrow(), "[");
    assert!(
        app.files.borrow().service.is_none(),
        "UI does no disk access or pattern compilation"
    );
    delegate.leave_scoped_search();
    assert!(ui.file_controls.view.isHidden());
    delegate.activate_selected();
}

fn verify_worker(delegate: &Delegate) {
    let predicate = crate::macos::platform::files::predicate("dc").unwrap();
    for (name, kind, expected) in [
        ("Documents", "public.folder", true),
        ("Documents", "public.directory", true),
        ("Documents.txt", "public.plain-text", false),
        ("dc.txt", "public.plain-text", true),
        ("Downloads", "public.folder", false),
    ] {
        let object = NSDictionary::from_slices(
            &[
                ns_string!("kMDItemFSName"),
                ns_string!("kMDItemContentType"),
            ],
            &[&*NSString::from_str(name), &*NSString::from_str(kind)],
        );
        assert_eq!(
            unsafe { predicate.evaluateWithObject(Some(&object)) },
            expected,
            "Spotlight candidates must support short folder abbreviations: {name}"
        );
    }
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("sample.txt"), "fixture").unwrap();
    let service = Service::new(
        temp.path().into(),
        delegate.ivars().wake.get().unwrap().handle(),
    );
    let generation = service.cancel();
    service.search(Request {
        generation,
        query: format!("{}/", temp.path().display()),
        settings: files::Settings::default(),
        recent: Vec::new(),
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let update = service
            .receiver
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        if let Update::Results {
            entries,
            error,
            gathering,
            ..
        } = update
        {
            assert!(error.is_none());
            assert!(!gathering);
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].name, "sample.txt");
            break;
        }
    }
    // Starting native queries catches Spotlight's stricter compound-predicate rules,
    // including single conditions and the short-folder abbreviation branch.
    for word in [
        "r",
        "rs",
        "dc",
        "sample",
        "文",
        "文件",
        "测试笔记",
        "[]",
        "*",
        "?",
        "\"",
        "'",
        "\\",
        "a\0b",
        "sample note",
        "x' OR TRUEPREDICATE OR '",
    ] {
        let generation = service.cancel();
        service.search(Request {
            generation,
            query: word.into(),
            settings: files::Settings {
                roots: vec![temp.path().to_string_lossy().into_owned()],
                excluded: Vec::new(),
                hide_generated: false,
            },
            recent: Vec::new(),
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let update = service
                .receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
            if let Update::Results {
                generation: result_generation,
                error,
                gathering,
                ..
            } = update
            {
                if result_generation != generation {
                    continue;
                }
                assert_eq!(
                    error,
                    winlane::features::files::query::Matcher::new(word, temp.path()).error,
                    "native query {word:?} must complete, with only a pattern validation warning"
                );
                if !gathering {
                    break;
                }
            }
        }
    }
    for pattern in [r"\.(txt|md)$", r"^\d+$", "*.txt"] {
        let (_, error) = worker_result(&service, pattern.into(), temp.path());
        assert!(
            error.is_none(),
            "native pattern predicate must be accepted: {error:?}"
        );
    }
    std::fs::create_dir(temp.path().join("nested")).unwrap();
    std::fs::write(temp.path().join("nested/child.txt"), "fixture").unwrap();
    let (entries, error) = worker_result(
        &service,
        format!("{}/*.txt", temp.path().display()),
        temp.path(),
    );
    assert!(error.is_none());
    assert_eq!(
        entries.len(),
        2,
        "explicit patterns recurse through the worker without Spotlight"
    );
    let (entries, error) = worker_result(
        &service,
        format!("{}/^sample\\.txt$", temp.path().display()),
        temp.path(),
    );
    assert!(error.is_none());
    assert_eq!(entries.len(), 1);
    std::fs::write(temp.path().join("["), "literal fixture").unwrap();
    let (entries, error) = worker_result(
        &service,
        format!("{}/[", temp.path().display()),
        temp.path(),
    );
    assert!(error.is_some());
    assert_eq!(
        entries.len(),
        1,
        "invalid regex still finds its literal filename"
    );
    service.cancel();
    service.save(Vec::new());
    let deadline = Instant::now() + Duration::from_secs(3);
    while !files::history::path(temp.path()).exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        files::history::path(temp.path()).exists(),
        "cancellation frees the worker to save history"
    );
    let predicate = crate::macos::platform::files::predicate("x' OR TRUEPREDICATE OR '").unwrap();
    assert!(!predicate.predicateFormat().is_empty());
}

fn worker_result(
    service: &Service,
    query: String,
    root: &std::path::Path,
) -> (Vec<Entry>, Option<String>) {
    let generation = service.cancel();
    service.search(Request {
        generation,
        query,
        settings: files::Settings {
            roots: vec![root.to_string_lossy().into_owned()],
            excluded: Vec::new(),
            hide_generated: false,
        },
        recent: Vec::new(),
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let update = service
            .receiver
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        if let Update::Results {
            generation: actual,
            entries,
            gathering: false,
            error,
            ..
        } = update
            && actual == generation
        {
            return (entries, error);
        }
    }
}
