//! A native function can be called again while a call of it is still running:
//! by itself, or by script it calls.

use rong_test::*;
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn a_native_function_can_call_itself() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let own = ctx.clone();
    ctx.global().set(
        "recurse",
        JSFunc::new(&ctx, move |n: i32| -> JSResult<i32> {
            if n == 0 {
                return Ok(0);
            }
            let again: JSFunc = own.global().get("recurse")?;
            Ok(1 + again.call::<_, i32>(None, (n - 1,))?)
        })?,
    )?;
    let depth: i32 = ctx.eval(Source::from_bytes("recurse(3)"))?;
    assert_eq!(depth, 3);
    let depth: i32 = ctx.eval(Source::from_bytes("recurse(40)"))?;
    assert_eq!(depth, 40);
    ctx.global().set("recurse", JSValue::undefined(&ctx))?;
    Ok(())
}

/// The native function calls a script callback, which calls the native
/// function again: a host API taking a callback that uses the same API.
#[test]
fn a_native_function_can_be_called_again_by_its_callback() -> JSResult<()> {
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

/// A callback throws at the bottom of a re-entrant chain: the error reaches
/// the outermost caller unchanged, and the function works afterwards.
#[test]
fn a_callback_that_throws_inside_a_reentrant_call() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    ctx.global().set(
        "visit",
        JSFunc::new(&ctx, move |n: i32, callback: JSFunc| -> JSResult<i32> {
            counted.set(counted.get() + 1);
            callback.call(None, (n,))
        })?,
    )?;
    let got: String = ctx.eval(Source::from_bytes(
        r#"
        const failure = new RangeError('bottom');
        const step = (n) => { if (n === 0) throw failure; return visit(n - 1, step); };
        let first;
        try { visit(3, step); first = 'no throw'; }
        catch (e) { first = e === failure ? String(e) : 'other: ' + String(e); }
        first + ' / ' + visit(2, (n) => n * 10)
        "#,
    ))?;
    assert_eq!(got, "RangeError: bottom / 20");
    assert_eq!(calls.get(), 5);
    Ok(())
}

/// The synchronous part of an async native function calls the function
/// again before it returns its own promise.
#[test]
fn an_async_native_function_can_call_itself() {
    async_run!(|ctx: JSContext| async move {
        let own = ctx.clone();
        ctx.global().set(
            "countdown",
            JSFunc::new(&ctx, move |n: i32| {
                let below = if n > 0 {
                    own.global()
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
        ctx.global().set("countdown", JSValue::undefined(&ctx))?;
        Ok(())
    });
}

/// A function made to be called once refuses a nested call clearly instead of
/// failing as if it were not a function.
#[test]
fn a_once_function_called_again_while_running_refuses() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let own = ctx.clone();
    ctx.global().set(
        "once",
        JSFunc::new_once(&ctx, move || -> JSResult<String> {
            let again: JSFunc = own.global().get("once")?;
            Ok(match again.call::<_, ()>(None, ()) {
                Ok(()) => "called twice".to_string(),
                Err(e) => e.to_string(),
            })
        })?,
    )?;
    let got: String = ctx.eval(Source::from_bytes("once()"))?;
    assert!(!got.contains("Not JS Function"), "{got}");
    assert!(got.contains("called"), "{got}");
    ctx.global().set("once", JSValue::undefined(&ctx))?;
    Ok(())
}
