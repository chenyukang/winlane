mod bluetooth;
mod clipboard;
mod commands;
pub(crate) mod diagnostics;
mod emoji;
mod input_rules;
mod input_start;
mod keep_awake;
mod launch_search;
mod main_wake;
mod open_url;
mod panels;
mod projects;
mod quicklink_search;
mod recency;
mod responsiveness;
mod scoped_loading;
mod settings;
mod settings_focus;
mod snippet_search;
mod window_liveness;
mod window_search;

use clipboard::{verify_clipboard_images, verify_clipboard_search};
use commands::verify_command_search;
use diagnostics::export_hidden_previews;
use input_start::{
    verify_direct_layout_input, verify_input_language_is_prepared_before_focus,
    verify_search_composition_survives_refresh,
};
pub(crate) use launch_search::launch_search_fixture;
use launch_search::{
    verify_catalog_refresh, verify_external_focus_history, verify_launch_search,
    verify_project_rule_search, verify_shortcut_recency, verify_switch_delay, verify_typo_search,
};
use main_wake::verify_main_wake;
use panels::{verify_adaptive_panels, verify_display_density, verify_usage_hint_visibility};
use projects::verify_projects_search;
use quicklink_search::verify_quicklink_search;
use recency::verify_recency_persistence;
use responsiveness::{
    pending_test_focus, responsive_fixture, verify_async_focus_order, verify_async_window_snapshot,
    verify_responsive_panels,
};
use settings::verify_autosave;
use snippet_search::verify_snippet_search;
use window_search::{
    verify_alias_does_not_block_title_search, verify_app_name_search,
    verify_distinct_window_aliases, verify_editor_window_titles, verify_switch_alias_prefix,
};

use super::*;

