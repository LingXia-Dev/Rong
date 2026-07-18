use rong_test::*;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const START_FUNCTION: &str = "__rongInterruptTestStarted";
const BUSY_LOOP: &str = "__rongInterruptTestStarted(); while (true) {}";
/// Generous bound: interruption should land within the engine's polling
/// quantum (QuickJS counts instructions, JSC's watchdog fires every 100ms).
const PREEMPT_DEADLINE: Duration = Duration::from_secs(10);

struct RuntimeThread<T> {
    interrupt: InterruptHandle,
    started_rx: mpsc::Receiver<()>,
    result_rx: mpsc::Receiver<T>,
    join: thread::JoinHandle<()>,
}

impl<T> RuntimeThread<T> {
    fn wait_until_js_started(&self) {
        self.started_rx
            .recv_timeout(PREEMPT_DEADLINE)
            .expect("JavaScript must enter the busy-loop fixture");
    }

    fn finish(self) -> T {
        let result = self
            .result_rx
            .recv_timeout(PREEMPT_DEADLINE)
            .expect("runtime thread must finish after interruption");
        self.join.join().expect("runtime thread must not panic");
        result
    }
}

fn install_start_notifier(ctx: &JSContext, started_tx: mpsc::Sender<()>) -> JSResult<()> {
    ctx.global().set(
        START_FUNCTION,
        JSFunc::new(ctx, move || {
            let _ = started_tx.send(());
        })?,
    )?;
    Ok(())
}

/// Run `f` with a fresh runtime on a dedicated thread. The installed host
/// callback signals only after eval's cooperative pre-check has completed and
/// JavaScript is about to enter the busy loop.
fn on_runtime_thread<T: Send + 'static>(
    f: impl FnOnce(&JSContext) -> T + Send + 'static,
) -> RuntimeThread<T> {
    let (interrupt_tx, interrupt_rx) = mpsc::channel();
    let (started_tx, started_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let join = thread::spawn(move || {
        let runtime = RongJS::runtime();
        let ctx = runtime.context();
        install_start_notifier(&ctx, started_tx).expect("start notifier must be installed");
        interrupt_tx
            .send(runtime.interrupt_handle())
            .expect("interrupt receiver must remain alive");
        let _ = result_tx.send(f(&ctx));
    });
    RuntimeThread {
        interrupt: interrupt_rx
            .recv_timeout(PREEMPT_DEADLINE)
            .expect("runtime must publish its interrupt handle"),
        started_rx,
        result_rx,
        join,
    }
}

fn engine_is_preemptive() -> bool {
    let runtime = RongJS::runtime();
    runtime.interrupt_handle().mode().is_preemptive()
}

#[cfg(any(
    feature = "quickjs",
    feature = "jscore-interrupt",
    feature = "jscore-source",
    all(feature = "jscore", not(any(target_os = "macos", target_os = "ios")))
))]
#[test]
fn configured_engine_reports_native_preemption() {
    let runtime = RongJS::runtime();
    assert_eq!(
        runtime.interrupt_handle().mode(),
        InterruptMode::Preemptive,
        "this feature configuration promises native preemption"
    );
}

#[test]
fn interrupt_mode_and_scoped_requests_are_explicit() {
    let unbound = InterruptHandle::new();
    assert_eq!(unbound.mode(), InterruptMode::Unbound);

    let runtime = RongJS::runtime();
    assert_ne!(runtime.interrupt_handle().mode(), InterruptMode::Unbound);

    let interrupt = InterruptHandle::new();
    let first = interrupt.interrupt_scoped();
    let second = interrupt.interrupt_scoped();
    assert!(interrupt.is_interrupted());

    drop(first);
    assert!(
        interrupt.is_interrupted(),
        "one owner must not clear another owner's request"
    );

    interrupt.interrupt();
    drop(second);
    assert!(
        interrupt.is_interrupted(),
        "dropping a scoped request must preserve a persistent request"
    );

    interrupt.clear();
    assert!(!interrupt.is_interrupted());
}

#[test]
fn busy_loop_is_preempted() {
    if !engine_is_preemptive() {
        eprintln!("skipping: engine has no preemption support");
        return;
    }

    let worker = on_runtime_thread(|ctx| {
        ctx.eval::<JSValue>(Source::from_bytes(BUSY_LOOP))
            .expect_err("busy loop must be interrupted")
            .is_interrupted()
    });

    worker.wait_until_js_started();
    worker.interrupt.interrupt();
    let errored = worker.finish();
    assert!(errored, "interrupted busy loop must surface an error");
}

#[test]
fn busy_loop_cannot_catch_the_interruption() {
    if !engine_is_preemptive() {
        eprintln!("skipping: engine has no preemption support");
        return;
    }

    // The termination error must not be swallowed by JS-level try/catch.
    let worker = on_runtime_thread(|ctx| {
        ctx.eval::<JSValue>(Source::from_bytes(
            "try { __rongInterruptTestStarted(); while (true) {} } catch (e) { 'caught' }",
        ))
        .is_err()
    });

    worker.wait_until_js_started();
    worker.interrupt.interrupt();
    assert!(worker.finish());
}

