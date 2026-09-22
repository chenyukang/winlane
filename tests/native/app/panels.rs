use super::*;

pub(super) fn verify_usage_hint_visibility(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.config.borrow_mut().display_density = DisplayDensity::Compact;
    state.demo.set(true);
    state.windows.replace(
        (0..10)
            .map(|id| WindowInfo {
                id,
                pid: -1,
                app: "Browser".into(),
                title: format!("Window {id}"),
                minimized: false,
            })
            .collect(),
    );
    delegate.sync_displays();
    for mode in [PanelMode::Search, PanelMode::Switch] {
        state.mode.set(Some(mode));
        for show_hints in [true, false, true, false] {
            state.config.borrow_mut().show_usage_hints = show_hints;
            delegate.filter();
            for ui in delegate.panels() {
                assert_eq!(ui.footer.isHidden(), !show_hints);
                assert_eq!(
                    ui.mode_label.isHidden(),
                    mode != PanelMode::Switch || !show_hints
                );
                let root = ui.panel.contentView().unwrap();
                let settings = ui
                    .backdrop
                    .content
                    .subviews()
                    .into_iter()
                    .filter_map(|view| view.downcast::<NSButton>().ok())
                    .find(|button| button.action() == Some(sel!(showSettings:)))
                    .unwrap();
                assert_eq!(settings.isHidden(), !show_hints);
                assert!(settings.frame().origin.y >= 0.0);
                assert!(
                    settings.frame().origin.y + settings.frame().size.height
                        <= ui.scroll.frame().origin.y
                );
                assert_eq!(
                    root.bounds().size.height,
                    if mode == PanelMode::Switch {
                        380.0 + if show_hints { MODE_LABEL_SPACING } else { 0.0 }
                    } else {
                        408.0
                    }
                );
                assert!(!ui.panel.isVisible());
            }
        }
    }
    state.alias_input.borrow_mut().push('z');
    delegate.render();
    for ui in delegate.panels() {
        assert!(
            !ui.mode_label.isHidden(),
            "typed alias feedback must stay visible"
        );
        assert!(ui.mode_label.stringValue().to_string().contains("Alias  z"));
        assert!(ui.footer.isHidden());
    }
    state.alias_input.borrow_mut().clear();
    delegate.render();
    assert!(delegate.panels().iter().all(|ui| ui.mode_label.isHidden()));
    state.demo.set(false);
    state
        .hotkey_error
        .replace(Some("Shortcut unavailable".into()));
    delegate.render();
    for ui in delegate.panels() {
        assert!(!ui.footer.isHidden());
        assert_eq!(ui.footer.stringValue().to_string(), "Shortcut unavailable");
    }
    state.hotkey_error.replace(None);
    state.alias_error.replace(Some("Alias unavailable".into()));
    delegate.render();
    for ui in delegate.panels() {
        assert!(!ui.footer.isHidden());
        assert_eq!(ui.footer.stringValue().to_string(), "Alias unavailable");
    }
    state.alias_error.replace(None);
    for mode in [PanelMode::Search, PanelMode::Switch] {
        state.mode.set(Some(mode));
        for loading in [false, true] {
            state.loading.set(loading);
            for show_hints in [true, false] {
                state.config.borrow_mut().show_usage_hints = show_hints;
                delegate.render();
                for ui in delegate.panels() {
                    assert_eq!(
                        ui.footer.isHidden(),
                        !show_hints && accessibility::is_trusted(),
                        "refreshing must respect the footer toggle; permission errors stay visible"
                    );
                }
            }
        }
    }
    state.loading.set(false);
    state.demo.set(true);
    delegate.render();
    delegate.report_switch_error("Switch failed");
    for ui in delegate.panels() {
        assert!(!ui.footer.isHidden());
        assert_eq!(ui.footer.stringValue().to_string(), "Switch failed");
    }
    println!(
        "Usage hint checks passed: both modes, all displays, compact layout, settings access, alias feedback and errors."
    );
}

