//! A host function is shared: script may call it again while a call of it is
//! still running, from the function itself or from a callback it invoked.
//!
//! The functions take the context as a parameter instead of capturing it, so
//! no test leaves a function that keeps its own context alive.

use rong_macro::{js_class, js_method};
use rong_test::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use tokio::time::Duration;

fn failure(message: impl Into<String>) -> RongJSError {
    HostError::new(rong::error::E_INTERNAL, message.into()).into()
}

/// No script frame in between: the function looks itself up and calls it.
#[test]
fn a_host_function_calls_itself() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    ctx.global().set(
        "recurse",
        JSFunc::new(&ctx, |ctx: JSContext, n: i32| -> JSResult<i32> {
            if n == 0 {
                return Ok(0);
            }
            let again: JSFunc = ctx.global().get("recurse")?;
            Ok(1 + again.call::<_, i32>(None, (n - 1,))?)
        })?,
    )?;
    assert_eq!(ctx.eval::<i32>(Source::from_bytes("recurse(3)"))?, 3);
    assert_eq!(ctx.eval::<i32>(Source::from_bytes("recurse(64)"))?, 64);
    Ok(())
}

/// A host API that takes a callback, and the callback uses the same API.
#[test]
fn a_host_function_is_called_again_by_its_callback() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    ctx.global().set(
        "visit",
        JSFunc::new(&ctx, |n: i32, callback: JSFunc| -> JSResult<String> {
            let inner: String = callback.call(None, (n,))?;
            Ok(format!("v{n}({inner})"))
        })?,
    )?;
    let got: String = ctx.eval(Source::from_bytes(
        "const step = (n) => n === 0 ? 'bottom' : visit(n - 1, step); visit(4, step)",
    ))?;
    assert_eq!(got, "v4(v3(v2(v1(v0(bottom)))))");
    Ok(())
}

/// The nested call arrives through `Function.prototype.call` on a bound copy.
#[test]
fn a_host_function_is_reentered_through_call_and_bind() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    ctx.global().set(
        "visit",
        JSFunc::new(&ctx, |n: i32, callback: JSFunc| -> JSResult<i32> {
            callback.call(None, (n,))
        })?,
    )?;
    let got: i32 = ctx.eval(Source::from_bytes(
        r#"
        const bound = visit.bind(null);
        const step = (n) => n === 0 ? 0 : 1 + bound.call(undefined, n - 1, step);
        visit(5, step)
        "#,
    ))?;
    assert_eq!(got, 5);
    Ok(())
}

/// A callback throws at the bottom of a re-entrant chain: the error reaches
/// the outermost caller, every level has returned, and the function still
/// works.
#[test]
fn a_callback_throws_inside_a_reentrant_call() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let running = Rc::new(Cell::new(0u32));
    let calls = Rc::new(Cell::new(0u32));
    let (level, counted) = (running.clone(), calls.clone());
    ctx.global().set(
        "visit",
        JSFunc::new(&ctx, move |n: i32, callback: JSFunc| -> JSResult<i32> {
            counted.set(counted.get() + 1);
            level.set(level.get() + 1);
            let result = callback.call(None, (n,));
            level.set(level.get() - 1);
            result
        })?,
    )?;
    let got: String = ctx.eval(Source::from_bytes(
        r#"
        const step = (n) => {
            if (n === 0) throw new RangeError('bottom');
            return visit(n - 1, step);
        };
        let first;
        try { visit(3, step); first = 'no throw'; } catch (e) { first = String(e); }
        first + ' / ' + visit(2, (n) => n * 10)
        "#,
    ))?;
    assert_eq!(got, "RangeError: bottom / 20");
    assert_eq!(calls.get(), 5);
    assert_eq!(running.get(), 0);
    Ok(())
}

/// What a host function changes lives in `Cell` and `RefCell` captures, and
/// stays consistent when calls of it nest.
#[test]
fn captured_state_stays_consistent_across_nested_calls() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let deepest = Rc::new(Cell::new(0u32));
    let (entries, reached) = (log.clone(), deepest.clone());
    let level = Cell::new(0u32);
    ctx.global().set(
        "walk",
        JSFunc::new(&ctx, move |n: i32, callback: JSFunc| -> JSResult<i32> {
            level.set(level.get() + 1);
            reached.set(reached.get().max(level.get()));
            // Each borrow ends at its statement, before script runs.
            entries.borrow_mut().push(format!("in {n}"));
            let result = callback.call::<_, i32>(None, (n,));
            entries.borrow_mut().push(format!("out {n}"));
            level.set(level.get() - 1);
            result
        })?,
    )?;
    let got: i32 = ctx.eval(Source::from_bytes(
        "const step = (n) => n === 0 ? 0 : walk(n - 1, step) + 1; walk(3, step)",
    ))?;
    assert_eq!(got, 3);
    assert_eq!(deepest.get(), 4);
    assert_eq!(
        log.borrow().join(", "),
        "in 3, in 2, in 1, in 0, out 0, out 1, out 2, out 3"
    );
    Ok(())
}

struct Alive(Rc<Cell<bool>>);

