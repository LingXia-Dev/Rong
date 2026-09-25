#![allow(clippy::missing_const_for_thread_local)]

use crate::{JSRuntime, JSRuntimeImpl};
use std::cell::Cell;
use std::future::Future;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::sync::Notify;
use tracing::{Instrument, Span, info, warn};

// The initializer is already const, but cross-target Clippy reports
// `missing_const_for_thread_local` for the OHOS target.
thread_local! {
    static CURRENT_WORKER_ID: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Shortest wait between two runs of the engine's deferred work, so work that
/// is due again right away cannot spin the worker.
const ENGINE_TIMERS_MIN_WAIT: Duration = Duration::from_millis(10);

/// Longest wait between two runs of the engine's deferred work. The engine's
/// own delay is not enough: JavaScriptCore arms its timers from its GC and JIT
/// threads too, while this worker sleeps on the delay it read before, so the
/// worker looks again at least this often.
const ENGINE_TIMERS_MAX_WAIT: Duration = Duration::from_secs(1);

/// How long an idle worker waits before running the engine's deferred work
/// again, given the delay the engine reported.
fn engine_timers_wait(due: Duration) -> Duration {
    due.clamp(ENGINE_TIMERS_MIN_WAIT, ENGINE_TIMERS_MAX_WAIT)
}

/// Runs an engine's deferred work (see [`JSRuntimeImpl::run_engine_timers`])
/// from a worker's local task set: when the engine says it is due, and right
/// after each task. It runs between polls of the worker's tasks, never while
/// JavaScript is on the stack.
pub(crate) struct EngineTimers {
    wake: Arc<Notify>,
    task: tokio::task::JoinHandle<()>,
}

impl EngineTimers {
    /// Starts the loop on the current local task set, or returns `None` when
    /// the engine schedules no deferred work.
    pub(crate) fn start<R: JSRuntimeImpl + 'static>(
        runtime: &JSRuntime<R>,
        span: Span,
    ) -> Option<Self> {
        let first = runtime.run_engine_timers()?;
        let runtime = runtime.clone();
        let wake = Arc::new(Notify::new());
        let woken = wake.clone();
        let task = tokio::task::spawn_local(
            async move {
                let mut wait = engine_timers_wait(first);
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep(wait) => {}
                        _ = woken.notified() => {}
                    }
                    let due = runtime
                        .run_engine_timers()
                        .unwrap_or(ENGINE_TIMERS_MAX_WAIT);
                    wait = engine_timers_wait(due);
                }
            }
            .instrument(span),
        );
        Some(Self { wake, task })
    }

    /// Runs the engine's deferred work once more as soon as the worker is
    /// free. Workers call this when a task completes, because a task can
    /// leave work due before the next timed run (a dropped context, say).
    pub(crate) fn wake(&self) {
        self.wake.notify_one();
    }

    /// Stops the loop and drops its runtime handle. Await this before the
    /// worker drops its runtime, so the engine is torn down with nothing of
    /// the loop left alive.
    pub(crate) async fn stop(self) {
        self.task.abort();
        let _ = self.task.await;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_timers_wait_is_clamped() {
        assert_eq!(engine_timers_wait(Duration::ZERO), ENGINE_TIMERS_MIN_WAIT);
        assert_eq!(
            engine_timers_wait(Duration::from_millis(3)),
            ENGINE_TIMERS_MIN_WAIT
        );
        assert_eq!(
            engine_timers_wait(Duration::from_millis(250)),
            Duration::from_millis(250)
        );
        assert_eq!(
            engine_timers_wait(Duration::from_secs(30)),
            ENGINE_TIMERS_MAX_WAIT
        );
        assert_eq!(engine_timers_wait(Duration::MAX), ENGINE_TIMERS_MAX_WAIT);
    }
}