pub(super) fn verify_display_density(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.demo.set(true);
    state.windows.replace(
        (0..24)
            .map(|id| WindowInfo {
                id,
                pid: -1,
                app: "Editor".into(),
                title: format!("Project {id}"),
                minimized: false,
            })
            .collect(),
    );
    delegate.sync_displays();
    for mode in [PanelMode::Search, PanelMode::Switch] {
        state.mode.set(Some(mode));
        state.query.replace(String::new());
        delegate.filter();
        state.selected.set(23);
        for (density, pitch, icon_size, font_size) in [
            (DisplayDensity::Compact, 28.0, 20.0, 13.0),
            (DisplayDensity::Normal, 32.0, 24.0, 15.0),
            (DisplayDensity::Compact, 28.0, 20.0, 13.0),
        ] {
            state.config.borrow_mut().display_density = density;
            delegate.render();
            for ui in delegate.panels() {
                let rows = ui.rows.borrow();
                assert_eq!(
                    rows[1].button.frame().origin.y - rows[0].button.frame().origin.y,
                    pitch
                );
                assert_eq!(rows[0].icon.frame().size.width, icon_size);
                assert_eq!(rows[0].title.font().unwrap().pointSize(), font_size);
                for row in rows.iter() {
                    let center = row.button.bounds().size.height / 2.0;
                    for view in [&*row.alias as &NSView, &*row.icon as &NSView] {
                        let frame = view.frame();
                        assert!((frame.origin.y + frame.size.height / 2.0 - center).abs() < 0.01);
                    }
                    for child in row.button.subviews() {
                        let frame = child.frame();
                        assert!(frame.origin.y >= 0.0);
                        assert!(
                            frame.origin.y + frame.size.height <= row.button.bounds().size.height
                        );
                        assert!(
                            frame.origin.x + frame.size.width <= row.button.bounds().size.width
                        );
                    }
                }
                let selected = rows[23].button.frame();
                let viewport = ui.scroll.contentView().bounds();
                assert!(selected.origin.y >= viewport.origin.y);
                assert!(
                    selected.origin.y + selected.size.height
                        <= viewport.origin.y + viewport.size.height
                );
                assert!(rows[23].button.isAccessibilitySelected());
                assert!(ui.panel.contentView().unwrap().bounds().size.height <= HEIGHT);
                assert!(!ui.panel.isVisible());
            }
            let before: Vec<_> = delegate
                .panels()
                .iter()
                .map(|ui| ui.list.subviews())
                .collect();
            delegate.render();
            for (ui, rows) in delegate.panels().iter().zip(before) {
                assert_eq!(
                    ui.list.subviews(),
                    rows,
                    "unchanged density must reuse rows"
                );
            }
        }
        let mut heights = Vec::new();
        state.query.replace("Project 1".into());
        for density in [DisplayDensity::Compact, DisplayDensity::Normal] {
            state.config.borrow_mut().display_density = density;
            delegate.filter();
            heights.push(
                delegate.panels()[0]
                    .panel
                    .contentView()
                    .unwrap()
                    .bounds()
                    .size
                    .height,
            );
        }
        assert!(
            heights[1] > heights[0],
            "normal rows must expand the adaptive panel"
        );
    }
    println!(
        "Density checks passed: live Compact/Normal changes, centered aliases/icons, scrolling, selection, row reuse and all displays."
    );
}

pub(super) fn verify_adaptive_panels(mtm: MainThreadMarker) {
    let delegate = Delegate::new(mtm);
    let state = delegate.ivars();
    state.config.borrow_mut().display_density = DisplayDensity::Compact;
    state.demo.set(true);
    for mode in [PanelMode::Search, PanelMode::Switch] {
        state.mode.set(Some(mode));
        let mut heights = Vec::new();
        let mut top_edges = Vec::new();
        for count in [24, 14, 7, 1, 0] {
            state.windows.replace(
                (0..count)
                    .map(|id| WindowInfo {
                        id,
                        pid: -1,
                        app: "Browser".into(),
                        title: format!("Window {id}"),
                        minimized: false,
                    })
                    .collect(),
            );
            delegate.filter();
            if state.panels.borrow().is_empty() {
                delegate.sync_displays();
                delegate.render();
            }
            for (index, ui) in delegate.panels().iter().enumerate() {
                let frame = ui.panel.frame();
                let root = ui.panel.contentView().unwrap();
                let top = frame.origin.y + frame.size.height;
                if count == 24 {
                    top_edges.push(top);
                }
                assert!(
                    (top - top_edges[index]).abs() < 1.0,
                    "filtering must preserve the panel's top edge"
                );
                assert!(
                    ui.scroll.frame().origin.y
                        >= ui.footer.frame().origin.y + ui.footer.frame().size.height
                );
                if mode == PanelMode::Search {
                    assert!(
                        ui.input.frame().origin.y + ui.input.frame().size.height
                            <= root.bounds().size.height
                    );
                    assert!(
                        ui.scroll.frame().origin.y + ui.scroll.frame().size.height
                            <= ui.input.frame().origin.y - 8.0
                    );
                }
                for label in ui.empty_labels.borrow().iter() {
                    assert!(label.frame().origin.y >= 0.0);
                    assert!(
                        label.frame().origin.y + label.frame().size.height
                            <= ui.scroll.frame().size.height,
                        "empty-state text must fit without scrolling"
                    );
                }
                assert!(!ui.panel.isVisible());
            }
            heights.push(
                delegate.panels()[0]
                    .panel
                    .contentView()
                    .unwrap()
                    .bounds()
                    .size
                    .height,
            );
        }
        assert!(
            heights[1] < heights[0],
            "fourteen items should be shorter than the capped list"
        );
        assert!(heights[2] < heights[1]);
        assert!(heights[3] < heights[2]);
    }
    println!(
        "Adaptive panel checks passed: two modes, 0/1/7/14/24 items, fixed top edge and visible controls."
    );
}
