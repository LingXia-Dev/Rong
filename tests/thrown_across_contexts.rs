//! A value thrown in one context reaches a caller in another: as itself when
//! both contexts share a runtime, and as an error carrying its text when they
//! do not.
//!
//! The context that calls into another is made last, so it closes first and
//! releases the other context's values while that context is still open.

use rong_test::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Makes `name` in `ctx` a native function that calls `target`, which may
/// belong to another context, and returns whatever it throws unchanged.
fn install_call(ctx: &JSContext, name: &str, target: JSFunc) -> JSResult<()> {
    ctx.global().set(
        name,
        JSFunc::new(ctx, move || -> JSResult<()> {
            target.call::<_, ()>(None, ())
        })?,
    )?;
    Ok(())
}

fn thrower(ctx: &JSContext, thrown: &str) -> JSResult<JSFunc> {
    ctx.eval(Source::from_bytes(format!("() => {{ throw {thrown}; }}")))
}

fn caught(ctx: &JSContext, call: &str, check: &str) -> JSResult<String> {
    ctx.eval(Source::from_bytes(format!(
        "try {{ {call}(); 'no throw' }} catch (e) {{ {check} }}"
    )))
}

#[test]
fn an_error_thrown_in_another_context_reaches_the_caller() -> JSResult<()> {
    let rt = RongJS::runtime();
    let b = rt.context();
    let a = rt.context();
    install_call(&a, "callB", thrower(&b, "new TypeError('from b')")?)?;
    assert_eq!(caught(&a, "callB", "String(e)")?, "TypeError: from b");
    Ok(())
}

#[test]
fn a_value_thrown_in_another_context_keeps_its_identity() -> JSResult<()> {
    let rt = RongJS::runtime();
    let b = rt.context();
    let a = rt.context();
    b.eval::<()>(Source::from_bytes(
        "globalThis.payload = { from: 'b' }; globalThis.failure = new RangeError('from b');",
    ))?;
    a.global()
        .set("payload", b.global().get::<_, JSObject>("payload")?)?;
    a.global()
        .set("failure", b.global().get::<_, JSObject>("failure")?)?;
    install_call(&a, "throwPayload", thrower(&b, "payload")?)?;
    install_call(&a, "throwFailure", thrower(&b, "failure")?)?;

    assert_eq!(
        caught(&a, "throwPayload", "e === payload ? 'same' : String(e)")?,
        "same"
    );
    assert_eq!(
        caught(&a, "throwFailure", "e === failure ? String(e) : 'other'")?,
        "RangeError: from b"
    );
    Ok(())
}

#[test]
fn a_non_error_thrown_in_another_context_reaches_the_caller() -> JSResult<()> {
    let rt = RongJS::runtime();
    let b = rt.context();
    let a = rt.context();
    let cases = [
        ("'boom'", "typeof e === 'string' && e === 'boom'"),
        ("undefined", "e === undefined"),
        ("null", "e === null"),
        ("42", "e === 42"),
        ("{ tag: 'plain' }", "e !== null && e.tag === 'plain'"),
    ];
    for (i, (thrown, check)) in cases.iter().enumerate() {
        let name = format!("callB{i}");
        install_call(&a, &name, thrower(&b, thrown)?)?;
        let got = caught(&a, &name, &format!("({check}) ? 'ok' : 'got ' + String(e)"))?;
        assert_eq!(got, "ok", "throw {thrown}");
    }
    Ok(())
}

