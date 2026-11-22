use rong_macro::FromJSObject;
use rong_test::*;

#[derive(FromJSObject)]
struct FetchOptions {
    url: String,
    retries: Option<u32>,
    #[js_name = "followRedirects"]
    follow_redirects: Option<bool>,
}

/// Runs `call` and returns `[name, code, message, data.argument]` of what it
/// threw, joined with `|`.
fn thrown(ctx: &JSContext, call: &str) -> JSResult<String> {
    ctx.eval(Source::from_bytes(format!(
        r#"
        (() => {{
            try {{ {call}; }} catch (e) {{
                const arg = e.data && e.data.argument !== undefined ? e.data.argument : '-';
                return [e.name, e.code, e.message, arg].join('|');
            }}
            return 'no error';
        }})()
        "#
    )))
}

fn install(ctx: &JSContext) -> JSResult<()> {
    let global = ctx.global();
    global.set("takeString", JSFunc::new(ctx, |s: String| s)?)?;
    global.set(
        "takeTwo",
        JSFunc::new(ctx, |s: String, n: i32| format!("{s}{n}"))?,
    )?;
    global.set("takeBool", JSFunc::new(ctx, |b: bool| b)?)?;
    global.set("takeF64", JSFunc::new(ctx, |n: f64| n)?)?;
    global.set("takeI32", JSFunc::new(ctx, |n: i32| n)?)?;
    global.set("takeU32", JSFunc::new(ctx, |n: u32| n)?)?;
    global.set("takeU8", JSFunc::new(ctx, |n: u8| n)?)?;
    global.set("takeI64", JSFunc::new(ctx, |n: i64| n)?)?;
    global.set("takeU64", JSFunc::new(ctx, |n: u64| n)?)?;
    global.set(
        "takeOptional",
        JSFunc::new(ctx, |a: String, b: Optional<String>| {
            format!("{a}:{}", b.0.unwrap_or_default())
        })?,
    )?;
    global.set(
        "takeRest",
        JSFunc::new(ctx, |rest: Rest<String>| rest.0.join(","))?,
    )?;
    global.set(
        "takeVec",
        JSFunc::new(ctx, |items: Vec<String>| items.join(","))?,
    )?;
    global.set(
        "takeOptions",
        JSFunc::new(ctx, |path: String, options: FetchOptions| {
            format!(
                "{path}:{}:{}:{}",
                options.url,
                options.retries.unwrap_or(0),
                options.follow_redirects.unwrap_or(false)
            )
        })?,
    )?;
    global.set(
        "takeCoerced",
        JSFunc::new(ctx, |s: Coerced<String>, n: Coerced<i32>| {
            format!("{}:{}", s.0, n.0)
        })?,
    )?;
    Ok(())
}

#[test]
fn a_string_parameter_refuses_other_types() {
    run(|ctx| {
        install(ctx)?;
        let s: String = ctx.eval(Source::from_bytes("takeString('ok')"))?;
        assert_eq!(s, "ok");
        assert_eq!(
            thrown(ctx, "takeString(undefined)")?,
            "TypeError|E_INVALID_ARG|argument 1 must be a string, got undefined|1"
        );
        assert_eq!(
            thrown(ctx, "takeString(42)")?,
            "TypeError|E_INVALID_ARG|argument 1 must be a string, got number|1"
        );
        assert_eq!(
            thrown(ctx, "takeString(null)")?,
            "TypeError|E_INVALID_ARG|argument 1 must be a string, got null|1"
        );
        assert_eq!(
            thrown(ctx, "takeString({})")?,
            "TypeError|E_INVALID_ARG|argument 1 must be a string, got object|1"
        );
        assert_eq!(
            thrown(ctx, "takeTwo('a', '5')")?,
            "TypeError|E_INVALID_ARG|argument 2 must be an integer, got string|2"
        );
        Ok(())
    });
}

#[test]
fn boolean_and_number_parameters_refuse_other_types() {
    run(|ctx| {
        install(ctx)?;
        assert_eq!(
            thrown(ctx, "takeBool(1)")?,
            "TypeError|E_INVALID_ARG|argument 1 must be a boolean, got number|1"
        );
        assert_eq!(
            thrown(ctx, "takeF64('5')")?,
            "TypeError|E_INVALID_ARG|argument 1 must be a number, got string|1"
        );
        let nan: bool = ctx.eval(Source::from_bytes("Number.isNaN(takeF64(NaN))"))?;
        assert!(nan);
        Ok(())
    });
}

