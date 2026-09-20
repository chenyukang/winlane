use super::*;

#[derive(Default)]
struct FocusProbe {
    visible: Cell<bool>,
    key_requests: Cell<usize>,
    centers: Cell<usize>,
}

define_class!(
    // SAFETY: This main-thread test window records presentation without showing UI.
    #[unsafe(super = NSWindow)]
    #[thread_kind = MainThreadOnly]
    #[ivars = FocusProbe]
    struct SettingsFocusProbe;
    unsafe impl NSObjectProtocol for SettingsFocusProbe {}
    impl SettingsFocusProbe {
        #[unsafe(method(isVisible))]
        fn visible(&self) -> bool { self.ivars().visible.get() }
        #[unsafe(method(makeKeyAndOrderFront:))]
        fn make_key(&self, _: Option<&AnyObject>) {
            self.ivars().visible.set(true);
            self.ivars().key_requests.set(self.ivars().key_requests.get() + 1);
        }
        #[unsafe(method(orderFrontRegardless))]
        fn order_front(&self) { self.ivars().visible.set(true); }
        #[unsafe(method(orderOut:))]
        fn order_out(&self, _: Option<&AnyObject>) { self.ivars().visible.set(false); }
        #[unsafe(method(center))]
        fn center(&self) { self.ivars().centers.set(self.ivars().centers.get() + 1); }
    }
);

pub(super) fn verify_settings_focus(mtm: MainThreadMarker) {
    let previous_locale = winlane::core::i18n::locale();
    let app = NSApplication::sharedApplication(mtm);
    let source_before = Source::current(mtm).and_then(|source| source.id());
    let frontmost_before = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|app| app.processIdentifier());
    let delegate = responsive_fixture(mtm);
    let mut settings = SettingsWindow::new(&delegate, mtm);
    let frame = rect(90.0, 110.0, 1020.0, 740.0);
    let probe: Retained<SettingsFocusProbe> = unsafe {
        msg_send![super(SettingsFocusProbe::alloc(mtm).set_ivars(FocusProbe::default())),
            initWithContentRect: frame, styleMask: NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
            backing: NSBackingStoreType::Buffered, defer: false]
    };
    unsafe { probe.setReleasedWhenClosed(false) };
    probe.setCollectionBehavior(settings.window.collectionBehavior());
    probe.setContentView(settings.window.contentView().as_deref());
    settings.window = probe.clone().into_super();
    let draft = Config {
        background_opacity: 42,
        ..Config::default()
    };
    settings.fill(&draft);
    settings.select_tab(1);
    let settings = Rc::new(settings);
    delegate.ivars().settings.replace(Some(settings.clone()));
    probe.ivars().visible.set(true);
    delegate.prepare_panel(PanelMode::Search, 123, 0);
    assert!(
        !probe.isVisible(),
        "search temporarily hides the existing Settings window"
    );
    delegate.cancel_input_start();

    let previous_menu = app.mainMenu();
    let menu = NSMenu::new(mtm);
    let item = delegate.menu_item("Settings", sel!(showSettings:), ",");
    menu.addItem(&item);
    app.setMainMenu(Some(&menu));
    let panel = delegate.panels()[0].clone();
    let event = {
        NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::KeyDown, NSPoint::ZERO, NSEventModifierFlags::Command, 0.0,
            panel.panel.windowNumber(), None, ns_string!(","), ns_string!(","), false, 43,
        ).unwrap()
    };
    panel.panel.sendEvent(&event);
    app.setMainMenu(previous_menu.as_deref());
    assert_eq!(
        delegate.ivars().mode.get(),
        None,
        "Command-comma must end search"
    );
    assert!(Rc::ptr_eq(&settings, &delegate.settings_window().unwrap()));
    assert!(probe.isVisible());
    let before_activation = probe.ivars().key_requests.get();
    assert!(before_activation > 0);
    let activation = unsafe {
        NSNotification::notificationWithName_object(
            NSApplicationDidBecomeActiveNotification,
            Some(&app),
        )
    };
    delegate.became_active(sel!(applicationDidBecomeActive:), &activation);
    assert!(
        probe.ivars().key_requests.get() > before_activation,
        "a delayed app activation must complete the focus request for existing Settings"
    );
    assert_eq!(
        probe.ivars().centers.get(),
        0,
        "bringing back existing Settings must preserve its position"
    );
    assert_eq!(settings.selected_tab(), 1);
    assert_eq!(
        settings.candidate().unwrap(),
        draft,
        "bringing back Settings must not reload unfinished edits"
    );
    assert!(
        probe
            .collectionBehavior()
            .contains(NSWindowCollectionBehavior::MoveToActiveSpace)
    );
    let completed = probe.ivars().key_requests.get();
    delegate.became_active(sel!(applicationDidBecomeActive:), &activation);
    assert_eq!(
        probe.ivars().key_requests.get(),
        completed,
        "later activation must not steal focus again"
    );

    delegate.settings_action(sel!(showSettings:), None);
    delegate.prepare_panel(PanelMode::Search, 124, 0);
    let canceled = probe.ivars().key_requests.get();
    delegate.became_active(sel!(applicationDidBecomeActive:), &activation);
    assert_eq!(
        probe.ivars().key_requests.get(),
        canceled,
        "a new search cancels pending Settings focus"
    );
    delegate.end_session();
    delegate.settings_action(sel!(showSettings:), None);
    let closed = unsafe {
        NSNotification::notificationWithName_object(NSWindowWillCloseNotification, Some(&*probe))
    };
    delegate.settings_closed(sel!(windowWillClose:), &closed);
    probe.ivars().visible.set(false);
    let canceled = probe.ivars().key_requests.get();
    delegate.became_active(sel!(applicationDidBecomeActive:), &activation);
    assert_eq!(
        probe.ivars().key_requests.get(),
        canceled,
        "closing Settings cancels pending focus"
    );
    delegate.release_closed_settings();
    assert!(delegate.settings_window().is_none());
    assert_eq!(
        Source::current(mtm).and_then(|source| source.id()),
        source_before
    );
    assert_eq!(
        NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|app| app.processIdentifier()),
        frontmost_before
    );
    assert!(!delegate.any_panel_visible());
    for panel in delegate.panels() {
        panel.panel.close();
    }
    winlane::core::i18n::set_locale(previous_locale);
    println!(
        "Settings focus: Command-comma reuses the existing window and draft, completes delayed activation once, and cancels on search/close; no windows shown or apps activated."
    );
}
