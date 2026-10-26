//! QuickJS and JavaScriptCore linked into one binary, each driven by its own types.
#![cfg(all(feature = "quickjs", feature = "jscore"))]

use rong::{InterruptMode, JSEngine, Source, jscore, quickjs};
use rong_core::engine::JSContextImpl;
use std::any::TypeId;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const SUM: &[u8] =
    b"(() => { let s = 0; for (let i = 0; i < 100000; i++) s = (s + i) % 9973; return s; })()";
const DEADLINE: Duration = Duration::from_secs(10);

#[test]
fn both_engines_evaluate_in_one_process() {
    let qjs_runtime = quickjs::RongJS::runtime();
    let qjs: quickjs::JSContext = qjs_runtime.context();
    let jsc_runtime = jscore::RongJS::runtime();
    let jsc: jscore::JSContext = jsc_runtime.context();

    let a: i32 = qjs.eval(Source::from_bytes(SUM)).unwrap();
    let b: i32 = jsc.eval(Source::from_bytes(SUM)).unwrap();
    assert_eq!(a, b);

    // Each engine keeps its own globals.
    qjs.eval::<()>(Source::from_bytes(b"globalThis.owner = 'quickjs'"))
        .unwrap();
    jsc.eval::<()>(Source::from_bytes(b"globalThis.owner = 'jscore'"))
        .unwrap();
    let owner: String = qjs.eval(Source::from_bytes(b"owner")).unwrap();
    assert_eq!(owner, "quickjs");
    let owner: String = jsc.eval(Source::from_bytes(b"owner")).unwrap();
    assert_eq!(owner, "jscore");

    let array: quickjs::JSArray = qjs.eval(Source::from_bytes(b"[1, 2, 3]")).unwrap();
    assert_eq!(array.len().unwrap(), 3);
    let array: jscore::JSArray = jsc.eval(Source::from_bytes(b"[4, 5]")).unwrap();
    assert_eq!(array.len().unwrap(), 2);
}

#[cfg(not(any(feature = "primary-quickjs", feature = "primary-arkjs")))]
#[test]
fn the_platform_engine_stays_primary() {
    assert_eq!(TypeId::of::<rong::RongJS>(), TypeId::of::<jscore::RongJS>());
    assert_eq!(
        TypeId::of::<rong::JSContext>(),
        TypeId::of::<jscore::JSContext>()
    );
}

#[cfg(feature = "primary-quickjs")]
#[test]
fn a_primary_feature_pins_the_root_aliases() {
    assert_eq!(
        TypeId::of::<rong::RongJS>(),
        TypeId::of::<quickjs::RongJS>()
    );
    assert_eq!(
        TypeId::of::<rong::JSValue>(),
        TypeId::of::<quickjs::JSValue>()
    );
}

/// Interrupting a QuickJS runaway loop leaves a JSC runtime on another thread untouched.
#[test]
fn quickjs_hard_interrupt_leaves_jscore_alone() {
    let (handle_tx, handle_rx) = mpsc::channel();
    let (started_tx, started_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let qjs_thread = thread::spawn(move || {
        let runtime = quickjs::RongJS::runtime();
        assert_eq!(runtime.interrupt_handle().mode(), InterruptMode::Preemptive);
        let ctx = runtime.context();
        ctx.global()
            .set(
                "started",
                quickjs::JSFunc::new(&ctx, move || {
                    let _ = started_tx.send(());
                })
                .unwrap(),
            )
            .unwrap();
        handle_tx.send(runtime.interrupt_handle()).unwrap();
        let result = ctx.eval::<()>(Source::from_bytes(b"started(); while (true) {}"));
        result_tx.send(result.is_err()).unwrap();
    });

    let interrupt = handle_rx.recv_timeout(DEADLINE).unwrap();
    started_rx.recv_timeout(DEADLINE).unwrap();

    let jsc_runtime = jscore::RongJS::runtime();
    let jsc = jsc_runtime.context();
    interrupt.interrupt();
    assert!(
        result_rx.recv_timeout(DEADLINE).unwrap(),
        "an interrupted QuickJS eval must fail"
    );
    qjs_thread.join().unwrap();

    assert!(!jsc_runtime.interrupt_handle().is_interrupted());
    let sum: i32 = jsc.eval(Source::from_bytes(SUM)).unwrap();
    let expected: i32 = quickjs::RongJS::runtime()
        .context()
        .eval(Source::from_bytes(SUM))
        .unwrap();
    assert_eq!(sum, expected);
}

/// A self-rescheduling microtask chain is drained a bounded number of jobs at a
/// time on QuickJS, then cut off by interruption; JSC's own jobs still settle.
#[test]
fn quickjs_bounded_jobs_beside_jscore() {
    use rong_quickjs_sys as qjs;

    let runtime = quickjs::RongJS::runtime();
    let ctx = runtime.context();
    ctx.eval::<()>(Source::from_bytes(
        b"globalThis.ticks = 0; (function tick() { ticks++; Promise.resolve().then(tick); })();",
    ))
    .unwrap();

    let raw_runtime = unsafe { qjs::JS_GetRuntime(*ctx.as_ref().as_raw()) };
    let execute = || unsafe {
        let mut owner = std::ptr::null_mut();
        let rc = qjs::JS_ExecutePendingJob(raw_runtime, &mut owner);
        if rc < 0 && !owner.is_null() {
            qjs::JS_FreeValue(owner, qjs::JS_GetException(owner));
        }
        rc
    };

    const BUDGET: usize = 1000;
    let mut ran = 0;
    while unsafe { qjs::JS_IsJobPending(raw_runtime) } && ran < BUDGET {
        assert_eq!(execute(), 1);
        ran += 1;
    }
    assert_eq!(ran, BUDGET);
    assert!(unsafe { qjs::JS_IsJobPending(raw_runtime) });
    let ticks: i32 = ctx.eval(Source::from_bytes(b"ticks")).unwrap();
    assert_eq!(ticks as usize, BUDGET + 1);

    let jsc_runtime = jscore::RongJS::runtime();
    let jsc = jsc_runtime.context();
    jsc.eval::<()>(Source::from_bytes(
        b"globalThis.settled = 0; Promise.resolve().then(() => { settled = 42; });",
    ))
    .unwrap();

    // Interrupted, every pending job fails instead of rescheduling, so the queue empties.
    let _interrupt = runtime.interrupt_handle().interrupt_scoped();
    let started = Instant::now();
    while unsafe { qjs::JS_IsJobPending(raw_runtime) } {
        assert!(
            started.elapsed() < DEADLINE,
            "interrupted jobs must stop rescheduling"
        );
        execute();
    }

    let settled: i32 = jsc.eval(Source::from_bytes(b"settled")).unwrap();
    assert_eq!(settled, 42);
}
