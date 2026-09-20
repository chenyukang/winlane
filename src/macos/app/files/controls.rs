use super::super::*;

pub(in crate::macos::app) struct Controls {
    pub view: Retained<NSView>,
    pub actions: Retained<NSButton>,
}

impl Controls {
    pub(in crate::macos::app) fn new(delegate: &Delegate) -> Self {
        let mtm = delegate.mtm();
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(WIDTH - 140.0, 0.0, 100.0, 28.0));
        let actions = delegate.button(
            tr!("操作  ⌃T", "Actions  ⌃T"),
            sel!(showFileActions:),
            rect(0.0, 1.0, 100.0, 25.0),
        );
        actions.setRefusesFirstResponder(true);
        view.addSubview(&actions);
        view.setHidden(true);
        Self { view, actions }
    }

    pub(in crate::macos::app) fn render(&self, visible: bool, height: f64, selected: bool) {
        self.view.setHidden(!visible);
        self.view
            .setFrameOrigin(NSPoint::new(WIDTH - 140.0, height - 46.0));
        self.actions.setEnabled(selected);
    }
}
