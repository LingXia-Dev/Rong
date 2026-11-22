//! A JS value that Rust keeps past the native call that created it must keep
//! naming the same value, however much the engine allocates in between.
//!
//! Each test returns `Err` with what ran or what was thrown, so a broken
//! engine reports a failure rather than a panic.

use rong::JSEngineValue;
use rong_macro::js_class;
use rong_test::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Held = Rc<RefCell<Vec<JSFunc>>>;

const DECOYS: usize = 16;
const CHURN_ROUNDS: usize = 64;

fn failure(message: String) -> RongJSError {
    HostError::new(rong::error::E_INTERNAL, message).into()
}

/// The thrown value's text, or the error itself when nothing was thrown.
fn describe(ctx: &JSContext, err: &RongJSError) -> String {
    let Some(thrown) = err.thrown_value(ctx) else {
        return err.to_string();
    };
    let text = ctx
        .global()
        .set("__heldThrown", thrown)
        .and_then(|_| ctx.eval::<String>(Source::from_bytes("String(globalThis.__heldThrown)")));
    match text {
        Ok(text) => format!("threw {text}"),
        Err(_) => err.to_string(),
    }
}

fn in_context(body: impl FnOnce(&JSContext) -> JSResult<()>) -> Result<(), String> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    body(&ctx).map_err(|err| describe(&ctx, &err))
}

/// Allocates JS functions and Rust closures inside native calls and drops them
/// before returning, so released handle slots and references get reused.
fn install_churn(ctx: &JSContext) -> JSResult<()> {
    let churn_ctx = ctx.clone();
    ctx.global().set(
        "churn",
        JSFunc::new(ctx, move |make_decoy: JSFunc| -> JSResult<()> {
            let mut list = Vec::new();
            let mut map = HashMap::new();
            for i in 0..DECOYS {
                let js_decoy: JSFunc = make_decoy.call(None, ())?;
                let held = js_decoy.clone();
                let native_decoy = JSFunc::new(&churn_ctx, move || {
                    let _ = held.call::<_, ()>(None, ());
                })?;
                list.push(js_decoy);
                map.insert(i, native_decoy);
            }
            drop(list);
            drop(map);
            Ok(())
        })?,
    )?;
    ctx.eval::<()>(Source::from_bytes("globalThis.log = [];"))?;
    churn(ctx)
}

fn churn(ctx: &JSContext) -> JSResult<()> {
    ctx.eval::<()>(Source::from_bytes(&format!(
        "for (let i = 0; i < {CHURN_ROUNDS}; i++) churn(() => () => log.push('B'));"
    )))
}

/// Calls every held function, then checks that exactly `expected` calls ran
/// the original function `A` and nothing else ran or threw.
fn call_all_expecting_a(ctx: &JSContext, held: &Held, expected: usize) -> JSResult<()> {
    let mut thrown = Vec::new();
    let funcs: Vec<JSFunc> = held.borrow().clone();
    for func in funcs {
        if let Err(err) = func.call::<_, ()>(None, ()) {
            thrown.push(describe(ctx, &err));
        }
    }
    expect_a(ctx, expected, thrown)
}

fn expect_a(ctx: &JSContext, expected: usize, thrown: Vec<String>) -> JSResult<()> {
    let log: Vec<String> = ctx.eval(Source::from_bytes("globalThis.log.slice()"))?;
    if log == vec!["A".to_string(); expected] && thrown.is_empty() {
        Ok(())
    } else {
        Err(failure(format!(
            "expected A to run {expected} time(s); ran {log:?}, calls threw {thrown:?}"
        )))
    }
}

