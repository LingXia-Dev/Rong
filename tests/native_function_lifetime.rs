//! A native function Rust creates and drops is freed with its closure,
//! whether it was created inside a native call or by Rust at top level.
//!
//! JavaScriptCore collects from run-loop timers the test harness does not
//! service (see `tests/gc.rs`), so it does not run this test. `target_env =
//! "ohos"` is the device build, where the engine is ArkJS.
#![cfg(any(
    target_env = "ohos",
    feature = "primary-quickjs",
    all(feature = "quickjs", not(feature = "jscore"), not(feature = "arkjs"))
))]

use rong_test::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const FUNCTIONS: usize = 10_000;
static DROPPED_IN_CALL: AtomicUsize = AtomicUsize::new(0);
static DROPPED_AT_TOP: AtomicUsize = AtomicUsize::new(0);

struct DropMarker(&'static AtomicUsize);

impl Drop for DropMarker {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn log_line(line: &str) {
    #[cfg(target_env = "ohos")]
    log::info!("[native_function_lifetime] {line}");
    #[cfg(not(target_env = "ohos"))]
    println!("[native_function_lifetime] {line}");
}

#[cfg(target_env = "ohos")]
fn global_handles(ctx: &JSContext) -> usize {
    use rong_arkjs_sys as sys;
    use rong_core::JSContextImpl;
    unsafe {
        let env: sys::JSVM_Env = *ctx.as_ref().as_raw();
        let mut vm: sys::JSVM_VM = std::ptr::null_mut();
        let mut stats: sys::JSVM_HeapStatistics = std::mem::zeroed();
        sys::OH_JSVM_GetVM(env, &mut vm);
        sys::OH_JSVM_GetHeapStatistics(vm, &mut stats);
        stats.totalGlobalHandlesSize
    }
}

#[cfg(not(target_env = "ohos"))]
fn global_handles(_ctx: &JSContext) -> usize {
    0
}

/// Created inside a native call, as an app registers a handler: the call's
/// handle scope closes when it returns. Half the functions never reach JS;
/// the other half are handed to JS and released there, the way a replaced
/// handler is.
#[test]
fn dropped_native_functions_are_freed() -> Result<(), String> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let err = |e: RongJSError| e.to_string();
    let make_ctx = ctx.clone();
    ctx.global()
        .set(
            "makeHandlers",
            JSFunc::new(&ctx, move |count: u32| -> JSResult<()> {
                for i in 0..count {
                    let marker = DropMarker(&DROPPED_IN_CALL);
                    let func = JSFunc::new(&make_ctx, move || {
                        let _ = &marker;
                    })?;
                    if i % 2 == 1 {
                        make_ctx.global().set("handler", func)?;
                    }
                }
                Ok(())
            })
            .map_err(err)?,
        )
        .map_err(err)?;

    let create = || {
        ctx.eval::<()>(Source::from_bytes(format!(
            "for (let i = 0; i < {}; i++) makeHandlers(100); delete globalThis.handler;",
            FUNCTIONS / 100
        )))
        .map_err(err)
    };
    two_rounds(&rt, &ctx, "in a native call", &DROPPED_IN_CALL, create)
}

/// Created by Rust outside any native call, as a host sets up handlers at
/// startup. On engines with scoped handles nothing but the backend's own
/// scope frees the handles these calls make.
#[test]
fn native_functions_created_at_top_level_are_freed() -> Result<(), String> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let err = |e: RongJSError| e.to_string();
    let create = || {
        for i in 0..FUNCTIONS {
            let marker = DropMarker(&DROPPED_AT_TOP);
            let func = JSFunc::new(&ctx, move || {
                let _ = &marker;
            })
            .map_err(err)?;
            if i % 2 == 1 {
                ctx.global().set("handler", func).map_err(err)?;
            }
        }
        ctx.eval::<()>(Source::from_bytes("delete globalThis.handler;"))
            .map_err(err)
    };
    two_rounds(&rt, &ctx, "at top level", &DROPPED_AT_TOP, create)
}

fn two_rounds(
    rt: &JSRuntime,
    ctx: &JSContext,
    label: &str,
    dropped: &AtomicUsize,
    create: impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    let handles_before = global_handles(ctx);
    let first = create_and_collect(rt, ctx, dropped, &create)?;
    let second = create_and_collect(rt, ctx, dropped, &create)?;
    let report = format!(
        "{label}: global handles {handles_before} bytes before; round 1: {first}; round 2: {second}"
    );
    log_line(&report);

    if first.freed < FUNCTIONS / 2 || second.freed < FUNCTIONS / 2 {
        return Err(format!("dropped native functions were not freed: {report}"));
    }
    // Freed handles are reused: a second round of the same size must not need
    // as many new ones again. Blocks V8 keeps allocated are a high-water mark.
    let grown_first = first.after_collecting.saturating_sub(handles_before);
    let grown_second = second
        .after_collecting
        .saturating_sub(first.after_collecting);
    if grown_second > grown_first / 4 {
        return Err(format!(
            "persistent references outlive their functions: {report}"
        ));
    }
    Ok(())
}

struct Round {
    freed: usize,
    after_creating: usize,
    after_collecting: usize,
}

impl std::fmt::Display for Round {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "freed {} of {FUNCTIONS} closures, global handles {} after creating, {} after collecting",
            self.freed, self.after_creating, self.after_collecting
        )
    }
}

fn create_and_collect(
    rt: &JSRuntime,
    ctx: &JSContext,
    dropped: &AtomicUsize,
    create: &impl Fn() -> Result<(), String>,
) -> Result<Round, String> {
    let before = dropped.load(Ordering::SeqCst);
    create()?;
    let after_creating = global_handles(ctx);

    let deadline = Instant::now() + Duration::from_secs(10);
    let freed = loop {
        rt.run_gc();
        rt.run_pending_jobs();
        let freed = dropped.load(Ordering::SeqCst) - before;
        if freed >= FUNCTIONS || Instant::now() >= deadline {
            break freed;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    Ok(Round {
        freed,
        after_creating,
        after_collecting: global_handles(ctx),
    })
}
