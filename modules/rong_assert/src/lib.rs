//! # Assert Module
//!
//! This module provides assertion functions similar to Node.js's `assert` module.
//!
//! ### JavaScript Usage
//!
//! ```javascript
//! assert.ok(true);
//! assert.equal(1, 1);
//! ```

use rong::{function::*, *};

/// Handles assertion errors with optional custom message
fn handle_assertion_error(
    ctx: &JSContext,
    message: Optional<JSValue>,
    default_message: &str,
) -> JSValue {
    message.0.map_or_else(
        || ctx.throw_error(default_message),
        |value| {
            if value.is_string() {
                let msg: String = value.to_rust().unwrap_or_default();
                ctx.throw_error(msg)
            } else {
                ctx.throw(value)
            }
        },
    )
}

fn get_loose_equal_fn(ctx: &JSContext) -> JSResult<JSFunc> {
    let assert_obj: JSObject = ctx.global().get("assert")?;
    if let Ok(existing) = assert_obj.get::<_, JSFunc>("__looseEqual") {
        return Ok(existing);
    }

    // Node's `assert.equal` uses loose equality (==).
    let func = ctx.eval::<JSFunc>(Source::from_bytes("(a, b) => a == b"))?;
    assert_obj.set("__looseEqual", func.clone())?;
    Ok(func)
}

/// Asserts that two values are equal.
fn equal(ctx: JSContext, left: JSValue, right: JSValue, message: Optional<JSValue>) -> JSValue {
    // JSValue's Rust `PartialEq` is identity-based (engine handle equality), so we must compare
    // using JavaScript semantics.
    let result = get_loose_equal_fn(&ctx).and_then(|f| f.call::<_, bool>(None, (left, right)));

    match result {
        Ok(true) => JSValue::undefined(&ctx),
        Ok(false) => handle_assertion_error(&ctx, message, "AssertionError: It's not equal!"),
        Err(e) => ctx.throw_error(format!("AssertionError internal: {e}")),
    }
}

/// Asserts that a value is truthy.
fn ok(ctx: JSContext, value: JSValue, message: Optional<JSValue>) -> JSValue {
    let undefined = JSValue::undefined(&ctx);
    let is_truthy = match value.type_of() {
        JSValueType::Boolean => value.to_rust::<bool>().unwrap_or(false),
        JSValueType::Number => value
            .to_rust::<f64>()
            .map(|n| n != 0.0 && !n.is_nan())
            .unwrap_or(false),
        JSValueType::String => value
            .to_rust::<String>()
            .map(|s| !s.is_empty())
            .unwrap_or(false),
        JSValueType::Array
        | JSValueType::BigInt
        | JSValueType::Constructor
        | JSValueType::Exception
        | JSValueType::Function
        | JSValueType::Symbol
        | JSValueType::Object => true,
        _ => false,
    };
    if is_truthy {
        return undefined;
    }

    handle_assertion_error(
        &ctx,
        message,
        "AssertionError: The expression was evaluated to a falsy value",
    )
}

/// Forces a test to fail with a custom message
fn fail(ctx: JSContext, message: Optional<JSValue>) -> JSValue {
    if let Some(msg) = message.0 {
        if msg.is_string() {
            let msg: String = msg.to_rust().unwrap_or_default();
            ctx.throw_error(msg)
        } else {
            ctx.throw(msg)
        }
    } else {
        ctx.throw_error("Failed")
    }
}

/// Asserts that a function does not throw an error
fn does_not_throw(ctx: JSContext, func: JSFunc, message: Optional<JSValue>) -> JSValue {
    // Call the function and check if it throws an error
    if func
        .call::<_, ()>(None, (JSValue::undefined(&ctx),))
        .is_err()
    {
        return handle_assertion_error(&ctx, message, "AssertionError: Got unwanted exception");
    }
    // If no error was thrown, return undefined
    JSValue::undefined(&ctx)
}

pub fn init(ctx: &JSContext) -> JSResult<()> {
    let ok = JSFunc::new(ctx, ok)?.name("ok")?;
    let equal = JSFunc::new(ctx, equal)?.name("equal")?;
    let fail = JSFunc::new(ctx, fail)?.name("fail")?;
    let does_not_throw = JSFunc::new(ctx, does_not_throw)?.name("doesNotThrow")?;

    ok.set("ok", ok.clone())?;
    ok.set("default", ok.clone())?;
    ok.set("equal", equal)?;
    ok.set("fail", fail)?;
    ok.set("doesNotThrow", does_not_throw)?;
    ctx.global().set("assert", ok)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;

    #[test]
    fn test_event() {
        async_run!(|ctx: JSContext| async move {
            ctx.global().set(
                "print",
                JSFunc::new(&ctx, |msg: String| println!("{}", msg)),
            )?;

            ctx.eval::<()>(Source::from_bytes(
                r#"
                    const console={
                        log: function(...args){
                            print(args.join(' '))
                        },
                        error: function(...args){
                            print(args.join(' '))
                        }
                    }
                "#,
            ))?;

            init(&ctx)?;
            let passed = UnitJSRunner::load_script(&ctx, "assert.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }
}