/// C throws, a native function of B rethrows it into B's script, which does
/// not catch it, and a native function of A rethrows what B threw.
#[test]
fn a_value_thrown_three_contexts_deep_keeps_its_identity() -> JSResult<()> {
    let rt = RongJS::runtime();
    let c = rt.context();
    let b = rt.context();
    let a = rt.context();
    c.eval::<()>(Source::from_bytes(
        "globalThis.payload = new Error('from c');",
    ))?;
    a.global()
        .set("payload", c.global().get::<_, JSObject>("payload")?)?;

    install_call(&b, "callC", thrower(&c, "payload")?)?;
    let through_b: JSFunc = b.eval(Source::from_bytes("() => { callC(); }"))?;
    install_call(&a, "callB", through_b)?;

    assert_eq!(
        caught(
            &a,
            "callB",
            "e === payload ? String(e) : 'other: ' + String(e)"
        )?,
        "Error: from c"
    );
    Ok(())
}

#[test]
fn a_thrown_value_reads_back_through_any_context_of_its_runtime() -> JSResult<()> {
    let rt = RongJS::runtime();
    let b = rt.context();
    let a = rt.context();
    let err = thrower(&b, "new TypeError('from b')")?
        .call::<_, ()>(None, ())
        .unwrap_err();
    let thrown = err
        .thrown_value(&a)
        .expect("the value thrown in b reads back through a");
    assert_eq!(thrown.to_string_coerced()?, "TypeError: from b");
    Ok(())
}

/// The rejection of a native async function in A carries a value thrown in B.
#[test]
fn an_async_native_function_rejects_with_a_value_thrown_in_another_context() {
    async_run!(|b: JSContext| async move {
        let a = b.runtime().context();
        b.eval::<()>(Source::from_bytes(
            "globalThis.payload = new Error('from b');",
        ))?;
        a.global()
            .set("payload", b.global().get::<_, JSObject>("payload")?)?;
        let target = thrower(&b, "payload")?;
        a.global().set(
            "callBLater",
            JSFunc::new(&a, move || {
                let target = target.clone();
                async move {
                    tokio::task::yield_now().await;
                    target.call::<_, ()>(None, ())
                }
            })?,
        )?;
        let same: bool = a
            .eval_async(Source::from_bytes(
                "callBLater().then(() => false, (e) => e === payload)",
            ))
            .await?;
        assert!(same);
        Ok(())
    });
}

/// A runtime's values cannot enter another runtime: the caller gets an error
/// with the thrown value's text.
#[test]
fn a_value_thrown_in_another_runtime_arrives_as_its_text() -> JSResult<()> {
    let rt_b = RongJS::runtime();
    let b = rt_b.context();
    let rt_a = RongJS::runtime();
    let a = rt_a.context();
    install_call(&a, "throwError", thrower(&b, "new TypeError('from b')")?)?;
    install_call(&a, "throwString", thrower(&b, "'boom'")?)?;
    let check = "[e instanceof Error, e.message, e.code, e.reason].join('|')";

    assert_eq!(
        caught(&a, "throwError", check)?,
        "true|TypeError: from b|E_JS_THROWN|thrown_value_unreachable"
    );
    assert_eq!(
        caught(&a, "throwString", check)?,
        "true|boom|E_JS_THROWN|thrown_value_unreachable"
    );
    Ok(())
}

/// A thrown value is handed over once; rethrowing a copy of the same error
/// gives its text.
#[test]
fn a_thrown_value_rethrown_twice_keeps_its_text() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let target = thrower(&ctx, "new TypeError('once')")?;
    let kept: Rc<RefCell<Option<RongJSError>>> = Rc::default();
    let kept_in = kept.clone();
    ctx.global().set(
        "throwKept",
        JSFunc::new(&ctx, move || -> JSResult<()> {
            if let Some(err) = kept_in.borrow().clone() {
                return Err(err);
            }
            let err = target.call::<_, ()>(None, ()).unwrap_err();
            *kept_in.borrow_mut() = Some(err.clone());
            Err(err)
        })?,
    )?;

    assert_eq!(caught(&ctx, "throwKept", "String(e)")?, "TypeError: once");
    assert_eq!(
        caught(&ctx, "throwKept", "[e.message, e.reason].join('|')")?,
        "TypeError: once|thrown_value_unreachable"
    );
    kept.borrow_mut().take();
    Ok(())
}