#[test]
fn integer_parameters_do_not_truncate_or_wrap() {
    run(|ctx| {
        install(ctx)?;
        let n: i32 = ctx.eval(Source::from_bytes("takeI32(-7)"))?;
        assert_eq!(n, -7);
        assert_eq!(
            thrown(ctx, "takeI32(1.5)")?,
            "TypeError|E_INVALID_ARG|argument 1 must be an integer, got 1.5|1"
        );
        assert_eq!(
            thrown(ctx, "takeI32(NaN)")?,
            "TypeError|E_INVALID_ARG|argument 1 must be an integer, got NaN|1"
        );
        assert_eq!(
            thrown(ctx, "takeI32(2 ** 31)")?,
            "RangeError|E_INVALID_ARG|argument 1 must be an integer from -2147483648 to 2147483647, got 2147483648|1"
        );
        assert_eq!(
            thrown(ctx, "takeU32(-1)")?,
            "RangeError|E_INVALID_ARG|argument 1 must be an integer from 0 to 4294967295, got -1|1"
        );
        assert_eq!(
            thrown(ctx, "takeU8(300)")?,
            "RangeError|E_INVALID_ARG|argument 1 must be an integer from 0 to 255, got 300|1"
        );
        assert_eq!(
            thrown(ctx, "takeI64(2 ** 53)")?,
            "RangeError|E_INVALID_ARG|argument 1 must be a safe integer or a bigint that fits i64, got 9007199254740992|1"
        );
        let big: String = ctx.eval(Source::from_bytes("String(takeI64(2n ** 60n))"))?;
        assert_eq!(big, (1i64 << 60).to_string());
        assert_eq!(
            thrown(ctx, "takeU64(-1n)")?,
            "RangeError|E_INVALID_ARG|argument 1 must be a non-negative safe integer or a bigint that fits u64, got -1n|1"
        );
        Ok(())
    });
}

#[test]
fn optional_rest_and_array_parameters_name_the_position() {
    run(|ctx| {
        install(ctx)?;
        let s: String = ctx.eval(Source::from_bytes("takeOptional('a', undefined)"))?;
        assert_eq!(s, "a:");
        assert_eq!(
            thrown(ctx, "takeOptional('a', 5)")?,
            "TypeError|E_INVALID_ARG|argument 2 must be a string, got number|2"
        );
        assert_eq!(
            thrown(ctx, "takeRest('a', 'b', 3)")?,
            "TypeError|E_INVALID_ARG|argument 3 must be a string, got number|3"
        );
        assert_eq!(
            thrown(ctx, "takeVec(['a', 2])")?,
            "TypeError|E_INVALID_ARG|argument 1: `[1]` must be a string, got number|1"
        );
        Ok(())
    });
}

#[test]
fn object_fields_name_the_field_and_the_position() {
    run(|ctx| {
        install(ctx)?;
        let s: String = ctx.eval(Source::from_bytes(
            "takeOptions('p', { url: 'u', retries: 2, followRedirects: true })",
        ))?;
        assert_eq!(s, "p:u:2:true");
        assert_eq!(
            thrown(ctx, "takeOptions('p', { url: 5 })")?,
            "TypeError|E_INVALID_ARG|argument 2: `url` must be a string, got number|2"
        );
        assert_eq!(
            thrown(ctx, "takeOptions('p', { url: 'u', retries: 1.5 })")?,
            "TypeError|E_INVALID_ARG|argument 2: `retries` must be an integer, got 1.5|2"
        );
        assert_eq!(
            thrown(
                ctx,
                "takeOptions('p', { url: 'u', followRedirects: 'yes' })"
            )?,
            "TypeError|E_INVALID_ARG|argument 2: `followRedirects` must be a boolean, got string|2"
        );
        assert_eq!(
            thrown(ctx, "takeOptions('p', {})")?,
            "TypeError|E_INVALID_ARG|argument 2: `url` is required|2"
        );
        assert_eq!(
            thrown(ctx, "takeOptions('p', 'u')")?,
            "TypeError|E_INVALID_ARG|argument 2 must be an object, got string|2"
        );
        Ok(())
    });
}

#[test]
fn coerced_parameters_keep_javascript_conversion() {
    run(|ctx| {
        install(ctx)?;
        let s: String = ctx.eval(Source::from_bytes("takeCoerced(42, 1.9)"))?;
        assert_eq!(s, "42:1");
        let s: String = ctx.eval(Source::from_bytes("takeCoerced(undefined, -1.5)"))?;
        assert_eq!(s, "undefined:-1");
        Ok(())
    });
}

