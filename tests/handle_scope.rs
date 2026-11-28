//! Rust-driven work outside any native call must not pile up engine handles,
//! and values it returns must outlive the operation that made them: across a
//! thrown exception, and across native calls that drive more calls of their
//! own, in the same context or another one.

use rong_test::*;
use std::cell::RefCell;
use std::rc::Rc;

const SOAK_CALLS: usize = 100_000;
const SOAK_CHECK_EVERY: usize = 10_000;
const PAYLOAD_BYTES: usize = 4 * 1024;
const GROWTH_LIMIT: usize = 64 * 1024 * 1024;
const RETAINED_LIMIT: usize = 4 * 1024 * 1024;

fn log_line(line: &str) {
    #[cfg(target_env = "ohos")]
    log::info!("[handle_scope] {line}");
    #[cfg(not(target_env = "ohos"))]
    println!("[handle_scope] {line}");
}

/// Used heap in bytes, where the engine reports it.
#[cfg(target_env = "ohos")]
fn used_heap(ctx: &JSContext) -> Option<usize> {
    use rong_core::JSContextImpl;
    use rong_jsvm_sys as sys;
    unsafe {
        let env: sys::JSVM_Env = *ctx.as_ref().as_raw();
        let mut vm: sys::JSVM_VM = std::ptr::null_mut();
        let mut stats: sys::JSVM_HeapStatistics = std::mem::zeroed();
        sys::OH_JSVM_GetVM(env, &mut vm);
        sys::OH_JSVM_GetHeapStatistics(vm, &mut stats);
        Some(stats.usedHeapSize)
    }
}

#[cfg(not(target_env = "ohos"))]
fn used_heap(_ctx: &JSContext) -> Option<usize> {
    None
}

fn failure(message: String) -> RongJSError {
    HostError::new(rong::error::E_INTERNAL, message).into()
}

/// The thrown value's text, or the error itself when nothing was thrown.
fn describe(ctx: &JSContext, err: &RongJSError) -> String {
    let Some(thrown) = err.thrown_value(ctx) else {
        return err.to_string();
    };
    ctx.global()
        .set("__scopeThrown", thrown)
        .and_then(|_| ctx.eval::<String>(Source::from_bytes("String(globalThis.__scopeThrown)")))
        .unwrap_or_else(|_| err.to_string())
}

/// Every call passes a fresh few-KB string and reads a fresh object back, so
/// handles kept past each call would hold megabytes of heap.
#[test]
fn rust_driven_calls_return_the_heap_to_baseline() -> Result<(), String> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let err = |e: RongJSError| describe(&ctx, &e);
    let func: JSFunc = ctx
        .eval(Source::from_bytes(
            "(text, i) => ({ length: text.length, i, tail: text.slice(-8) })",
        ))
        .map_err(err)?;

    rt.run_gc();
    let baseline = used_heap(&ctx);
    let filler = "x".repeat(PAYLOAD_BYTES - 16);
    let mut peak = baseline;
    for i in 0..SOAK_CALLS {
        let payload = format!("{filler}{i:016}");
        let item: JSObject = func.call(None, (payload, i as i32)).map_err(err)?;
        let length: u32 = item.get("length").map_err(err)?;
        if length as usize != PAYLOAD_BYTES {
            return Err(format!("call {i} saw a payload of {length} bytes"));
        }
        if (i + 1) % SOAK_CHECK_EVERY == 0
            && let (Some(base), Some(now)) = (baseline, used_heap(&ctx))
        {
            peak = peak.max(Some(now));
            if now.saturating_sub(base) > GROWTH_LIMIT {
                return Err(format!(
                    "used heap grew from {base} to {now} bytes after {} calls",
                    i + 1
                ));
            }
        }
    }
    rt.run_gc();
    let retained = used_heap(&ctx);
    log_line(&format!(
        "{SOAK_CALLS} calls with a {PAYLOAD_BYTES}-byte payload: used heap {baseline:?} before, \
         peak {peak:?} at checks, {retained:?} after collecting"
    ));
    match (baseline, retained) {
        (Some(base), Some(after)) if after.saturating_sub(base) > RETAINED_LIMIT => Err(format!(
            "used heap retained {} bytes over its {base}-byte baseline",
            after - base
        )),
        _ => Ok(()),
    }
}

