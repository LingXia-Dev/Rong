use rong_test::*;
use std::collections::{BTreeMap, HashMap};

#[test]
fn test_throw_error() {
    run(|ctx| {
        let error = ctx.throw_syntax_error("Invalid syntax");
        assert!(error.is_exception());

        let error = ctx.throw_type_error("Invalid type");
        assert!(error.is_exception());

        let error = ctx.throw_reference_error("Undefined variable");
        assert!(error.is_exception());

        let error = ctx
            .eval::<()>(Source::from_bytes(b"throw new Error('throw-error')"))
            .unwrap_err();
        let thrown = thrown_js_value(ctx, &error)?;
        let error = String::from_js_value(ctx, thrown.clone())?;
        assert!(
            error.contains("throw-error"),
            "Expected error message to contain 'throw-error', but got: {}",
            error
        );
        Ok(())
    });
}

#[test]
fn test_throw_primitive_value() {
    run(|ctx| {
        let err = ctx.eval::<()>(Source::from_bytes(b"throw 1")).unwrap_err();
        let thrown = thrown_js_value(ctx, &err)?;
        let s = String::from_js_value(ctx, thrown)?;
        assert_eq!(s, "1");
        Ok(())
    });
}

#[test]
fn test_eval_returns_error_object_as_value() {
    run(|ctx| {
        let value: JSValue = ctx.eval(Source::from_bytes(br#"new Error("x")"#))?;
        assert!(value.is_error());
        assert!(!value.is_exception());

        let obj = value
            .into_object()
            .expect("Expected returned Error to be an object");
        let message: String = obj.get("message")?;
        assert_eq!(message, "x");
        Ok(())
    });
}

#[test]
fn test_error_constructor() {
    run(|ctx| {
        // Register multiple error constructors
        ctx.global().set(
            "type_error",
            JSFunc::new(ctx, || -> JSResult<()> {
                Err(
                    HostError::new(rong::error::E_INVALID_ARG, "this is typeError")
                        .with_name("TypeError")
                        .into(),
                )
            })?,
        )?;

        ctx.global().set(
            "reference_error",
            JSFunc::new(ctx, || -> JSResult<()> {
                Err(HostError::new(
                    rong::error::E_MISSING_PROPERTY,
                    "Property 'dummy' Not Found",
                )
                .with_name("ReferenceError")
                .into())
            })?,
        )?;

        ctx.global().set(
            "range_error",
            JSFunc::new(ctx, || -> JSResult<()> {
                Err(HostError::new(
                    rong::error::E_OUT_OF_RANGE,
                    "Invalid TypedArray range: offset or length exceeds buffer size",
                )
                .with_name("RangeError")
                .into())
            })?,
        )?;

        // Test TypeError
        let type_error = ctx
            .eval::<String>(Source::from_bytes(
                b"try {
                    type_error();
                } catch (e) {
                    e.constructor.name + ': ' + e.message
                }",
            ))
            .unwrap();
        assert_eq!(type_error, "TypeError: this is typeError");

        // Test ReferenceError
        let reference_error = ctx
            .eval::<String>(Source::from_bytes(
                b"try {
                    reference_error();
                } catch (e) {
                    e.constructor.name + ': ' + e.message
                }",
            ))
            .unwrap();
        assert_eq!(
            reference_error,
            "ReferenceError: Property 'dummy' Not Found"
        );

        // Test RangeError
        let range_error = ctx
            .eval::<String>(Source::from_bytes(
                b"try {
                    range_error();
                } catch (e) {
                    e.constructor.name + ': ' + e.message
                }",
            ))
            .unwrap();
        assert_eq!(
            range_error,
            "RangeError: Invalid TypedArray range: offset or length exceeds buffer size"
        );
        Ok(())
    });
}