#[test]
fn conversions_outside_arguments_are_strict_too() {
    run(|ctx| {
        let value: JSValue = ctx.eval(Source::from_bytes("42"))?;
        let err = value.clone().to_rust::<String>().unwrap_err();
        assert!(err.is_code(rong::error::E_TYPE));
        assert!(err.is_value_mismatch());
        assert_eq!(
            err.to_string(),
            "E_TYPE: value must be a string, got number"
        );
        assert_eq!(value.to_string_coerced()?, "42");

        let obj: JSObject = ctx.eval(Source::from_bytes("({ n: '1' })"))?;
        let err = obj.get::<_, u32>("n").unwrap_err();
        assert!(err.is_code(rong::error::E_TYPE));

        let n: f64 = ctx.eval(Source::from_bytes("1.5"))?;
        assert_eq!(n, 1.5);
        let err = ctx.eval::<i32>(Source::from_bytes("1.5")).unwrap_err();
        assert!(err.is_value_mismatch());
        Ok(())
    });
}

#[derive(FromJSObject, Default)]
struct Endpoint {
    url: String,
}

#[derive(FromJSObject)]
struct WithRequired {
    inner: Endpoint,
}

#[derive(FromJSObject)]
struct WithOptional {
    inner: Option<Endpoint>,
}

#[derive(FromJSObject)]
struct WithDefault {
    #[js_default]
    inner: Endpoint,
}

#[derive(FromJSObject)]
struct TwoLevels {
    outer: WithRequired,
}

fn install_nested(ctx: &JSContext) -> JSResult<()> {
    let global = ctx.global();
    global.set(
        "takeRequired",
        JSFunc::new(ctx, |o: WithRequired| o.inner.url)?,
    )?;
    global.set(
        "takeOptionalInner",
        JSFunc::new(ctx, |o: WithOptional| {
            o.inner.map(|e| e.url).unwrap_or_else(|| "none".into())
        })?,
    )?;
    global.set(
        "takeDefault",
        JSFunc::new(ctx, |o: WithDefault| format!("<{}>", o.inner.url))?,
    )?;
    global.set(
        "takeTwoLevels",
        JSFunc::new(ctx, |o: TwoLevels| o.outer.inner.url)?,
    )?;
    Ok(())
}

#[test]
fn a_nested_missing_field_is_reported_with_its_full_path() {
    run(|ctx| {
        install_nested(ctx)?;
        let nested_missing = "TypeError|E_INVALID_ARG|argument 1: `inner.url` is required|1";

        assert_eq!(thrown(ctx, "takeRequired({ inner: {} })")?, nested_missing);
        assert_eq!(
            thrown(ctx, "takeRequired({})")?,
            "TypeError|E_INVALID_ARG|argument 1: `inner` is required|1"
        );

        // Inside `Option`, a present but incomplete object is an error, not `None`.
        assert_eq!(
            thrown(ctx, "takeOptionalInner({ inner: {} })")?,
            nested_missing
        );
        let s: String = ctx.eval(Source::from_bytes("takeOptionalInner({})"))?;
        assert_eq!(s, "none");
        let s: String = ctx.eval(Source::from_bytes("takeOptionalInner({ inner: null })"))?;
        assert_eq!(s, "none");

        // With `#[js_default]`, only an absent property takes the default.
        assert_eq!(thrown(ctx, "takeDefault({ inner: {} })")?, nested_missing);
        let s: String = ctx.eval(Source::from_bytes("takeDefault({})"))?;
        assert_eq!(s, "<>");

        assert_eq!(
            thrown(ctx, "takeTwoLevels({ outer: { inner: {} } })")?,
            "TypeError|E_INVALID_ARG|argument 1: `outer.inner.url` is required|1"
        );
        assert_eq!(
            thrown(ctx, "takeTwoLevels({ outer: { inner: { url: 5 } } })")?,
            "TypeError|E_INVALID_ARG|argument 1: `outer.inner.url` must be a string, got number|1"
        );
        let s: String = ctx.eval(Source::from_bytes(
            "takeTwoLevels({ outer: { inner: { url: 'u' } } })",
        ))?;
        assert_eq!(s, "u");
        Ok(())
    });
}

#[derive(FromJSObject)]
struct Kinds {
    items: Vec<String>,
    callback: Option<JSFunc>,
    config: Option<JSObject>,
    list: Option<JSArray>,
    buffer: Option<JSArrayBuffer>,
    bytes: Option<JSTypedArray<u8>>,
    view: Option<AnyJSTypedArray>,
    when: Option<JSDate>,
}