/// A call that throws ends its operation with the exception pending; the
/// thrown value must still read back after later operations reuse the slots.
#[test]
fn a_thrown_value_outlives_the_call_that_threw_it() -> Result<(), String> {
    const THROWS: usize = 64;
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let thrower: JSFunc = ctx
        .eval(Source::from_bytes(
            "(i) => { throw new Error('thrown ' + i + ' ' + 'z'.repeat(64)); }",
        ))
        .map_err(|e| describe(&ctx, &e))?;
    let echo: JSFunc = ctx
        .eval(Source::from_bytes("(i) => ({ i, text: 'echo ' + i })"))
        .map_err(|e| describe(&ctx, &e))?;

    let mut errors = Vec::with_capacity(THROWS);
    for i in 0..THROWS {
        match thrower.call::<_, ()>(None, (i as i32,)) {
            Ok(()) => return Err(format!("call {i} returned instead of throwing")),
            Err(e) => errors.push(e),
        }
        // A later call must not see the earlier exception as its own.
        let item: JSObject = echo
            .call(None, (i as i32,))
            .map_err(|e| format!("call after throw {i} failed: {}", describe(&ctx, &e)))?;
        let back: i32 = item.get("i").map_err(|e| describe(&ctx, &e))?;
        if back != i as i32 {
            return Err(format!("call after throw {i} returned {back}"));
        }
    }
    for (i, e) in errors.iter().enumerate() {
        let text = describe(&ctx, e);
        if !text.starts_with(&format!("Error: thrown {i} ")) {
            return Err(format!("thrown value {i} reads back as {text:?}"));
        }
    }
    Ok(())
}

/// The next step of the chain and the context it belongs to.
type Next = Rc<RefCell<Option<(JSFunc, JSContext)>>>;

const DEPTH: i32 = 8;

/// The levels of the chain: native functions that, from inside their own
/// native call, drive a call of `next` and check a value made before. Each
/// level has its own function, since a native function cannot be re-entered
/// while it runs. A value thrown in another context cannot be rethrown here
/// (thrown values live in their context's table), so a failure below crosses
/// as text.
fn install_descend(ctx: &JSContext, next: Next) -> JSResult<()> {
    for level in 0..=DEPTH {
        install_level(ctx, level, next.clone())?;
    }
    Ok(())
}

fn install_level(ctx: &JSContext, level: i32, next: Next) -> JSResult<()> {
    let own = ctx.clone();
    let name = format!("descend{level}");
    ctx.global().set(
        name.as_str(),
        JSFunc::new(ctx, move |n: i32| -> JSResult<String> {
            let before = JSObject::new(&own);
            before.set("n", n)?;
            before.set("label", format!("level {n}"))?;
            let inner = if n > 0 {
                let (next, next_ctx) = next
                    .borrow()
                    .clone()
                    .ok_or_else(|| failure("chain not set up".into()))?;
                next.call::<_, String>(None, (n - 1,))
                    .map_err(|e| failure(format!("below level {n}: {}", describe(&next_ctx, &e))))?
            } else {
                String::from("bottom")
            };
            let label: String = before.get("label")?;
            if label != format!("level {n}") || before.get::<_, i32>("n")? != n {
                return Err(failure(format!("level {n} lost its value: {label:?}")));
            }
            Ok(format!("d{n}({inner})"))
        })?,
    )?;
    Ok(())
}

fn install_step(ctx: &JSContext, next: Next) -> JSResult<JSFunc> {
    install_descend(ctx, next)?;
    ctx.eval(Source::from_bytes(
        "(n) => { const pad = 'p'.repeat(32); return 's' + n + ':' + globalThis['descend' + n](n) + pad.slice(32); }",
    ))
}

/// Drives `step(DEPTH)` 100 times and checks the whole chain's result.
fn drive(ctx: &JSContext, step: &JSFunc) -> Result<(), String> {
    let mut expected = String::from("bottom");
    for n in 0..=DEPTH {
        expected = format!("s{n}:d{n}({expected})");
    }
    for round in 0..100 {
        let got: String = step
            .call(None, (DEPTH,))
            .map_err(|e| format!("round {round}: {}", describe(ctx, &e)))?;
        if got != expected {
            return Err(format!(
                "round {round} returned {got:?}, expected {expected:?}"
            ));
        }
    }
    Ok(())
}

