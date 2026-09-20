use super::super::*;
use winlane::features::files::query::{Kind, Options};

pub(in crate::macos::app) struct Controls {
    pub view: Retained<NSView>,
    pub kind: Retained<NSPopUpButton>,
    pub regex: Retained<NSButton>,
    pub actions: Retained<NSButton>,
}

impl Controls {
    pub(in crate::macos::app) fn new(delegate: &Delegate) -> Self {
        let mtm = delegate.mtm();
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(16.0, 0.0, WIDTH - 32.0, 28.0));
        let kind = NSPopUpButton::initWithFrame_pullsDown(
            NSPopUpButton::alloc(mtm),
            rect(0.0, 1.0, 160.0, 25.0),
            false,
        );
        for kind_value in Kind::ALL {
            kind.addItemWithTitle(&NSString::from_str(kind_value.label()));
        }
        kind.setAccessibilityLabel(Some(&NSString::from_str(tr!("文件类型", "File type"))));
        unsafe {
            kind.setTarget(Some(delegate));
            kind.setAction(Some(sel!(fileKindChanged:)));
        }
        let regex = delegate.button(".*", sel!(fileRegexChanged:), rect(170.0, 1.0, 40.0, 25.0));
        regex.setButtonType(NSButtonType::PushOnPushOff);
        regex.setRefusesFirstResponder(true);
        regex.setToolTip(Some(&NSString::from_str(tr!(
            "正则匹配文件或目录名，例如 \\.pdf$",
            "Match names with a regular expression, e.g. \\.pdf$"
        ))));
        regex.setAccessibilityLabel(Some(&NSString::from_str(tr!(
            "正则表达式",
            "Regular expression"
        ))));
        let actions = delegate.button(
            tr!("操作  ⌃T", "Actions  ⌃T"),
            sel!(showFileActions:),
            rect(WIDTH - 162.0, 1.0, 130.0, 25.0),
        );
        actions.setRefusesFirstResponder(true);
        view.addSubview(&kind);
        view.addSubview(&regex);
        view.addSubview(&actions);
        view.setHidden(true);
        Self {
            view,
            kind,
            regex,
            actions,
        }
    }

    pub(in crate::macos::app) fn render(
        &self,
        visible: bool,
        height: f64,
        options: Options,
        selected: bool,
    ) {
        self.view.setHidden(!visible);
        self.view.setFrameOrigin(NSPoint::new(16.0, height - 94.0));
        self.kind.selectItemAtIndex(
            Kind::ALL
                .iter()
                .position(|kind| *kind == options.kind)
                .unwrap_or(0) as isize,
        );
        self.regex.setState(if options.regex {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        self.actions.setEnabled(selected);
    }
}

impl Delegate {
    pub(in crate::macos::app) fn set_files_kind(&self, index: isize) {
        if !self.searching_files() || self.ivars().syncing_controls.get() {
            return;
        }
        let Some(kind) = usize::try_from(index)
            .ok()
            .and_then(|index| Kind::ALL.get(index))
            .copied()
        else {
            return;
        };
        self.ivars().files.borrow_mut().options.kind = kind;
        self.filter_preserving(self.selected_result());
        self.focus_search();
    }

    pub(in crate::macos::app) fn set_files_regex(&self, enabled: bool) {
        if !self.searching_files() || self.ivars().syncing_controls.get() {
            return;
        }
        self.ivars().files.borrow_mut().options.regex = enabled;
        self.filter_preserving(self.selected_result());
        self.focus_search();
    }
}
