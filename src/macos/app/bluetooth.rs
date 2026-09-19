use super::*;
use crate::macos::platform::bluetooth::{self, Authorization};
use winlane::features::bluetooth::{Device, Update};

impl Delegate {
    pub(super) fn bluetooth_busy(&self) -> bool {
        self.ivars().bluetooth_receiver.borrow().is_some()
            || self.ivars().bluetooth_permission.borrow().is_some()
    }

    pub(super) fn refresh_bluetooth(&self) {
        if !self.searching_bluetooth() || self.bluetooth_busy() {
            return;
        }
        self.cancel_scoped_refresh();
        self.ivars().bluetooth_error.take();
        match bluetooth::authorization() {
            Authorization::Pending => {
                self.ivars()
                    .bluetooth_permission
                    .replace(Some(bluetooth::request_access(self.mtm())));
            }
            Authorization::Denied => {
                self.ivars()
                    .bluetooth_error
                    .replace(Some(bluetooth::permission_error()));
            }
            Authorization::Allowed => self.start_bluetooth_worker(None),
        }
    }

    fn start_bluetooth_worker(&self, request: Option<(String, bool)>) {
        let state = self.ivars();
        let (tx, rx) = mpsc::channel();
        state.bluetooth_receiver.replace(Some(rx));
        state.bluetooth_pending.replace(request.clone());
        state.bluetooth_error.take();
        let wake = state.wake.get().unwrap().handle();
        std::thread::spawn(move || {
            let update = match request {
                Some((address, connected)) => bluetooth::set_connection(&address, connected),
                None => bluetooth::load(),
            };
            let _ = tx.send(update);
            wake.signal();
        });
    }

    pub(super) fn toggle_selected_bluetooth(&self) {
        // Serialize operations and refreshes so stale callbacks cannot replace
        // the result of a newer connection request.
        if self.bluetooth_busy() || self.ivars().scoped_refresh_timer.borrow().is_some() {
            return;
        }
        let Some(device) = self.selected_bluetooth() else {
            return;
        };
        self.start_bluetooth_worker(Some((device.address, !device.connected)));
        self.render();
    }

    pub(super) fn poll_bluetooth(&self) {
        let state = self.ivars();
        if state.bluetooth_permission.borrow().is_some() {
            let authorization = bluetooth::authorization();
            if authorization == Authorization::Pending {
                return;
            }
            state.bluetooth_permission.take();
            if authorization == Authorization::Denied {
                state
                    .bluetooth_error
                    .replace(Some(bluetooth::permission_error()));
                self.render();
            } else if self.searching_bluetooth() {
                self.refresh_bluetooth();
                self.render();
            }
        }
        let result = state
            .bluetooth_receiver
            .borrow()
            .as_ref()
            .map(|rx| rx.try_recv());
        let update = match result {
            Some(Ok(update)) => update,
            Some(Err(TryRecvError::Disconnected)) => Update {
                devices: None,
                error: Some(
                    tr!(
                        "蓝牙操作中断，按 ⌘R 重试。",
                        "Bluetooth operation stopped. Press ⌘R to retry."
                    )
                    .into(),
                ),
            },
            _ => return,
        };
        let selected = self.selected_result();
        state.bluetooth_receiver.take();
        state.bluetooth_pending.take();
        if let Some(devices) = update.devices {
            state.bluetooth_devices.replace(devices);
        }
        state.bluetooth_error.replace(update.error);
        if self.searching_bluetooth() {
            self.filter_preserving(selected);
        }
    }

    pub(super) fn filter_bluetooth(&self, selected: Option<SelectedResult>) {
        let state = self.ivars();
        let devices = winlane::features::bluetooth::matching(
            &state.bluetooth_devices.borrow(),
            &state.query.borrow(),
        );
        let selected = if let Some(SelectedResult::Bluetooth(address)) = selected {
            devices.iter().position(|device| device.address == address)
        } else {
            None
        };
        state.matches.borrow_mut().clear();
        state.launch_matches.borrow_mut().clear();
        state.command_matches.borrow_mut().clear();
        state.snippet_matches.borrow_mut().clear();
        state.clipboard_matches.borrow_mut().clear();
        state.quicklink_matches.borrow_mut().clear();
        state.project_matches.borrow_mut().clear();
        self.clear_open_url_matches();
        state.application_icons.borrow_mut().clear();
        state.bluetooth_matches.replace(devices);
        state.selected.set(selected.unwrap_or(0));
        self.render();
    }

    pub(super) fn selected_bluetooth(&self) -> Option<Device> {
        if !self.searching_bluetooth() {
            return None;
        }
        self.ivars()
            .bluetooth_matches
            .borrow()
            .get(self.ivars().selected.get())
            .cloned()
    }
}

pub(super) fn device_status(device: &Device, pending: Option<bool>) -> &'static str {
    match pending {
        Some(true) => tr!("正在连接…", "Connecting…"),
        Some(false) => tr!("正在断开…", "Disconnecting…"),
        None if device.connected => tr!("已连接", "Connected"),
        None => tr!("未连接", "Disconnected"),
    }
}
