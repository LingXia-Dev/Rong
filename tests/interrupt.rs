use rong_test::*;

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

fn engine_preemption() -> bool {
    let runtime = RongJS::runtime();
    runtime
        .interrupt_handle()
        .engine_preemption()
        .expect("preemption support must be known once the runtime exists")
}

#[cfg(any(
    feature = "quickjs",
    feature = "jscore-interrupt",
    feature = "jscore-source",
    all(feature = "jscore", not(any(target_os = "macos", target_os = "ios")))
))]
#[test]
fn configured_engine_reports_native_preemption() {
    assert!(
        engine_preemption(),
        "this feature configuration promises native preemption"
    );
}

#[test]
fn busy_loop_is_preempted() {
    if !engine_preemption() {
        eprintln!("skipping: engine has no preemption support");
        return;
    }

    let worker =
        on_runtime_thread(|ctx| ctx.eval::<JSValue>(Source::from_bytes(BUSY_LOOP)).is_err());

    worker.wait_until_js_started();
    worker.interrupt.interrupt();
    let errored = worker.finish();
    assert!(errored, "interrupted busy loop must surface an error");
}

#[test]
fn busy_loop_cannot_catch_the_interruption() {
    if !engine_preemption() {
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
    assert!(
        error.to_string().contains("interrupted"),
        "unexpected error: {error:?}"
    );
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
    if !engine_preemption() {
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
    if !engine_preemption() {
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