#[test]
fn interrupted_runtime_rejects_new_evaluations() {
    // Cooperative layer: applies on every engine, preemption or not.
    let runtime = RongJS::runtime();
    let ctx = runtime.context();
    let interrupt = runtime.interrupt_handle();

    interrupt.interrupt();
    let error = ctx
        .eval::<JSValue>(Source::from_bytes("1 + 1"))
        .expect_err("eval while interrupted must fail");
    assert!(error.is_interrupted(), "unexpected error: {error:?}");
    assert_eq!(error.code(), Some(error::E_INTERRUPTED));
}

#[test]
fn clear_resumes_execution() {
    let runtime = RongJS::runtime();
    let ctx = runtime.context();
    let interrupt = runtime.interrupt_handle();

    interrupt.interrupt();
    assert!(ctx.eval::<JSValue>(Source::from_bytes("1 + 1")).is_err());

    interrupt.clear();
    let result: i32 = ctx
        .eval(Source::from_bytes("40 + 2"))
        .expect("eval after clear must succeed");
    assert_eq!(result, 42);
}

#[test]
fn preempted_runtime_recovers_after_clear() {
    if !engine_is_preemptive() {
        eprintln!("skipping: engine has no preemption support");
        return;
    }

    let (preempted_tx, preempted_rx) = mpsc::channel();
    let worker = on_runtime_thread(move |ctx| {
        // First eval is preempted…
        let preempted = ctx.eval::<JSValue>(Source::from_bytes(BUSY_LOOP)).is_err();
        let _ = preempted_tx.send(preempted);
        // …then, once cleared, the same context keeps working.
        loop {
            match ctx.eval::<i32>(Source::from_bytes("6 * 7")) {
                Ok(value) => return value,
                Err(_) => thread::sleep(Duration::from_millis(10)),
            }
        }
    });

    worker.wait_until_js_started();
    worker.interrupt.interrupt();
    assert!(
        preempted_rx
            .recv_timeout(PREEMPT_DEADLINE)
            .expect("the first eval must finish after interruption")
    );
    worker.interrupt.clear();

    assert_eq!(worker.finish(), 42);
}

#[test]
fn worker_interrupt_breaks_busy_task_and_recovers() {
    if !engine_is_preemptive() {
        eprintln!("skipping: engine has no preemption support");
        return;
    }

    let rong = Rong::<RongJS>::builder()
        .shared()
        .workers(1)
        .build()
        .unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let worker = rong.worker(0).unwrap();
        let interrupt = worker.interrupt_handle();

        // Exercise the same worker twice so clear/re-arm behavior is covered.
        for attempt in 1..=2 {
            let (started_tx, started_rx) = mpsc::channel();
            let handle = worker
                .spawn(
                    async move |runtime: JSRuntime, _receiver| -> JSResult<bool> {
                        let ctx = runtime.context();
                        install_start_notifier(&ctx, started_tx)?;
                        Ok(ctx.eval::<JSValue>(Source::from_bytes(BUSY_LOOP)).is_err())
                    },
                )
                .await
                .unwrap();

            if started_rx.recv_timeout(PREEMPT_DEADLINE).is_err() {
                interrupt.interrupt();
                std::mem::forget(rong);
                panic!("worker busy-loop fixture {attempt} did not start");
            }
            interrupt.interrupt();

            let preempted = match tokio::time::timeout(PREEMPT_DEADLINE, handle.join()).await {
                Ok(result) => result.unwrap(),
                Err(_) => {
                    // A broken native hook leaves the worker in non-yielding JS;
                    // leaking the failed pool keeps test teardown from joining it.
                    std::mem::forget(rong);
                    panic!("worker busy task {attempt} survived interruption");
                }
            };
            assert!(preempted);
            interrupt.clear();
        }

        // The worker survives repeated interruption and runs another task.
        let value = worker
            .call(
                async move |runtime: JSRuntime, _receiver| -> JSResult<i32> {
                    let ctx = runtime.context();
                    ctx.eval::<i32>(Source::from_bytes("6 * 7"))
                },
            )
            .await
            .unwrap();
        assert_eq!(value, 42);
    });
}

