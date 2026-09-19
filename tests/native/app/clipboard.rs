use super::*;

pub(super) fn verify_clipboard_search(mtm: MainThreadMarker) {
    use crate::macos::platform::clipboard::{ClipboardRuntime, Observer, now};
    use winlane::features::clipboard::ClipboardSettings;
    let board = NSPasteboard::pasteboardWithUniqueName();
    let path = std::env::temp_dir()
        .join(format!("winlane-native-clipboard-{}", std::process::id()))
        .join("history.json");
    let settings = ClipboardSettings::default();
    let mut observer = Observer::new(&board);
    let mut history = winlane::features::clipboard::History::default();
    let put = |text: &str| {
        board.clearContents();
        assert!(
            board.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString })
        );
    };
    let capture = |observer: &mut Observer,
                   history: &mut winlane::features::clipboard::History,
                   settings: &ClipboardSettings| match observer.read(
        &board,
        settings,
        "com.example.editor",
    ) {
        Some(crate::macos::platform::clipboard::Capture::Text(text)) => {
            history.record(&text, "Example Editor", now(), settings)
        }
        _ => false,
    };
    assert!(!capture(&mut observer, &mut history, &settings));
    put("First test copy");
    assert!(capture(&mut observer, &mut history, &settings));
    assert!(!capture(&mut observer, &mut history, &settings));
    put("protected test content");
    board.setData_forType(
        Some(&NSData::new()),
        ns_string!("org.nspasteboard.ConcealedType"),
    );
    assert!(!capture(&mut observer, &mut history, &settings));
    put("generated test content");
    crate::macos::platform::clipboard::mark_generated(&board);
    assert!(!capture(&mut observer, &mut history, &settings));
    put("while paused");
    let paused = ClipboardSettings {
        enabled: false,
        ..settings.clone()
    };
    assert!(!capture(&mut observer, &mut history, &paused));
    assert!(!capture(&mut observer, &mut history, &settings));
    put(&"x".repeat(winlane::features::clipboard::MAX_ITEM_BYTES + 1));
    assert!(!capture(&mut observer, &mut history, &settings));
    assert_eq!(history.entries.len(), 1);

    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    let mut runtime = ClipboardRuntime::with_board(settings.clone(), board.clone(), path.clone());
    let deadline = Instant::now() + Duration::from_secs(5);
    while runtime.loading {
        assert!(Instant::now() < deadline);
        runtime.poll_storage();
        std::thread::sleep(Duration::from_millis(5));
    }
    runtime.history = history;
    runtime
        .history
        .record("https://example.com/notes", "Browser", now(), &settings);
    let recent = runtime.history.entries[0].id;
    state.clipboard.replace(Some(runtime));
    state.catalog_checked.set(Some(Instant::now()));
    state.mode.set(Some(PanelMode::Search));
    state.previous_pid.set(-400);
    state.config.borrow_mut().show_usage_hints = false;
    state.config.borrow_mut().snippets = vec![winlane::features::snippets::Snippet {
        id: "test".into(),
        name: "First".into(),
        body: "Text".into(),
    }];
    delegate.install_windows(vec![WindowInfo {
        id: 77,
        pid: -77,
        app: "Editor".into(),
        title: "First".into(),
        minimized: false,
    }]);
    delegate.sync_displays();
    state.query.replace("first".into());
    delegate.filter();
    assert!(
        state.clipboard_matches.borrow().is_empty(),
        "ordinary search excludes history"
    );
    state.query.replace("clipboard".into());
    delegate.filter();
    assert_eq!(delegate.selected_command(), Some(CommandId::Clipboard));
    delegate.activate_selected();
    assert!(delegate.searching_clipboard());
    assert!(state.query.borrow().is_empty());
    assert_eq!(state.previous_pid.get(), -400);
    assert_eq!(delegate.match_count(), 2);
    assert_eq!(delegate.selected_clipboard().unwrap().id, recent);
    assert!(state.matches.borrow().is_empty());
    assert!(state.launch_matches.borrow().is_empty());
    assert!(state.snippet_matches.borrow().is_empty());
    for ui in delegate.panels() {
        assert!(!ui.scope_back.isHidden());
        assert!(!ui.clipboard_actions.isHidden());
        assert_eq!(
            ui.rows.borrow()[0].title.stringValue().to_string(),
            "https://example.com/notes"
        );
        assert!(
            ui.scope_back.frame().origin.x + ui.scope_back.frame().size.width
                < ui.input.frame().origin.x
        );
        assert!(
            ui.input.frame().origin.x + ui.input.frame().size.width
                < ui.clipboard_actions.frame().origin.x
        );
        assert!(!ui.panel.isVisible());
    }
    delegate.move_selection(1);
    let selected = delegate.selected_clipboard().unwrap().id;
    state
        .clipboard
        .borrow_mut()
        .as_mut()
        .unwrap()
        .history
        .record("Later copy", "Example", now(), &settings);
    delegate.filter_preserving(Some(SelectedResult::Clipboard(selected)));
    assert_eq!(delegate.selected_clipboard().unwrap().id, selected);
    state.catalog_checked.set(None);
    state.query.replace("browser notes".into());
    delegate.filter();
    delegate.ensure_app_catalog();
    assert!(state.catalog_receiver.borrow().is_none());
    assert_eq!(delegate.match_count(), 1);
    assert_eq!(delegate.selected_clipboard().unwrap().id, recent);
    delegate.delete_clipboard_entry();
    assert!(delegate.selected_clipboard().is_none());
    delegate.activate_selected();
    assert!(
        delegate.searching_clipboard(),
        "empty Enter leaves history open"
    );
    state.query.borrow_mut().clear();
    delegate.filter();
    delegate.toggle_mode(0);
    assert!(
        delegate.searching_clipboard(),
        "Space belongs to history search"
    );
    state.catalog_checked.set(Some(Instant::now()));
    delegate.leave_scoped_search();
    assert!(!delegate.scoped_search());
    assert_eq!(*state.query.borrow(), "clipboard");
    assert_eq!(delegate.selected_command(), Some(CommandId::Clipboard));
    delegate.activate_selected();
    delegate.clear_clipboard_history();
    assert_eq!(delegate.match_count(), 0);
    assert!(
        state
            .clipboard
            .borrow()
            .as_ref()
            .unwrap()
            .history
            .entries
            .is_empty()
    );
    delegate.cancel_search();
    assert_eq!(state.mode.get(), None);
    state.clipboard.take();
    assert!(!path.exists());

    let mut runtime = ClipboardRuntime::with_board(settings.clone(), board.clone(), path.clone());
    runtime.configure(ClipboardSettings {
        persistent: false,
        ..settings
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while runtime.loading {
        assert!(Instant::now() < deadline);
        runtime.poll_storage();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !runtime.poll_clipboard(),
        "startup must not read pre-existing clipboard content"
    );
    drop(runtime);
    assert!(!path.exists());
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
    board.clearContents();
    println!(
        "Clipboard checks passed: isolated pasteboard capture, markers, pause, scope isolation, MRU search, deletion, clear and navigation; no real clipboard or application changed."
    );
}

pub(super) fn verify_clipboard_images(mtm: MainThreadMarker) {
    use crate::macos::platform::clipboard::{
        Capture, ClipboardRuntime, Observer, PasteContent, now,
    };
    use winlane::features::clipboard::{ClipboardSettings, ImageFormat};
    let png = include_bytes!("../../fixtures/clipboard-image.png");
    let settings = ClipboardSettings::default();
    let board = NSPasteboard::pasteboardWithUniqueName();
    let destination = NSPasteboard::pasteboardWithUniqueName();
    let mut observer = Observer::new(&board);
    let native = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(png)).unwrap();
    let tiff = native.TIFFRepresentation().unwrap().to_vec();
    let path = std::env::temp_dir()
        .join(format!("winlane-native-image-{}", std::process::id()))
        .join("history.json");
    let mut runtime = ClipboardRuntime::with_board(settings.clone(), board.clone(), path.clone());
    let wait = |runtime: &mut ClipboardRuntime, count: usize| {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            runtime.poll_storage();
            if !runtime.loading && runtime.history.entries.len() == count {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "image processing timed out: {:?}",
                runtime.error
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    wait(&mut runtime, 0);
    for (format, bytes) in [
        (ImageFormat::Png, png.as_slice()),
        (ImageFormat::Tiff, tiff.as_slice()),
    ] {
        board.clearContents();
        board.setString_forType(ns_string!("Image alt text"), unsafe {
            NSPasteboardTypeString
        });
        board.setString_forType(
            ns_string!("file:///example.png"),
            ns_string!("public.file-url"),
        );
        assert!(board.setData_forType(
            Some(&NSData::with_bytes(bytes)),
            &NSString::from_str(format.pasteboard_type())
        ));
        let capture = observer
            .read(&board, &settings, "app.windowlane.desktop")
            .expect("system screenshots must be captured even with Winlane in front");
        assert!(
            matches!(capture, Capture::Image(_, kind) if kind == format),
            "images take priority over text and file metadata"
        );
        assert!(
            observer
                .read(&board, &settings, "app.windowlane.desktop")
                .is_none(),
            "each clipboard change is captured once"
        );
        let count = runtime.history.entries.len() + 1;
        runtime.accept(capture, "Image Fixture", now());
        wait(&mut runtime, count);
        let entry = &runtime.history.entries[0];
        let info = entry.image.as_ref().unwrap();
        assert_eq!((info.width, info.height), (256, 128));
        let thumb = runtime.thumbnail(info).unwrap();
        assert!(thumb.size().width <= 96.0 && thumb.size().height <= 96.0);
        let mut generated = Observer::new(&destination);
        PasteContent::from_entry(entry)
            .unwrap()
            .write(&destination)
            .unwrap();
        assert_eq!(
            destination
                .dataForType(&NSString::from_str(format.pasteboard_type()))
                .unwrap()
                .to_vec(),
            bytes,
            "paste must preserve the original bytes and format"
        );
        assert!(
            destination
                .stringForType(unsafe { NSPasteboardTypeString })
                .is_none(),
            "image copies must not become text paths"
        );
        assert!(
            destination
                .types()
                .unwrap()
                .iter()
                .any(|kind| kind.to_string() == "org.nspasteboard.AutoGeneratedType")
        );
        assert!(
            generated
                .read(&destination, &settings, "app.windowlane.desktop")
                .is_none(),
            "reusing history must not record the image again"
        );
    }
    let first_path = runtime.history.entries[0]
        .image
        .as_ref()
        .unwrap()
        .asset
        .as_ref()
        .unwrap()
        .image
        .clone();
    drop(runtime);
    assert!(!first_path.exists(), "session copies are released on quit");
    let mut runtime = ClipboardRuntime::with_board(settings.clone(), board.clone(), path.clone());
    wait(&mut runtime, 2);
    for entry in &runtime.history.entries {
        assert!(runtime.thumbnail(entry.image.as_ref().unwrap()).is_some());
        assert!(PasteContent::from_entry(entry).is_ok());
    }
    assert!(
        crate::macos::platform::clipboard::image::prepare(
            b"invalid".to_vec(),
            ImageFormat::Png,
            &winlane::features::clipboard::assets::SessionFiles::new()
        )
        .is_err()
    );
    let delegate = Delegate::new(mtm);
    delegate.ivars().clipboard.replace(Some(runtime));
    delegate.ivars().mode.set(Some(PanelMode::Search));
    delegate.ivars().catalog_checked.set(Some(Instant::now()));
    delegate.sync_displays();
    delegate.enter_scoped_search(SearchScope::Clipboard);
    for ui in delegate.panels() {
        let rows = ui.rows.borrow();
        assert!(
            rows[0]
                .title
                .stringValue()
                .to_string()
                .contains("256 × 128")
        );
        let image = rows[0].icon.image().unwrap();
        assert!(!image.isTemplate());
        assert!(image.size().width <= 96.0);
        assert!(!ui.panel.isVisible());
    }
    delegate.delete_clipboard_entry();
    assert_eq!(delegate.match_count(), 1);
    delegate.clear_clipboard_history();
    assert_eq!(delegate.match_count(), 0);
    let mut runtime = delegate.ivars().clipboard.take().unwrap();
    runtime.accept(
        Capture::Image(png.to_vec(), ImageFormat::Png),
        "Image Fixture",
        now(),
    );
    runtime.clear();
    // Complete the in-flight worker after clear; its result must not restore the deleted image.
    for _ in 0..50 {
        runtime.poll_storage();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(runtime.history.entries.is_empty());
    drop(runtime);
    assert!(!path.exists());
    assert!(
        std::fs::read_dir(path.with_file_name("images"))
            .unwrap()
            .next()
            .is_none()
    );
    board.clearContents();
    destination.clearContents();
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
    println!(
        "Clipboard images passed: PNG/TIFF capture, original-byte pasteboard round trips, bounded thumbnails, hidden rows, restart, deletion, and clear while decoding; real clipboard unchanged."
    );
}
