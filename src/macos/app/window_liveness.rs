use super::*;
use crate::macos::platform::window_server::Inventory;

impl Delegate {
    pub(super) fn check_window_liveness(&self) {
        let state = self.ivars();
        if state.demo.get() || state.window_server_ids.borrow().is_empty() {
            return;
        }
        if state.window_check_receiver.borrow().is_some() {
            state.window_check_again.set(true);
            return;
        }
        let ids = state.window_server_ids.borrow().clone();
        let (tx, rx) = mpsc::channel();
        state.window_check_receiver.replace(Some(rx));
        let wake = state.wake.get().unwrap().handle();
        std::thread::spawn(move || {
            let closed = Inventory::try_read().map_or_else(Vec::new, |inventory| {
                ids.into_iter()
                    .filter_map(|(id, server_id)| (!inventory.contains(server_id)).then_some(id))
                    .collect()
            });
            let _ = tx.send(closed);
            wake.signal();
        });
    }

    pub(super) fn poll_window_liveness(&self) {
        let state = self.ivars();
        let result = state
            .window_check_receiver
            .borrow()
            .as_ref()
            .map(|rx| rx.try_recv());
        match result {
            None | Some(Err(TryRecvError::Empty)) => return,
            Some(Ok(closed)) => self.remove_closed_windows(&closed),
            Some(Err(TryRecvError::Disconnected)) => {}
        }
        state.window_check_receiver.take();
        if state.window_check_again.replace(false) {
            self.check_window_liveness();
        }
    }

    pub(super) fn remove_closed_windows(&self, closed: &[u64]) {
        if closed.is_empty() {
            return;
        }
        let state = self.ivars();
        let closed: HashSet<_> = closed
            .iter()
            .copied()
            .filter(|id| {
                state.window_server_ids.borrow().contains_key(id)
                    || state.windows.borrow().iter().any(|w| w.id == *id)
                    || state
                        .deferred_windows
                        .borrow()
                        .as_ref()
                        .is_some_and(|windows| windows.iter().any(|w| w.id == *id))
            })
            .collect();
        // AX destruction notifications include ordinary controls, not just windows.
        if closed.is_empty() {
            return;
        }
        // A slower AX scan may still publish a snapshot taken before the close.
        if state.receiver.borrow().is_some() {
            state.closed_windows.borrow_mut().extend(&closed);
        }
        state
            .window_server_ids
            .borrow_mut()
            .retain(|id, _| !closed.contains(id));
        if let Some(windows) = state.deferred_windows.borrow_mut().as_mut() {
            windows.retain(|window| !closed.contains(&window.id));
        }
        let windows = state.windows.borrow();
        if !windows.iter().any(|window| closed.contains(&window.id)) {
            return;
        }
        let selected = self.selected_result();
        let keep: Vec<_> = state
            .matches
            .borrow()
            .iter()
            .map(|&index| !closed.contains(&windows[index].id))
            .collect();
        let order: Vec<_> = state
            .matches
            .borrow()
            .iter()
            .filter_map(|&index| {
                (!closed.contains(&windows[index].id)).then_some(windows[index].id)
            })
            .collect();
        let remaining: Vec<_> = windows
            .iter()
            .filter(|window| !closed.contains(&window.id))
            .cloned()
            .collect();
        drop(windows);
        self.install_windows(remaining);
        if state.mode.get() == Some(PanelMode::Switch) {
            let windows = state.windows.borrow();
            let indices: HashMap<_, _> = windows
                .iter()
                .enumerate()
                .map(|(index, w)| (w.id, index))
                .collect();
            state
                .matches
                .replace(order.iter().map(|id| indices[id]).collect());
            if let Some(selection) = state.switch_selection.borrow_mut().as_mut() {
                selection.select(state.selected.get());
                selection.retain(&keep);
                state.selected.set(selection.selected().unwrap_or(0));
            }
            drop(windows);
            self.select_alias();
            self.render();
        } else {
            self.filter_preserving(selected);
        }
    }
}