pub fn verify_hidden_panels() {
    let mtm = MainThreadMarker::new().expect("native checks must run on the main thread");
    let screens = NSScreen::screens(mtm);
    if screens.is_empty() {
        println!("Native panel checks skipped: no graphical display.");
        return;
    }
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Prohibited);
    crate::macos::ui::material::tests::verify_panel_materials(mtm);
    crate::macos::ui::input::tests::verify_shared_input(mtm);
    crate::macos::ui::input_indicator::tests::verify(mtm);
    crate::macos::ui::keep_awake_indicator::tests::verify(mtm);
    verify_main_wake(mtm);
    verify_switch_alias_prefix(mtm);
    verify_distinct_window_aliases(mtm);
    verify_alias_does_not_block_title_search(mtm);
    let delegate = Delegate::new(mtm);
    crate::macos::ui::settings::tests::verify_localized_settings(&delegate, mtm);
    verify_autosave(mtm);
    super::auto_appclose::tests::verify(mtm);
    settings_focus::verify_settings_focus(mtm);
    for language in ["en", "zh"] {
        if let Some(source) = Source::for_language(language, mtm) {
            let id = source
                .id()
                .expect("language source needs a stable identifier");
            assert_eq!(
                Source::by_id(&id, mtm).and_then(|source| source.id()),
                Some(id)
            );
        }
    }
    assert!(Source::by_id("example.test.missing-input-source", mtm).is_none());
    crate::macos::ui::app_shortcuts::tests::verify_hidden_settings(&delegate, mtm);
    crate::macos::platform::installed_apps::tests::verify_catalog();
    crate::macos::platform::catalog_watcher::tests::verify();
    verify_catalog_refresh(mtm);
    verify_launch_search(mtm);
    verify_command_search(mtm);
    keep_awake::verify(mtm);
    keep_awake::verify_reopened_selection(mtm);
    crate::macos::platform::keep_awake::tests::verify();
    crate::macos::platform::scrolling::tests::verify();
    scoped_loading::verify_scoped_loading(mtm);
    verify_snippet_search(mtm);
    emoji::verify_emoji(mtm);
    verify_quicklink_search(mtm);
    verify_projects_search(mtm);
    open_url::verify_open_url(mtm);
    super::files::tests::verify_files(mtm);
    bluetooth::verify_bluetooth(mtm);
    verify_clipboard_search(mtm);
    verify_clipboard_images(mtm);
    crate::macos::ui::snippets::tests::verify_editor(&delegate, mtm);
    crate::macos::platform::system_commands::tests::verify_prepared_commands(mtm);
    crate::macos::platform::menu_bar::tests::verify_reveal_positions();
    verify_typo_search(mtm);
    verify_shortcut_recency(mtm);
    verify_external_focus_history(mtm);
    verify_switch_delay(mtm);
    verify_responsive_panels(mtm);
    verify_search_composition_survives_refresh(mtm);
    verify_input_language_is_prepared_before_focus(mtm);
    input_start::verify_command_reapplies_input_policy(mtm);
    verify_direct_layout_input(mtm);
    input_rules::verify(mtm);
    verify_async_focus_order(mtm);
    verify_recency_persistence(mtm);
    verify_async_window_snapshot(mtm);
    window_liveness::verify_window_liveness(mtm);
    verify_project_rule_search(mtm);
    verify_adaptive_panels(mtm);
    verify_display_density(mtm);
    verify_usage_hint_visibility(mtm);
    verify_app_name_search(mtm);
    verify_editor_window_titles(mtm);
    delegate.ivars().demo.set(true);
    delegate.ivars().windows.replace(demo_windows());
    delegate.ivars().mode.set(Some(PanelMode::Search));
    delegate.sync_displays();
    delegate.filter();
    let panels = delegate.panels();
    for policy in [
        winlane::core::input_method::InputMethod::English,
        winlane::core::input_method::InputMethod::Chinese,
        winlane::core::input_method::InputMethod::Current,
        winlane::core::input_method::InputMethod::LastUsed,
    ] {
        let expected = match policy {
            winlane::core::input_method::InputMethod::English => Source::for_language("en", mtm),
            winlane::core::input_method::InputMethod::Chinese => Source::for_language("zh", mtm),
            winlane::core::input_method::InputMethod::Current => None,
            winlane::core::input_method::InputMethod::LastUsed => {
                NSUserDefaults::standardUserDefaults()
                    .stringForKey(ns_string!("WinlaneSearchInputSource"))
                    .and_then(|id| Source::by_id(&id.to_string(), mtm))
            }
        }
        .and_then(|source| source.id());
        delegate.ivars().config.borrow_mut().input_rules =
            winlane::features::input_rules::Settings::for_winlane(policy);
        delegate.prepare_search_input();
        assert_eq!(
            delegate.ivars().input_gate.borrow().target(),
            expected.as_deref(),
            "startup must respect the saved policy: {policy:?}"
        );
        delegate.start_input_gate_timer();
        let pending = delegate.ivars().input_start_timer.borrow().clone();
        delegate.focus_search();
        assert!(
            !delegate.ivars().input_session.borrow().focused,
            "hidden panels must never change or remember input sources"
        );
        assert_eq!(
            delegate.ivars().input_gate.borrow().target(),
            expected.as_deref(),
            "the prepared source must survive until the panel first receives focus"
        );
        delegate.ivars().input_session.borrow_mut().focused = true;
        delegate.complete_input_start();
        assert!(delegate.ivars().input_gate.borrow().target().is_none());
        assert!(delegate.ivars().input_start_timer.borrow().is_none());
        assert!(pending.as_ref().is_none_or(|timer| !timer.isValid()));
        delegate.prepare_search_input();
        delegate.start_input_gate_timer();
        let next = delegate.ivars().input_start_timer.borrow().clone();
        if let Some(stale) = pending {
            delegate.finish_input_start(sel!(finishSearchInputStart:), &stale);
            assert!(next.as_ref().is_none_or(|timer| timer.isValid()));
        }
        delegate.finish_search_input();
        assert!(delegate.ivars().input_gate.borrow().target().is_none());
        assert!(delegate.ivars().input_start_timer.borrow().is_none());
        assert!(next.as_ref().is_none_or(|timer| !timer.isValid()));
    }
    delegate.ivars().config.borrow_mut().input_rules =
        winlane::features::input_rules::Settings::for_winlane(
            winlane::core::input_method::InputMethod::Current,
        );
    let mut distinct_frames = Vec::new();
    for screen in screens.iter() {
        if !distinct_frames.contains(&screen.frame()) {
            distinct_frames.push(screen.frame());
        }
    }
    assert_eq!(panels.len(), distinct_frames.len());
    assert!(panels.iter().all(|ui| !ui.panel.isVisible()));
    for ui in &panels {
        let frame = ui.panel.frame();
        assert!(screens.iter().any(|screen| {
            let visible = screen.visibleFrame();
            frame.origin.x >= visible.origin.x
                && frame.origin.y >= visible.origin.y
                && frame.origin.x + frame.size.width <= visible.origin.x + visible.size.width + 1.0
                && frame.origin.y + frame.size.height
                    <= visible.origin.y + visible.size.height + 1.0
        }));
        assert!(
            ui.panel
                .collectionBehavior()
                .contains(NSWindowCollectionBehavior::CanJoinAllSpaces)
        );
        assert!(
            !ui.panel
                .collectionBehavior()
                .contains(NSWindowCollectionBehavior::MoveToActiveSpace)
        );
    }
    // Exercise typing from a secondary panel through the production delegate.
    let input = &panels.last().unwrap().input;
    input.setStringValue(ns_string!("Safari"));
    let notification = unsafe {
        NSNotification::notificationWithName_object(
            ns_string!("NSControlTextDidChangeNotification"),
            Some(input),
        )
    };
    unsafe {
        let _: () = msg_send![&*delegate, controlTextDidChange: &*notification];
    }
    assert_eq!(delegate.ivars().matches.borrow().len(), 2);
    for ui in &panels {
        assert_eq!(ui.input.stringValue().to_string(), "Safari");
        assert_eq!(ui.list.subviews().len(), 2);
        assert!(!ui.input.isHidden());
    }
    let rows_before: Vec<_> = panels.iter().map(|ui| ui.list.subviews()).collect();
    delegate.move_selection(1);
    for (ui, rows) in panels.iter().zip(&rows_before) {
        assert_eq!(
            ui.list.subviews(),
            *rows,
            "moving selection must reuse row views"
        );
        let selection: Vec<_> = ui
            .list
            .subviews()
            .iter()
            .map(|row| row.isAccessibilitySelected())
            .collect();
        assert_eq!(selection, [false, true]);
    }
    delegate.ivars().query.replace("no matching window".into());
    delegate.filter();
    delegate.ivars().query.replace("Safari".into());
    delegate.filter();
    for (ui, rows) in panels.iter().zip(&rows_before) {
        assert_eq!(
            ui.list.subviews(),
            *rows,
            "filtering should reuse detached rows"
        );
    }
    let matched = delegate.ivars().matches.borrow()[0];
    delegate.ivars().windows.borrow_mut()[matched].title = "Updated title".into();
    delegate.render();
    for ui in &panels {
        let rows = ui.rows.borrow();
        assert_eq!(rows[0].title.stringValue().to_string(), "Updated title");
        assert_eq!(rows[0].icon.image(), rows[1].icon.image());
    }
    let scope = delegate.menu_item("Current app only", sel!(toggleScope:), "");
    delegate.ivars().query.replace(String::new());
    unsafe {
        let _: () = msg_send![&*delegate, toggleScope: &*scope];
        let _: bool = msg_send![&*delegate, validateMenuItem: &*scope];
    }
    assert!(delegate.ivars().current_app_only.get());
    assert_eq!(scope.state(), NSControlStateValueOn);
    assert!(panels.iter().all(|ui| ui.list.subviews().len() == 2));
    unsafe {
        let _: () = msg_send![&*delegate, toggleScope: &*scope];
        let _: bool = msg_send![&*delegate, validateMenuItem: &*scope];
    }
    assert!(!delegate.ivars().current_app_only.get());
    assert_eq!(scope.state(), NSControlStateValueOff);
    delegate.ivars().mode.set(Some(PanelMode::Switch));
    delegate.render();
    assert!(
        panels
            .iter()
            .all(|ui| ui.input.isHidden() && !ui.mode_label.isHidden())
    );
    delegate.ivars().alias_input.borrow_mut().push('w');
    delegate.render();
    assert!(
        panels
            .iter()
            .all(|ui| ui.mode_label.stringValue().to_string().contains("Alias  w"))
    );
    delegate.display_search(0);
    assert!(
        panels
            .iter()
            .all(|ui| !ui.input.isHidden() && ui.mode_label.isHidden())
    );
    for mode in [PanelMode::Search, PanelMode::Switch] {
        delegate.ivars().mode.set(Some(mode));
        for percent in [0, 50, 100] {
            delegate.ivars().config.borrow_mut().background_opacity = percent;
            delegate.render();
            for ui in &panels {
                let root = ui.panel.contentView().unwrap();
                assert!(!ui.panel.isOpaque());
                assert_eq!(ui.panel.alphaValue(), 1.0);
                assert_eq!(root.alphaValue(), 1.0);
                crate::macos::ui::material::tests::verify_backdrop_opacity(
                    &ui.backdrop,
                    f64::from(percent) / 100.0,
                );
                assert_eq!(ui.backdrop.view().frame(), root.bounds());
                assert_eq!(ui.input.alphaValue(), 1.0);
                assert_eq!(
                    unsafe { ui.input.superview() },
                    Some(ui.backdrop.content.clone())
                );
                for row in ui.rows.borrow().iter() {
                    assert_eq!(row.button.alphaValue(), 1.0);
                    assert_eq!(row.title.alphaValue(), 1.0);
                    assert_eq!(row.icon.alphaValue(), 1.0);
                }
                assert!(!ui.panel.isVisible());
            }
        }
    }
    delegate.ivars().mode.set(Some(PanelMode::Search));
    delegate.render();
    let ids: Vec<_> = panels.iter().map(|ui| ui.panel.windowNumber()).collect();
    delegate.sync_displays();
    assert_eq!(
        delegate
            .panels()
            .iter()
            .map(|ui| ui.panel.windowNumber())
            .collect::<Vec<_>>(),
        ids
    );
    if let Ok(directory) = std::env::var("WINLANE_PREVIEW_DIR") {
        export_hidden_previews(&delegate, &directory);
    }
    delegate.end_session();
    assert!(delegate.panels().iter().all(|ui| !ui.panel.isVisible()));
    let retained_rows = panels[0].list.subviews();
    delegate.ivars().query.replace("nothing matches".into());
    delegate.filter();
    assert_eq!(
        panels[0].list.subviews(),
        retained_rows,
        "closed panels must not redraw"
    );
    println!(
        "Native panel checks passed on {} display(s): placement, shared query/scope/mode/alias, reuse; no panels shown or shortcuts registered.",
        panels.len()
    );
}
