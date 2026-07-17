#![allow(clippy::missing_const_for_thread_local)]

use std::cell::Cell;
use std::future::Future;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Notify;
use tracing::{Span, info, warn};

// The initializer is already const, but cross-target Clippy reports
// `missing_const_for_thread_local` for the OHOS target.
thread_local! {
    static CURRENT_WORKER_ID: Cell<Option<usize>> = const { Cell::new(None) };
}

pub(crate) fn in_worker_thread() -> bool {
    CURRENT_WORKER_ID.with(|slot| slot.get()).is_some()
}

pub(crate) fn spawn_js_worker_thread<F, Fut>(
    worker_id: usize,
    thread_name: String,
    worker_span: Span,
    start_log: &'static str,
    stop_log: &'static str,
    ready_tx: std::sync::mpsc::Sender<Result<(), String>>,
    run: F,
) -> std::thread::JoinHandle<()>
where
    F: FnOnce(std::sync::mpsc::Sender<Result<(), String>>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + 'static,
{
    std::thread::spawn(move || {
        let _entered = worker_span.enter();
        info!(target: "rong", "{start_log}");

        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .thread_name(thread_name)
            .build()
        {
            Ok(rt) => rt,
            Err(err) => {
                let _ = ready_tx.send(Err(err.to_string()));
                return;
            }
        };

        CURRENT_WORKER_ID.with(|slot| slot.set(Some(worker_id)));
        rt.block_on(run(ready_tx));
        CURRENT_WORKER_ID.with(|slot| slot.set(None));

        info!(target: "rong", "{stop_log}");
    })
}

pub(crate) fn shutdown_worker_threads(
    mut join_next: impl FnMut() -> Option<(usize, std::thread::JoinHandle<()>)>,
    current_thread_skip_log: &'static str,
    panic_log: &'static str,
) {
    while let Some((worker_id, handle)) = join_next() {
        if handle.thread().id() == std::thread::current().id() {
            warn!(
                target: "rong",
                worker_id,
                "{current_thread_skip_log}"
            );
            continue;
        }

        if let Err(err) = handle.join() {
            warn!(
                target: "rong",
                worker_id,
                error = ?err,
                "{panic_log}"
            );
        }
    }
}

pub(crate) fn terminate_signal() -> Arc<Notify> {
    Arc::new(Notify::new())
}

pub(crate) fn take_thread_handle(
    handle: &Arc<StdMutex<Option<std::thread::JoinHandle<()>>>>,
) -> Option<std::thread::JoinHandle<()>> {
    let mut guard = handle.lock().unwrap();
    guard.take()
}