#[test]
fn test_error_stack() {
    run(|ctx| {
        // test syntax error
        let result = ctx.eval::<()>(Source::from_bytes(b"function test() { a b c }"));
        let err = result.unwrap_err();
        let message = thrown_error_message(ctx, &err)?;
        assert!(!message.is_empty(), "Should have error message");
        // Javascriptcore only have value on message
        // assert!(error.stack.is_some(), "Should have stack trace");

        // test Reference Error
        let result = ctx.eval::<()>(Source::from_bytes(
            b"
            function foo() {
                return bar(); // undefined variable
            }
            foo();
        ",
        ));
        let err = result.unwrap_err();
        let message = thrown_error_message(ctx, &err)?;
        assert!(
            message.contains("bar"),
            "Error message should mention undefined variable"
        );
        let stack = thrown_error_stack(ctx, &err)?;
        assert!(
            stack.contains("foo"),
            "Stack trace should contain function name"
        );

        // test type error
        let result = ctx.eval::<()>(Source::from_bytes(
            b"
            let obj = null;
            obj.property;  // TypeError: Cannot read property of null
        ",
        ));
        let err = result.unwrap_err();
        let message = thrown_error_message(ctx, &err)?;
        assert!(
            message.contains("null"),
            "Error message should mention null"
        );
        Ok(())
    });
}

#[test]
fn test_custom_error() {
    run(|ctx| {
        // Test custom errors and stack traces
        let result = ctx.eval::<()>(Source::from_bytes(
            b"
            function throwCustomError() {
                throw new Error('Custom error message');
            }

            function caller() {
                throwCustomError();
            }

            caller();
        ",
        ));

        let err = result.unwrap_err();
        let message = thrown_error_message(ctx, &err)?;
        assert_eq!(message, "Custom error message");
        let stack = thrown_error_stack(ctx, &err)?;
        assert!(
            stack.contains("throwCustomError"),
            "Stack should contain throwCustomError"
        );
        assert!(stack.contains("caller"), "Stack should contain caller");
        Ok(())
    });
}

#[test]
fn test_error_conversion() {
    run(|ctx| {
        // Test conversion of different types of errors
        let cases = [
            (b"throw new TypeError('type error')" as &[u8], "type error"),
            (b"throw new ReferenceError('ref error')", "ref error"),
            (b"throw new SyntaxError('syntax error')", "syntax error"),
            (b"throw new Error('general error')", "general error"),
        ];

        for (code, expected_msg) in cases {
            let err = ctx.eval::<()>(Source::from_bytes(code)).unwrap_err();
            let message = thrown_error_message(ctx, &err)?;
            assert_eq!(message, expected_msg);
        }
        Ok(())
    });
}

#[test]
fn test_error_data_common_conversions() {
    let mut tree = BTreeMap::new();
    tree.insert("path".to_string(), "/tmp/demo");
    tree.insert("kind".to_string(), "io");

    let mut hash = HashMap::new();
    hash.insert("code".to_string(), 5u32);
    hash.insert("retry".to_string(), 1u32);

    assert_eq!(
        rong::error::ErrorData::from(None::<i32>),
        rong::error::ErrorData::Null
    );
    assert_eq!(
        rong::error::ErrorData::from(Some(7i32)),
        rong::error::ErrorData::from(7i32)
    );
    assert_eq!(
        rong::error::ErrorData::from(vec![1u32, 2u32, 3u32]),
        rong::error::ErrorData::Array(vec![
            rong::error::ErrorData::from(1u32),
            rong::error::ErrorData::from(2u32),
            rong::error::ErrorData::from(3u32),
        ])
    );
    assert_eq!(
        rong::error::ErrorData::from(["a", "b"]),
        rong::error::ErrorData::Array(vec![
            rong::error::ErrorData::from("a"),
            rong::error::ErrorData::from("b"),
        ])
    );
    assert!(matches!(
        rong::error::ErrorData::from(tree),
        rong::error::ErrorData::Object(map)
            if map.get("path") == Some(&rong::error::ErrorData::from("/tmp/demo"))
                && map.get("kind") == Some(&rong::error::ErrorData::from("io"))
    ));
    assert!(matches!(
        rong::error::ErrorData::from(hash),
        rong::error::ErrorData::Object(map)
            if map.get("code") == Some(&rong::error::ErrorData::from(5u32))
                && map.get("retry") == Some(&rong::error::ErrorData::from(1u32))
    ));
}