/// Rust drives `step`; its JS calls the native `descend`, which drives `step`
/// again from inside its native call, eight levels deep.
#[test]
fn reentrant_rust_driven_calls() -> Result<(), String> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let next: Next = Rc::default();
    let step = install_step(&ctx, next.clone()).map_err(|e| describe(&ctx, &e))?;
    *next.borrow_mut() = Some((step.clone(), ctx.clone()));
    let result = drive(&ctx, &step);
    next.borrow_mut().take();
    result
}

/// The same chain alternating between two contexts: each level's call goes to
/// the other context, so each keeps its own operation depth while the other
/// is mid-operation.
#[test]
fn reentrant_rust_driven_calls_across_contexts() -> Result<(), String> {
    let rt = RongJS::runtime();
    alternate(&rt.context(), &rt.context())
}

/// As above with each context in its own runtime, so on engines with one VM
/// per runtime each level's call crosses into the other VM.
#[test]
fn reentrant_rust_driven_calls_across_runtimes() -> Result<(), String> {
    let rt_a = RongJS::runtime();
    let rt_b = RongJS::runtime();
    alternate(&rt_a.context(), &rt_b.context())
}

fn alternate(a: &JSContext, b: &JSContext) -> Result<(), String> {
    let next_from_a: Next = Rc::default();
    let next_from_b: Next = Rc::default();
    let step_a = install_step(a, next_from_a.clone()).map_err(|e| describe(a, &e))?;
    let step_b = install_step(b, next_from_b.clone()).map_err(|e| describe(b, &e))?;
    *next_from_a.borrow_mut() = Some((step_b, b.clone()));
    *next_from_b.borrow_mut() = Some((step_a.clone(), a.clone()));
    let result = drive(a, &step_a);
    // Break the cycle between the two contexts' functions.
    next_from_a.borrow_mut().take();
    next_from_b.borrow_mut().take();
    result
}

const BATCH: usize = 20_000;
const BATCH_PAYLOAD: usize = 1024;

/// Used heap after collecting, from inside a native call.
fn collected_heap(ctx: &JSContext) -> Option<usize> {
    ctx.runtime().run_gc();
    used_heap(ctx)
}

/// One native call, driven from Rust, makes `BATCH` objects of about 1 KB in a
/// Rust loop, keeping none. Every operation in it is nested in the call's
/// operation, so without a scope of its own each object's handle stays until
/// the call returns; `JSContext::handle_scope` around each step frees it.
#[test]
fn handles_of_a_batch_in_one_native_call_stay_unless_scoped() -> Result<(), String> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let own = ctx.clone();
    ctx.global()
        .set(
            "batch",
            JSFunc::new(&ctx, move |scoped: bool| -> JSResult<String> {
                let filler = "b".repeat(BATCH_PAYLOAD - 16);
                let before = collected_heap(&own);
                for i in 0..BATCH {
                    let step = || -> JSResult<()> {
                        let item = JSObject::new(&own);
                        item.set("text", format!("{filler}{i:016}"))?;
                        Ok(())
                    };
                    if scoped {
                        own.handle_scope(step)?
                    } else {
                        step()?
                    }
                }
                let after = collected_heap(&own);
                Ok(match (before, after) {
                    (Some(b), Some(a)) => a.saturating_sub(b).to_string(),
                    _ => String::from("unknown"),
                })
            })
            .map_err(|e| describe(&ctx, &e))?,
        )
        .map_err(|e| describe(&ctx, &e))?;
    let growth = |scoped: bool| -> Result<Option<usize>, String> {
        let text: String = ctx
            .eval(Source::from_bytes(format!("batch({scoped})")))
            .map_err(|e| describe(&ctx, &e))?;
        Ok(text.parse().ok())
    };
    let unscoped = growth(false)?;
    let scoped = growth(true)?;
    log_line(&format!(
        "{BATCH} objects of {BATCH_PAYLOAD} bytes in one native call: heap kept {unscoped:?} \
         bytes unscoped, {scoped:?} with a scope per step"
    ));
    // Engines with handle scopes report heap sizes; the others answer None.
    let (Some(unscoped), Some(scoped)) = (unscoped, scoped) else {
        return Ok(());
    };
    if unscoped < BATCH * BATCH_PAYLOAD / 2 {
        return Err(format!(
            "unscoped batch kept only {unscoped} bytes: handles no longer stay until the \
             native call returns, so this test and the docs saying they do need updating"
        ));
    }
    if scoped > RETAINED_LIMIT {
        return Err(format!("scoped batch kept {scoped} bytes"));
    }
    Ok(())
}