impl Drop for Alive {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// A function that removes the only script reference to itself and forces a
/// collection keeps what it captured until its call returns.
#[test]
fn a_host_function_outlives_its_own_removal() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let alive = Rc::new(Cell::new(true));
    let marker = Alive(alive.clone());
    ctx.global().set(
        "vanish",
        JSFunc::new(&ctx, move |ctx: JSContext, n: i32| -> JSResult<i32> {
            ctx.eval::<()>(Source::from_bytes("globalThis.vanish = undefined;"))?;
            ctx.runtime().run_gc();
            if !marker.0.get() {
                return Err(failure("the closure was dropped during its own call"));
            }
            Ok(n * 2)
        })?,
    )?;
    assert!(alive.get());
    assert_eq!(ctx.eval::<i32>(Source::from_bytes("vanish(21)"))?, 42);
    assert!(ctx.eval::<bool>(Source::from_bytes("globalThis.vanish === undefined"))?);
    Ok(())
}

/// Two host functions calling each other in turn.
#[test]
fn two_host_functions_call_each_other() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    for (name, other) in [("ping", "pong"), ("pong", "ping")] {
        ctx.global().set(
            name,
            JSFunc::new(&ctx, move |ctx: JSContext, n: i32| -> JSResult<i32> {
                if n == 0 {
                    return Ok(0);
                }
                let next: JSFunc = ctx.global().get(other)?;
                Ok(1 + next.call::<_, i32>(None, (n - 1,))?)
            })?,
        )?;
    }
    assert_eq!(ctx.eval::<i32>(Source::from_bytes("ping(9)"))?, 9);
    Ok(())
}

/// The synchronous part of an async host function calls the function again
/// before it returns its own promise.
#[test]
fn an_async_host_function_calls_itself_before_returning() {
    async_run!(|ctx: JSContext| async move {
        ctx.global().set(
            "countdown",
            JSFunc::new(&ctx, |ctx: JSContext, n: i32| {
                let below = if n > 0 {
                    ctx.global()
                        .get::<_, JSFunc>("countdown")
                        .and_then(|again| again.call::<_, Promise>(None, (n - 1,)))
                        .map(Some)
                } else {
                    Ok(None)
                };
                async move {
                    match below? {
                        Some(promise) => Ok(promise.into_future::<i32>().await? + 1),
                        None => Ok::<i32, RongJSError>(0),
                    }
                }
            })?,
        )?;
        let depth: i32 = ctx.eval_async(Source::from_bytes("countdown(5)")).await?;
        assert_eq!(depth, 5);
        Ok(())
    });
}

/// The future of an async host function, when polled, calls a script
/// callback that calls the function again.
#[test]
fn an_async_host_function_is_called_again_from_its_future() {
    async_run!(|ctx: JSContext| async move {
        ctx.global().set(
            "later",
            JSFunc::new(&ctx, |n: i32, callback: JSFunc| async move {
                tokio::time::sleep(Duration::from_millis(1)).await;
                if n == 0 {
                    return Ok::<i32, RongJSError>(0);
                }
                let inner: Promise = callback.call(None, (n - 1,))?;
                Ok(inner.into_future::<i32>().await? + 1)
            })?,
        )?;
        let depth: i32 = ctx
            .eval_async(Source::from_bytes(
                "const step = (n) => later(n, step); later(4, step)",
            ))
            .await?;
        assert_eq!(depth, 4);
        Ok(())
    });
}

/// A once-function called from inside itself is refused as a once-function
/// that already ran, not as something that is not a function.
#[test]
fn a_once_function_refuses_a_nested_call() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    ctx.global().set(
        "once",
        JSFunc::new_once(&ctx, |ctx: JSContext| -> JSResult<String> {
            let again: JSFunc = ctx.global().get("once")?;
            Ok(match again.call::<_, ()>(None, ()) {
                Ok(()) => "ran twice".to_string(),
                Err(e) => thrown_error_message(&ctx, &e).unwrap_or_else(|_| e.to_string()),
            })
        })?,
    )?;
    let got: String = ctx.eval(Source::from_bytes("once()"))?;
    assert!(got.contains("OnceFn had been called"), "{got}");
    Ok(())
}

#[js_class]
struct Tree {
    depth: i32,
}

#[js_class]
impl Tree {
    #[js_method(constructor)]
    fn new(depth: i32) -> Self {
        Self { depth }
    }

    #[js_method(getter)]
    fn depth(&self) -> i32 {
        self.depth
    }

    /// Calls `callback` with the next level until the tree's depth is reached.
    #[js_method]
    fn walk(&self, this: This<JSObject>, n: i32, callback: JSFunc) -> JSResult<i32> {
        if n >= self.depth {
            return Ok(n);
        }
        callback.call(Some(this.0), (n + 1,))
    }
}

/// A `&self` method is one function shared by every instance: a callback
/// re-enters it, and a getter, on the instance whose call is still running.
#[test]
fn a_method_is_reentered_on_its_own_instance() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    ctx.register_class::<Tree>()?;
    let got: i32 = ctx.eval(Source::from_bytes(
        r#"
        const tree = new Tree(6);
        const step = function (n) { return this.walk(n, step) + (this.depth - 6); };
        tree.walk(0, step)
        "#,
    ))?;
    assert_eq!(got, 6);
    Ok(())
}
