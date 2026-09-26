use super::*;
use crate::macos::platform::main_wake::WakeHandle;

/// Background loading for one scoped panel. Owns the in-flight receiver so a
/// second load is refused while the first runs and a late reply is dropped once
/// the user leaves the scope.
pub(super) struct ScopedAsync<T> {
    receiver: RefCell<Option<Receiver<T>>>,
}

impl<T> Default for ScopedAsync<T> {
    fn default() -> Self {
        Self {
            receiver: RefCell::new(None),
        }
    }
}

impl<T: Send + 'static> ScopedAsync<T> {
    pub(super) fn is_loading(&self) -> bool {
        self.receiver.borrow().is_some()
    }

    /// Run `load` on a worker unless one is already in flight. Returns whether a
    /// worker was started, so the caller can show a loading state.
    pub(super) fn start(
        &self,
        wake: &WakeHandle,
        load: impl FnOnce() -> T + Send + 'static,
    ) -> bool {
        if self.receiver.borrow().is_some() {
            return false;
        }
        let (tx, rx) = mpsc::channel();
        self.receiver.replace(Some(rx));
        let wake = wake.clone();
        std::thread::spawn(move || {
            let _ = tx.send(load());
            wake.signal();
        });
        true
    }

    /// Take a finished result. `Ok` carries the value, `Err` means the worker
    /// went away without sending, and `None` means the load is still running.
    pub(super) fn poll(&self) -> Option<Result<T, ()>> {
        let result = self.receiver.borrow().as_ref().map(|rx| rx.try_recv());
        match result {
            Some(Ok(value)) => {
                self.receiver.take();
                Some(Ok(value))
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.receiver.take();
                Some(Err(()))
            }
            _ => None,
        }
    }

    /// Abandon the in-flight load; a late reply is dropped.
    pub(super) fn cancel(&self) {
        self.receiver.take();
    }
}

impl Delegate {
    /// Drop the cached rows for one scope so a stale list is not shown after
    /// leaving it, and clear their accessibility labels.
    pub(super) fn clear_scope_rows(&self, is_scope: impl Fn(&RowContent) -> bool) {
        for ui in self.panels() {
            for row in ui.rows.borrow_mut().iter_mut() {
                if row.content.as_ref().is_some_and(&is_scope) {
                    row.content = None;
                    row.app.setStringValue(&NSString::from_str(""));
                    row.title.setStringValue(&NSString::from_str(""));
                    row.button.setToolTip(None);
                    row.button.setAccessibilityLabel(None);
                }
            }
        }
    }
}