#[test]
fn object_array_and_function_parameters_name_the_position() {
    run(|ctx| {
        let global = ctx.global();
        global.set("takeObject", JSFunc::new(ctx, |_o: JSObject| true)?)?;
        global.set("takeArray", JSFunc::new(ctx, |_a: JSArray| true)?)?;
        global.set(
            "takeVecArg",
            JSFunc::new(ctx, |v: Vec<String>| v.len() as u32)?,
        )?;
        global.set("takeFunc", JSFunc::new(ctx, |_f: JSFunc| true)?)?;
        global.set("takeBuffer", JSFunc::new(ctx, |_b: JSArrayBuffer| true)?)?;
        global.set("takeBytes", JSFunc::new(ctx, |_t: JSTypedArray<u8>| true)?)?;
        global.set("takeView", JSFunc::new(ctx, |_t: AnyJSTypedArray| true)?)?;

        let cases = [
            ("takeObject(5)", "argument 1 must be an object, got number"),
            ("takeObject(null)", "argument 1 must be an object, got null"),
            ("takeArray({})", "argument 1 must be an array, got object"),
            (
                "takeVecArg('ab')",
                "argument 1 must be an array, got string",
            ),
            ("takeFunc({})", "argument 1 must be a function, got object"),
            (
                "takeBuffer([])",
                "argument 1 must be an ArrayBuffer, got array",
            ),
            (
                "takeBytes(new Int8Array(2))",
                "argument 1 must be a Uint8Array, got Int8Array",
            ),
            (
                "takeView(1)",
                "argument 1 must be a typed array, got number",
            ),
        ];
        for (call, message) in cases {
            assert_eq!(
                thrown(ctx, call)?,
                format!("TypeError|E_INVALID_ARG|{message}|1"),
                "{call}"
            );
        }
        let ok: bool = ctx.eval(Source::from_bytes(
            "takeObject({}) && takeArray([]) && takeFunc(() => 1) \
             && takeBuffer(new ArrayBuffer(1)) && takeBytes(new Uint8Array(1)) \
             && takeView(new Int8Array(1))",
        ))?;
        assert!(ok);
        Ok(())
    });
}

#[test]
fn object_array_and_function_fields_name_the_path() {
    run(|ctx| {
        ctx.global().set(
            "takeKinds",
            JSFunc::new(ctx, |k: Kinds| {
                (k.items.len()
                    + usize::from(k.callback.is_some())
                    + usize::from(k.config.is_some())
                    + usize::from(k.list.is_some())
                    + usize::from(k.buffer.is_some())
                    + usize::from(k.bytes.is_some())
                    + usize::from(k.view.is_some())
                    + usize::from(k.when.is_some())) as u32
            })?,
        )?;

        let cases = [
            ("{ items: 'x' }", "`items` must be an array, got string"),
            (
                "{ items: ['a', 2] }",
                "`items[1]` must be a string, got number",
            ),
            (
                "{ items: [], callback: 1 }",
                "`callback` must be a function, got number",
            ),
            (
                "{ items: [], config: 'x' }",
                "`config` must be an object, got string",
            ),
            (
                "{ items: [], list: {} }",
                "`list` must be an array, got object",
            ),
            (
                "{ items: [], buffer: [] }",
                "`buffer` must be an ArrayBuffer, got array",
            ),
            (
                "{ items: [], bytes: new Int8Array(1) }",
                "`bytes` must be a Uint8Array, got Int8Array",
            ),
            (
                "{ items: [], view: 5 }",
                "`view` must be a typed array, got number",
            ),
            (
                "{ items: [], when: 0 }",
                "`when` must be a Date, got number",
            ),
        ];
        for (arg, message) in cases {
            assert_eq!(
                thrown(ctx, &format!("takeKinds({arg})"))?,
                format!("TypeError|E_INVALID_ARG|argument 1: {message}|1"),
                "{arg}"
            );
        }
        let n: u32 = ctx.eval(Source::from_bytes(
            "takeKinds({ items: ['a'], callback() {}, config: {}, list: [], \
             buffer: new ArrayBuffer(1), bytes: new Uint8Array(1), \
             view: new Float32Array(1), when: new Date() })",
        ))?;
        assert_eq!(n, 8);
        Ok(())
    });
}

#[test]
fn coerced_f64_and_bool_are_the_strict_conversion() {
    run(|ctx| {
        let global = ctx.global();
        global.set("takeCoercedF64", JSFunc::new(ctx, |n: Coerced<f64>| n.0)?)?;
        global.set("takeCoercedBool", JSFunc::new(ctx, |b: Coerced<bool>| b.0)?)?;
        assert_eq!(
            thrown(ctx, "takeCoercedF64('5')")?,
            "TypeError|E_INVALID_ARG|argument 1 must be a number, got string|1"
        );
        assert_eq!(
            thrown(ctx, "takeCoercedBool(1)")?,
            "TypeError|E_INVALID_ARG|argument 1 must be a boolean, got number|1"
        );
        let n: f64 = ctx.eval(Source::from_bytes("takeCoercedF64(1.5)"))?;
        assert_eq!(n, 1.5);
        Ok(())
    });
}