#[test]
fn worker_timeout_is_typed_scoped_and_recoverable() {
    let rong = Rong::<RongJS>::builder()
        .shared()
        .workers(1)
        .build()
        .unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let worker = rong.worker(0).unwrap();
        assert_eq!(worker.interrupt_mode(), worker.interrupt_handle().mode());

        let error = worker
            .call_with_timeout(
                Duration::from_millis(50),
                async move |_runtime: JSRuntime, _receiver| -> JSResult<()> {
                    std::future::pending::<()>().await;
                    Ok(())
                },
            )
            .await
            .expect_err("pending task must time out");
        assert!(error.is_timeout(), "unexpected error: {error:?}");
        assert_eq!(error.code(), Some(error::E_TIMEOUT));

        let value = tokio::time::timeout(
            PREEMPT_DEADLINE,
            worker.call(
                async move |runtime: JSRuntime, _receiver| -> JSResult<i32> {
                    runtime.context().eval::<i32>(Source::from_bytes("40 + 2"))
                },
            ),
        )
        .await
        .expect("worker must become reusable after timeout")
        .unwrap();
        assert_eq!(value, 42);
        assert!(!worker.interrupt_handle().is_interrupted());
    });
}

#[test]
fn queued_timeout_does_not_interrupt_the_running_task() {
    let rong = Rong::<RongJS>::builder()
        .shared()
        .workers(1)
        .build()
        .unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let worker = rong.worker(0).unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let running = worker
            .spawn(
                async move |_runtime: JSRuntime, _receiver| -> JSResult<i32> {
                    let _ = started_tx.send(());
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok(7)
                },
            )
            .await
            .unwrap();
        started_rx
            .recv_timeout(PREEMPT_DEADLINE)
            .expect("first task must be running");

        let queued_ran = Arc::new(AtomicBool::new(false));
        let queued_ran_in_task = queued_ran.clone();
        let error = worker
            .call_with_timeout(
                Duration::from_millis(20),
                async move |_runtime: JSRuntime, _receiver| -> JSResult<()> {
                    queued_ran_in_task.store(true, Ordering::SeqCst);
                    Ok(())
                },
            )
            .await
            .expect_err("queued task must time out");
        assert!(error.is_timeout());

        assert_eq!(running.join().await.unwrap(), 7);
        worker.join().await.unwrap();
        assert!(
            !queued_ran.load(Ordering::SeqCst),
            "a task that timed out in the queue must never start"
        );
    });
}

#[test]
fn pool_timeout_includes_waiting_for_an_idle_worker() {
    let rong = Rong::<RongJS>::builder()
        .shared()
        .workers(1)
        .build()
        .unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let worker = rong.worker(0).unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let running = worker
            .spawn(
                async move |_runtime: JSRuntime, _receiver| -> JSResult<()> {
                    let _ = started_tx.send(());
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok(())
                },
            )
            .await
            .unwrap();
        started_rx
            .recv_timeout(PREEMPT_DEADLINE)
            .expect("worker must be occupied");

        let queued_ran = Arc::new(AtomicBool::new(false));
        let queued_ran_in_task = queued_ran.clone();
        let error = rong
            .call_with_timeout(
                Duration::from_millis(20),
                async move |_runtime: JSRuntime, _receiver| -> JSResult<()> {
                    queued_ran_in_task.store(true, Ordering::SeqCst);
                    Ok(())
                },
            )
            .await
            .expect_err("pool dispatch wait must count toward the timeout");
        assert!(error.is_timeout());

        running.join().await.unwrap();
        rong.join().await.unwrap();
        assert!(!queued_ran.load(Ordering::SeqCst));
        assert_eq!(rong.free_workers_count(), 1);
    });
}

#[test]
fn pinned_timeout_is_typed_and_recoverable() {
    let rong = Rong::<RongJS>::builder()
        .pinned::<u64, usize>()
        .workers(1)
        .build()
        .unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let error = rong
            .call_with_timeout(
                Duration::from_millis(50),
                7,
                async move |_runtime: JSRuntime, _key, state, _receiver| {
                    std::future::pending::<()>().await;
                    (Ok::<(), RongJSError>(()), state)
                },
            )
            .await
            .expect_err("pending pinned task must time out");
        assert!(error.is_timeout());

        let value = tokio::time::timeout(
            PREEMPT_DEADLINE,
            rong.call(7, async move |_runtime, _key, state, _receiver| {
                let value = state.unwrap_or_default() + 1;
                (Ok::<usize, RongJSError>(value), Some(value))
            }),
        )
        .await
        .expect("pinned worker must become reusable after timeout")
        .unwrap();
        assert_eq!(value, 1);
    });
}

#[cfg(feature = "quickjs")]
#[test]
fn interrupted_runtime_drop_preempts_pending_jobs() {
    let (done_tx, done_rx) = mpsc::channel();
    let join = thread::spawn(move || {
        let runtime = RongJS::runtime();
        let ctx = runtime.context();
        ctx.eval::<JSValue>(Source::from_bytes(
            "void Promise.resolve().then(() => { while (true) {} });",
        ))
        .expect("pending busy-loop job must be queued");

        runtime.interrupt_handle().interrupt();
        drop(ctx);
        drop(runtime);
        let _ = done_tx.send(());
    });

    done_rx
        .recv_timeout(PREEMPT_DEADLINE)
        .expect("interrupted runtime teardown must not hang in a pending job");
    join.join().expect("runtime teardown thread must not panic");
}
