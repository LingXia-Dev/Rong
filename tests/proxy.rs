use rong_test::*;

const PROXY_UNIT_JS: &str = include_str!("unit/proxy.js");

#[test]
fn proxy_create_is_proxy_and_target() {
    run(|ctx| {
        let target = JSObject::new(ctx);
        target.set("value", 41)?;

        let get_trap: JSFunc = ctx.eval(Source::from_bytes(
            r#"(function(target, key, receiver) {
                if (key === "value") {
                    return 42;
                }
                return Reflect.get(target, key, receiver);
            })"#,
        ))?;

        let handler = JSObject::new(ctx);
        handler.set("get", get_trap)?;

        let proxy = JSProxy::new(ctx, target.clone(), handler)?;
        assert!(proxy.is_proxy());
        assert!(proxy.is_object());

        let proxy_target = proxy.target()?;
        assert_eq!(proxy_target.get::<_, i32>("value")?, 41);

        ctx.global().set("__proxy", proxy)?;
        assert_eq!(ctx.eval::<i32>(Source::from_bytes("__proxy.value"))?, 42);
        Ok(())
    });
}

#[test]
fn proxy_can_guard_function_calls() {
    run(|ctx| {
        let functions: JSObject = ctx.eval(Source::from_bytes(
            r#"({
                xx() { return "ok"; },
                yy() { return "blocked"; }
            })"#,
        ))?;

        let allow = JSObject::new(ctx);
        allow.set("xx", true)?;
        allow.set("yy", false)?;
        ctx.global().set("__allow", allow)?;

        let get_trap: JSFunc = ctx.eval(Source::from_bytes(
            r#"(function(target, key, receiver) {
                const value = Reflect.get(target, key, receiver);
                if (typeof value !== "function") {
                    return value;
                }
                return function(...args) {
                    if (!__allow[key]) {
                        throw new Error(`blocked:${String(key)}`);
                    }
                    return Reflect.apply(value, target, args);
                };
            })"#,
        ))?;

        let handler = JSObject::new(ctx);
        handler.set("get", get_trap)?;

        let proxy = JSProxy::new(ctx, functions, handler)?;
        ctx.global().set("guardedFunctions", proxy)?;

        assert_eq!(
            ctx.eval::<String>(Source::from_bytes("guardedFunctions.xx()"))?,
            "ok"
        );
        assert_eq!(
            ctx.eval::<String>(Source::from_bytes(
                r#"try { guardedFunctions.yy(); "nope"; } catch (e) { e.message }"#,
            ))?,
            "blocked:yy"
        );
        Ok(())
    });
}

#[test]
fn plain_js_proxy_is_detected_without_prewarm() {
    run(|ctx| {
        let proxy: JSValue = ctx.eval(Source::from_bytes(
            r#"(() => {
                const target = { id: "plain-target" };
                globalThis.__plain_proxy_target = target;
                return new Proxy(target, {});
            })()"#,
        ))?;

        assert!(proxy.is_proxy());

        let proxy = JSProxy::from_js_value(ctx, proxy)?;
        let target = proxy.target()?;
        let expected: JSObject = ctx.global().get("__plain_proxy_target")?;
        assert_eq!(target, expected);
        Ok(())
    });
}

#[test]
fn proxy_js_unit_tests() {
    async_run!(|ctx: JSContext| async move {
        ctx.global().set(
            "print",
            JSFunc::new(&ctx, |msg: String| {
                println!("{}", msg);
            })?,
        )?;

        ctx.eval::<()>(Source::from_bytes(
            r#"
                globalThis.console = {
                    log: (...args) => print(args.join(" ")),
                    error: (...args) => print(args.join(" ")),
                };
            "#,
        ))?;

        ctx.global().set(
            "createHostProxy",
            JSFunc::new(
                &ctx,
                |ctx: JSContext, target: JSObject, handler: JSObject| -> JSResult<JSProxy> {
                    JSProxy::new(&ctx, target, handler)
                },
            )?,
        )?;

        ctx.global().set(
            "isHostProxy",
            JSFunc::new(&ctx, |value: JSValue| Ok(value.is_proxy()))?,
        )?;

        ctx.global().set(
            "getHostProxyTarget",
            JSFunc::new(
                &ctx,
                |ctx: JSContext, value: JSValue| -> JSResult<JSObject> {
                    JSProxy::from_js_value(&ctx, value)?.target()
                },
            )?,
        )?;

        let passed = match async {
            let runner = UnitJSRunner::load_source(&ctx, PROXY_UNIT_JS).await?;
            runner.run().await
        }
        .await
        {
            Ok(passed) => passed,
            Err(err) => panic!(
                "proxy.js threw: {}",
                thrown_error_message(&ctx, &err).unwrap_or_else(|_| err.to_string())
            ),
        };
        assert!(passed);
        Ok(())
    });
}