/// The LingXia watcher shape: the native `watch(cb)` wraps a clone of `cb` in
/// a `JSFunc::new` closure, keeps that closure, and drops other functions
/// before it returns. The kept closure is called later from Rust.
#[test]
fn a_native_closure_created_in_a_callback_calls_its_own_function() -> Result<(), String> {
    in_context(|ctx| {
        let held: Held = Rc::default();
        let watch_held = held.clone();
        let watch_ctx = ctx.clone();
        ctx.global().set(
            "watch",
            JSFunc::new(ctx, move |cb: JSFunc, make_decoy: JSFunc| -> JSResult<()> {
                let callback = cb.clone();
                let handler = JSFunc::new(&watch_ctx, move || {
                    let _ = callback.call::<_, ()>(None, ());
                })?;
                watch_held.borrow_mut().push(handler);
                let decoys: Vec<JSFunc> = (0..DECOYS)
                    .map(|_| make_decoy.call(None, ()))
                    .collect::<JSResult<_>>()?;
                drop(decoys);
                Ok(())
            })?,
        )?;
        install_churn(ctx)?;
        ctx.eval::<()>(Source::from_bytes(
            "watch(() => log.push('A'), () => () => log.push('B'));
             watch(() => log.push('A'), () => () => log.push('B'));",
        ))?;
        churn(ctx)?;
        call_all_expecting_a(ctx, &held, 2)
    })
}

/// A clone of a clone of a callback argument, kept past the callback.
#[test]
fn a_clone_of_a_clone_of_an_argument_calls_its_own_function() -> Result<(), String> {
    in_context(|ctx| {
        let held: Held = Rc::default();
        let keep = held.clone();
        ctx.global().set(
            "keep",
            JSFunc::new(ctx, move |cb: JSFunc| {
                let first = cb.clone();
                keep.borrow_mut().push(first.clone());
            })?,
        )?;
        install_churn(ctx)?;
        ctx.eval::<()>(Source::from_bytes("keep(() => log.push('A'));"))?;
        churn(ctx)?;
        call_all_expecting_a(ctx, &held, 1)
    })
}

/// A function the native call creates itself (not one it was given), kept as
/// is and as a clone made after the call returned.
#[test]
fn a_function_created_in_a_callback_calls_itself_later() -> Result<(), String> {
    in_context(|ctx| {
        let held: Held = Rc::default();
        let keep = held.clone();
        let keep_ctx = ctx.clone();
        ctx.global().set(
            "make",
            JSFunc::new(ctx, move || -> JSResult<()> {
                let made: JSFunc = keep_ctx.eval(Source::from_bytes("() => log.push('A')"))?;
                keep.borrow_mut().push(made);
                Ok(())
            })?,
        )?;
        install_churn(ctx)?;
        ctx.eval::<()>(Source::from_bytes("make();"))?;
        churn(ctx)?;
        let late_clone = held.borrow()[0].clone();
        held.borrow_mut().push(late_clone);
        churn(ctx)?;
        call_all_expecting_a(ctx, &held, 2)
    })
}

/// The same shape across an await, called with `call_async` as LingXia's
/// event dispatch does.
#[test]
fn a_native_closure_kept_across_an_await_calls_its_own_function() -> Result<(), String> {
    let outcome: Arc<Mutex<Option<String>>> = Arc::default();
    let report = outcome.clone();
    async_run!(|ctx: JSContext| async move {
        let result: JSResult<()> = async {
            let held: Held = Rc::default();
            let watch_held = held.clone();
            let watch_ctx = ctx.clone();
            ctx.global().set(
                "watch",
                JSFunc::new(&ctx, move |cb: JSFunc| -> JSResult<()> {
                    let callback = cb.clone();
                    let handler = JSFunc::new(&watch_ctx, move || {
                        let _ = callback.call::<_, ()>(None, ());
                    })?;
                    watch_held.borrow_mut().push(handler);
                    Ok(())
                })?,
            )?;
            install_churn(&ctx)?;
            ctx.eval::<()>(Source::from_bytes("watch(() => log.push('A'));"))?;
            tokio::time::sleep(Duration::from_millis(5)).await;
            churn(&ctx)?;
            tokio::time::sleep(Duration::from_millis(5)).await;
            let mut thrown = Vec::new();
            let handlers: Vec<JSFunc> = held.borrow().clone();
            for handler in handlers {
                if let Err(err) = handler.call_async::<_, ()>(None, ()).await {
                    thrown.push(describe(&ctx, &err));
                }
            }
            expect_a(&ctx, 1, thrown)
        }
        .await;
        if let Err(err) = result {
            *report.lock().unwrap() = Some(describe(&ctx, &err));
        }
        Ok(())
    });
    match outcome.lock().unwrap().take() {
        Some(failure) => Err(failure),
        None => Ok(()),
    }
}