#[test]
fn test_error_helpers_use_host_error_builders() {
    run(|ctx| {
        ctx.global().set(
            "error_with_aliases",
            JSFunc::new(ctx, || -> JSResult<()> {
                let data =
                    HostError::new(rong::error::E_IO, "alias test").with_data(rong::err_data!({
                        os_error: (Some(5i32)),
                        tags: (vec!["fs", "read"]),
                    }));
                let err: RongJSError = data.into();
                let host = err.as_host_error().expect("expected host error");
                assert!(matches!(
                    host.data.as_ref(),
                    Some(rong::error::ErrorData::Object(map))
                        if map.get("os_error") == Some(&rong::error::ErrorData::from(5i32))
                            && map.get("tags")
                                == Some(&rong::error::ErrorData::from(vec!["fs", "read"]))
                ));
                Err(HostError::new(
                    rong::error::E_MISSING_PROPERTY,
                    "Property 'missing' Not Found",
                )
                .with_name("ReferenceError")
                .into())
            })?,
        )?;

        let result = ctx.eval::<String>(Source::from_bytes(
            br#"try { error_with_aliases(); } catch (e) { `${e.name}:${e.message}` }"#,
        ))?;
        assert_eq!(result, "ReferenceError:Property 'missing' Not Found");
        Ok(())
    });
}

#[test]
fn test_error_display() {
    run(|ctx| {
        // Test formatting of error messages
        let result = ctx.eval::<()>(Source::from_bytes(
            b"
            function foo() {
                throw new Error('test error');
            }
            foo();
        ",
        ));

        let err = result.unwrap_err();
        let message = thrown_error_message(ctx, &err)?;
        let stack = thrown_error_stack(ctx, &err)?;
        assert!(
            message.contains("test error"),
            "Error string should contain message"
        );
        assert!(
            stack.contains("foo"),
            "Error string should contain stack trace"
        );
        Ok(())
    });
}

/// JavaScriptCore stringifies an exception leaving a C API call for the Web
/// Inspector, before Rong receives it: script-visible reads Rong did not make.
/// True when JavaScriptCore is the primary engine; the cfg follows `src/lib.rs`.
const ENGINE_STRINGIFIES_EXCEPTIONS: bool = cfg!(any(
    feature = "primary-jscore",
    all(
        feature = "jscore",
        not(feature = "primary-quickjs"),
        not(feature = "primary-arkjs")
    )
));

#[test]
fn a_thrown_value_prints_what_was_thrown() {
    run(|ctx| {
        let text = |source: &str| {
            ctx.eval::<()>(Source::from_bytes(source))
                .unwrap_err()
                .to_string()
        };
        assert_eq!(
            text("throw new TypeError('bad input')"),
            "TypeError: bad input"
        );
        assert_eq!(text("throw new Error('')"), "Error");
        assert_eq!(text("throw 'plain text'"), "plain text");
        assert_eq!(text("throw 42"), "42");
        // An object's toString may run script, so Rong does not call it.
        assert_eq!(
            text("throw { toString() { globalThis.ran = true; return 'x' } }"),
            "JavaScript threw a value"
        );
        if !ENGINE_STRINGIFIES_EXCEPTIONS {
            assert!(ctx.eval::<bool>(Source::from_bytes("globalThis.ran === undefined"))?);
        }
        Ok(())
    });
}

