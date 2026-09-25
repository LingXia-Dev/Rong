//! A worker reclaims what its dropped contexts held, with nothing but its
//! own idle work.
//!
//! JavaScriptCore collects and sweeps from timers on the thread's run loop,
//! which the workers service only on Apple's system framework. ArkJS has not
//! been verified on a device yet, and source/JSCOnly builds keep collecting
//! only under allocation, so neither runs these tests.
#![cfg(any(
    feature = "quickjs",
    all(
        feature = "jscore",
        target_vendor = "apple",
        not(feature = "jscore-source")
    )
))]

use rong_macro::js_class;
use rong_test::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const CONTEXTS: usize = 16;
const PROBES_PER_CONTEXT: usize = 2;
const PROBES: usize = CONTEXTS * PROBES_PER_CONTEXT;

/// Engines keep some garbage alive for a while (JavaScriptCore scans stacks
/// conservatively), so the tests ask for half, well above the handful an
/// engine frees without its scheduled collections.
const EXPECTED: usize = PROBES / 2;
const DEADLINE: Duration = Duration::from_secs(90);
const POLL: Duration = Duration::from_millis(50);
const TASK_TIMEOUT: Duration = Duration::from_secs(30);

/// QuickJS collects cycles only under allocation or on `run_gc`, so the tests
/// ask for it. JavaScriptCore's `run_gc` does nothing, and there the tests
/// leave the worker idle: its own timers are what is under test.
const COLLECT_EXPLICITLY: bool = cfg!(feature = "quickjs");

/// Drop counts, one per test, since the tests run in parallel.
static DROPS: [AtomicUsize; 2] = [AtomicUsize::new(0), AtomicUsize::new(0)];
const SHARED: usize = 0;
const PINNED: usize = 1;

#[js_class]
struct Probe {
    counter: usize,
    held: Option<JSObject>,
}

impl Drop for Probe {
    fn drop(&mut self) {
        DROPS[self.counter].fetch_add(1, Ordering::SeqCst);
    }
}

impl JSClass<JSEngineValue> for Probe {
    const NAME: &'static str = "Probe";

    fn data_constructor() -> Constructor<JSEngineValue> {
        Constructor::new(|counter: u32| Probe {
            counter: counter as usize,
            held: None,
        })
    }

    fn class_setup(class: &ClassSetup<JSEngineValue>) -> JSResult<()> {
        class.method(
            "hold",
            |this: ThisMut<Probe>, value: JSObject| -> JSResult<()> {
                this.borrow_mut()?.held = Some(value);
                Ok(())
            },
        )?;
        Ok(())
    }

    fn gc_mark_with<F>(&self, mut mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
        if let Some(held) = &self.held {
            mark_fn(held.as_js_value());
        }
    }
}

/// Create `CONTEXTS` contexts one after another, each with two probes, and
/// drop each one.
///
/// One probe is reachable from the global object, in a closure cycle. The
/// other is unreachable but holds a value of its context from Rust, which on
/// JavaScriptCore keeps the whole context alive until the engine finalizes
/// the probe: a full collection and a sweep have to run before the context
/// can go.
fn retire_contexts(runtime: &JSRuntime, counter: usize) -> JSResult<()> {
    let script = format!(
        r#"(() => {{
            const node = {{
                probe: new Probe({counter}),
                ballast: Array.from({{ length: 2000 }}, (_, i) => ({{ i }})),
            }};
            node.self = node;
            node.next = () => node;
            globalThis.kept = node;
            new Probe({counter}).hold({{ payload: new Array(256).fill(0) }});
        }})();"#
    );
    for _ in 0..CONTEXTS {
        let ctx = runtime.context();
        ctx.register_class::<Probe>()?;
        ctx.eval::<()>(Source::from_bytes(script.as_str()))?;
        drop(ctx);
    }
    Ok(())
}

/// Poll the drop count until `EXPECTED` probes are gone, failing at
/// `DEADLINE`. `collect` runs a collection on the worker, where needed.
fn wait_for_probes(counter: usize, mut collect: impl FnMut()) {
    let start = Instant::now();
    loop {
        let dropped = DROPS[counter].load(Ordering::SeqCst);
        if dropped >= EXPECTED {
            return;
        }
        assert!(
            start.elapsed() < DEADLINE,
            "{dropped} of {PROBES} probes dropped after {DEADLINE:?}, expected {EXPECTED}"
        );
        if COLLECT_EXPLICITLY {
            collect();
        }
        std::thread::sleep(POLL);
    }
}

#[test]
fn a_shared_worker_reclaims_dropped_contexts() {
    let rong = Rong::<RongJS>::builder()
        .shared()
        .workers(1)
        .build()
        .unwrap();

    rong.call_blocking_with_timeout(TASK_TIMEOUT, |runtime: JSRuntime, _receiver| async move {
        retire_contexts(&runtime, SHARED)
    })
    .unwrap();

    wait_for_probes(SHARED, || {
        rong.call_blocking_with_timeout(TASK_TIMEOUT, |runtime: JSRuntime, _receiver| async move {
            runtime.run_gc();
            Ok(())
        })
        .unwrap();
    });
}

#[test]
fn a_pinned_worker_reclaims_dropped_contexts() {
    let rong = Rong::<RongJS>::builder()
        .pinned::<u8, ()>()
        .workers(1)
        .build()
        .unwrap();

    rong.call_blocking_with_timeout(
        TASK_TIMEOUT,
        0,
        |runtime: JSRuntime, _key, state, _receiver| async move {
            (retire_contexts(&runtime, PINNED), state)
        },
    )
    .unwrap();

    wait_for_probes(PINNED, || {
        rong.call_blocking_with_timeout(
            TASK_TIMEOUT,
            0,
            |runtime: JSRuntime, _key, state, _receiver| async move {
                runtime.run_gc();
                (Ok(()), state)
            },
        )
        .unwrap();
    });
}

const GC_JS: &str = include_str!("unit/gc.js");

#[test]
fn weak_ref_targets_are_collected_on_a_worker() {
    async_run!(|ctx: JSContext| async move {
        ctx.global().set(
            "sleep",
            JSFunc::new(&ctx, |ms: u32| async move {
                tokio::time::sleep(Duration::from_millis(ms.into())).await;
            })?,
        )?;
        let runner = UnitJSRunner::load_source(&ctx, GC_JS).await?;
        assert!(runner.run().await?);
        Ok(())
    });
}
