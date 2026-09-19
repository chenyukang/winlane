use super::*;

pub(crate) fn pending(
    origin_pid: i32,
) -> (
    PendingProjectOpen,
    mpsc::Sender<Result<OpenedProject, String>>,
    Arc<AtomicBool>,
) {
    let (tx, receiver) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    (
        PendingProjectOpen {
            receiver,
            origin_pid,
            target_seen: false,
            cancelled: cancelled.clone(),
        },
        tx,
        cancelled,
    )
}