#[test]
fn describing_a_thrown_error_converts_nothing_to_a_string() {
    run(|ctx| {
        let text = |source: &str| {
            ctx.eval::<()>(Source::from_bytes(source))
                .unwrap_err()
                .to_string()
        };
        // Only a `name` / `message` that already is a string is printed.
        assert_eq!(
            text(
                "var e = new Error('x'); \
                 e.message = { toString() { globalThis.coerced = true; return 'm' } }; throw e"
            ),
            "Error"
        );
        assert_eq!(
            text("var e = new Error('x'); e.message = undefined; throw e"),
            "Error"
        );
        assert_eq!(text("var e = new Error('x'); e.name = 7; throw e"), "x");
        assert_eq!(text("throw ''"), "JavaScript threw a value");
        if !ENGINE_STRINGIFIES_EXCEPTIONS {
            assert!(ctx.eval::<bool>(Source::from_bytes("globalThis.coerced === undefined"))?);
        }
        Ok(())
    });
}

#[test]
fn describing_a_thrown_error_runs_no_script() {
    run(|ctx| {
        let text = |source: &str| {
            ctx.eval::<()>(Source::from_bytes(source))
                .unwrap_err()
                .to_string()
        };
        ctx.eval::<()>(Source::from_bytes("globalThis.ran = [];"))?;

        // An accessor is recognised and not called, own or inherited.
        assert_eq!(
            text(
                "var e = new Error('x'); \
                 Object.defineProperty(e, 'message', { get() { ran.push('message'); return 'g' } }); \
                 throw e"
            ),
            "Error"
        );
        assert_eq!(
            text(
                "class Loud extends Error { get name() { ran.push('name'); return 'Loud' } } \
                 throw new Loud('quiet')"
            ),
            "quiet"
        );
        // A Proxy is not looked into, thrown or on the prototype chain.
        assert_eq!(
            text(
                "var trap = new Proxy({}, { get(t, k) { ran.push('trap ' + String(k)); \
                 return Reflect.get(t, k) } }); \
                 throw new Proxy(new Error('x'), trap)"
            ),
            "JavaScript threw a value"
        );
        assert_eq!(
            text(
                "var e = new Error('x'); \
                 Object.setPrototypeOf(e, new Proxy(Error.prototype, trap)); throw e"
            ),
            "JavaScript threw a value"
        );
        // Built-ins a script replaced are not the ones used.
        assert_eq!(
            text(
                "Object.getOwnPropertyDescriptor = () => { ran.push('descriptor') }; \
                 Object.getPrototypeOf = () => { ran.push('prototype') }; \
                 Object.prototype.hasOwnProperty = () => { ran.push('hasOwn') }; \
                 Object.defineProperty(Object.prototype, 'value', { get() { ran.push('value') } }); \
                 throw new RangeError('still read')"
            ),
            "RangeError: still read"
        );
        // A promise job queued by the script is left for the event loop.
        assert_eq!(
            text(
                "globalThis.jobs = 0; Promise.resolve().then(() => { jobs++ }); \
                 throw new Error('now')"
            ),
            "Error: now"
        );
        // An object that only inherits from `Error.prototype` is an error too.
        assert_eq!(text("throw Object.create(Error.prototype)"), "Error");

        let ran: String = ctx.eval(Source::from_bytes("ran.join()"))?;
        if ENGINE_STRINGIFIES_EXCEPTIONS {
            // JavaScriptCore's own stringification reads `name` and `message`
            // (through a Proxy's `get` trap too). Nothing Rong's read would
            // reach has run.
            for reached in [
                "descriptor",
                "prototype",
                "hasOwn",
                "value",
                "trap getOwn",
                "trap getProto",
            ] {
                assert!(!ran.contains(reached), "{reached} ran: {ran}");
            }
        } else {
            assert_eq!(ran, "");
        }
        // JavaScriptCore and ArkJS drain promise jobs themselves as an
        // evaluation ends, before Rong sees its result.
        if !(ENGINE_STRINGIFIES_EXCEPTIONS || cfg!(target_env = "ohos")) {
            assert_eq!(ctx.eval::<i32>(Source::from_bytes("globalThis.jobs"))?, 0);
        }
        Ok(())
    });
}