#[js_class]
struct FirstHeld {}

impl JSClass<JSEngineValue> for FirstHeld {
    const NAME: &'static str = "FirstHeld";
    fn data_constructor() -> Constructor<JSEngineValue> {
        Constructor::new(|| FirstHeld {})
    }
    fn class_setup(_class: &ClassSetup<JSEngineValue>) -> JSResult<()> {
        Ok(())
    }
}

#[js_class]
struct SecondHeld {}

impl JSClass<JSEngineValue> for SecondHeld {
    const NAME: &'static str = "SecondHeld";
    fn data_constructor() -> Constructor<JSEngineValue> {
        Constructor::new(|| SecondHeld {})
    }
    fn class_setup(_class: &ClassSetup<JSEngineValue>) -> JSResult<()> {
        Ok(())
    }
}

/// Two classes registered from separate native calls stay separate classes.
#[test]
fn classes_registered_in_callbacks_stay_distinct() -> Result<(), String> {
    in_context(|ctx| {
        let first_ctx = ctx.clone();
        let second_ctx = ctx.clone();
        ctx.global().set(
            "registerFirst",
            JSFunc::new(ctx, move || first_ctx.register_class::<FirstHeld>())?,
        )?;
        ctx.global().set(
            "registerSecond",
            JSFunc::new(ctx, move || second_ctx.register_class::<SecondHeld>())?,
        )?;
        install_churn(ctx)?;
        ctx.eval::<()>(Source::from_bytes("registerFirst();"))?;
        churn(ctx)?;
        ctx.eval::<()>(Source::from_bytes("registerSecond();"))?;
        churn(ctx)?;

        let first = Class::lookup::<FirstHeld>(ctx)?.instance(FirstHeld {});
        let second = Class::lookup::<SecondHeld>(ctx)?.instance(SecondHeld {});
        let checks = [
            (
                "first is FirstHeld",
                Class::instance_of::<FirstHeld>(&first),
            ),
            (
                "first is not SecondHeld",
                !Class::instance_of::<SecondHeld>(&first),
            ),
            (
                "second is SecondHeld",
                Class::instance_of::<SecondHeld>(&second),
            ),
            (
                "second is not FirstHeld",
                !Class::instance_of::<FirstHeld>(&second),
            ),
        ];
        let failed: Vec<&str> = checks
            .iter()
            .filter(|(_, ok)| !ok)
            .map(|(n, _)| *n)
            .collect();
        if !failed.is_empty() {
            // ArkJS keeps each class's id on its constructor; other engines
            // report null. Type tags are not readable from JS.
            let ids: String = ctx.eval(Source::from_bytes(
                "JSON.stringify([FirstHeld, SecondHeld].map((c) => {
                     const d = Object.getOwnPropertyDescriptor(c, '__rong_class_id');
                     return d ? d.value : null;
                 }))",
            ))?;
            return Err(failure(format!(
                "class checks failed: {failed:?}; constructor ids [FirstHeld, SecondHeld] = {ids}"
            )));
        }
        let js_ok: bool = ctx.eval(Source::from_bytes(
            "class SubHeld extends FirstHeld {}
             const plain = new FirstHeld();
             plain instanceof FirstHeld
                 && !(plain instanceof SecondHeld)
                 && !(plain instanceof SubHeld)
                 && new SubHeld() instanceof FirstHeld",
        ))?;
        if !js_ok {
            return Err(failure("JS instanceof checks failed".into()));
        }
        Ok(())
    })
}
