use rong_test::*;

const APP_ERROR_CLASS: &str = r#"
    globalThis.AppError = class AppError extends Error {
        constructor(message) {
            super(message);
            this.name = 'AppError';
        }
    };
    AppError
"#;

fn register_app_error(ctx: &JSContext, name: &str) -> JSResult<()> {
    let ctor: JSFunc = ctx.eval(Source::from_bytes(APP_ERROR_CLASS))?;
    ctx.register_error_class(name, ctor)
}

fn eval_bool(ctx: &JSContext, source: &str) -> JSResult<bool> {
    ctx.eval(Source::from_bytes(source))
}

#[test]
fn a_host_error_is_an_instance_of_the_registered_class() {
    run(|ctx| {
        register_app_error(ctx, "AppError")?;
        ctx.global().set(
            "fail",
            JSFunc::new(ctx, || -> JSResult<()> {
                Err(HostError::new(rong::error::E_NOT_FOUND, "no such item")
                    .with_name("AppError")
                    .into())
            })?,
        )?;

        assert!(eval_bool(
            ctx,
            r#"
            (() => {
                try { fail(); } catch (e) {
                    return e instanceof AppError
                        && e instanceof Error
                        && e.name === 'AppError'
                        && e.message === 'no such item'
                        && e.code === 'E_NOT_FOUND'
                        && Object.getPrototypeOf(e) === AppError.prototype
                        && typeof e.stack === 'string';
                }
                return false;
            })()
            "#,
        )?);
        Ok(())
    });
}

#[test]
fn registering_error_covers_every_plain_host_error() {
    run(|ctx| {
        register_app_error(ctx, "Error")?;
        ctx.global().set(
            "fail",
            JSFunc::new(ctx, || -> JSResult<()> {
                Err(HostError::new(rong::error::E_IO, "disk").into())
            })?,
        )?;
        ctx.global().set(
            "failType",
            JSFunc::new(ctx, || -> JSResult<()> {
                Err(HostError::new(rong::error::E_TYPE, "bad")
                    .with_name("TypeError")
                    .into())
            })?,
        )?;

        assert!(eval_bool(
            ctx,
            r#"
            (() => {
                let plain, typed;
                try { fail(); } catch (e) { plain = e; }
                try { failType(); } catch (e) { typed = e; }
                return plain instanceof AppError && plain.code === 'E_IO'
                    && typed instanceof TypeError && !(typed instanceof AppError);
            })()
            "#,
        )?);
        Ok(())
    });
}

#[test]
fn an_async_rejection_is_an_instance_of_the_registered_class() {
    async_run!(|ctx: JSContext| async move {
        register_app_error(&ctx, "AppError")?;
        ctx.global().set(
            "failLater",
            JSFunc::new(&ctx, || async {
                Err::<(), _>(
                    HostError::new(rong::error::E_TIMEOUT, "too slow")
                        .with_name("AppError")
                        .with_reason("upstream_timeout")
                        .into(),
                )
            })?,
        )?;

        let ok: bool = ctx
            .eval_async(Source::from_bytes(
                r#"
                (async () => {
                    try { await failLater(); } catch (e) {
                        return e instanceof AppError && e.code === 'E_TIMEOUT'
                            && e.reason === 'upstream_timeout';
                    }
                    return false;
                })()
                "#,
            ))
            .await?;
        assert!(ok);
        Ok(())
    });
}

#[test]
fn replacing_the_global_does_not_change_the_registered_class() {
    run(|ctx| {
        register_app_error(ctx, "AppError")?;
        ctx.global().set(
            "fail",
            JSFunc::new(ctx, || -> JSResult<()> {
                Err(HostError::new(rong::error::E_IO, "x")
                    .with_name("AppError")
                    .into())
            })?,
        )?;
        assert!(eval_bool(
            ctx,
            r#"
            (() => {
                const Original = AppError;
                globalThis.AppError = class Fake {};
                try { fail(); } catch (e) { return e instanceof Original; }
                return false;
            })()
            "#,
        )?);
        Ok(())
    });
}

#[test]
fn a_constructor_that_throws_falls_back_to_the_built_in_error() {
    run(|ctx| {
        let ctor: JSFunc = ctx.eval(Source::from_bytes(
            "(class Broken extends Error { constructor() { throw 1; } })",
        ))?;
        ctx.register_error_class("Broken", ctor)?;
        ctx.global().set(
            "fail",
            JSFunc::new(ctx, || -> JSResult<()> {
                Err(HostError::new(rong::error::E_IO, "still here")
                    .with_name("Broken")
                    .into())
            })?,
        )?;
        assert!(eval_bool(
            ctx,
            r#"
            (() => {
                try { fail(); } catch (e) {
                    return e instanceof Error && e.name === 'Broken'
                        && e.message === 'still here' && e.code === 'E_IO';
                }
                return false;
            })()
            "#,
        )?);
        Ok(())
    });
}

#[test]
fn a_non_constructor_is_refused() {
    run(|ctx| {
        let arrow: JSFunc = ctx.eval(Source::from_bytes("(() => {})"))?;
        let err = ctx.register_error_class("Arrow", arrow).unwrap_err();
        assert!(err.is_code(rong::error::E_INVALID_ARG));
        Ok(())
    });
}

#[test]
fn reason_and_detail_are_own_properties_only_when_set() {
    run(|ctx| {
        ctx.global().set(
            "withBoth",
            JSFunc::new(ctx, || -> JSResult<()> {
                Err(HostError::new(rong::error::E_NOT_SUPPORTED, "upgrade")
                    .with_reason("host_upgrade_required")
                    .with_detail("needs host 2.1")
                    .into())
            })?,
        )?;
        ctx.global().set(
            "withNeither",
            JSFunc::new(ctx, || -> JSResult<()> {
                Err(HostError::new(rong::error::E_NOT_SUPPORTED, "plain").into())
            })?,
        )?;

        assert!(eval_bool(
            ctx,
            r#"
            (() => {
                let both, neither;
                try { withBoth(); } catch (e) { both = e; }
                try { withNeither(); } catch (e) { neither = e; }
                return both.reason === 'host_upgrade_required'
                    && both.detail === 'needs host 2.1'
                    && Object.prototype.hasOwnProperty.call(both, 'reason')
                    && Object.prototype.hasOwnProperty.call(both, 'detail')
                    && !('reason' in neither) && !('detail' in neither);
            })()
            "#,
        )?);
        Ok(())
    });
}

#[test]
fn a_rethrown_host_error_keeps_reason_and_detail() {
    run(|ctx| {
        ctx.global().set(
            "fail",
            JSFunc::new(ctx, || -> JSResult<()> {
                Err(HostError::new(rong::error::E_INVALID_STATE, "conflict")
                    .with_reason("revision_conflict")
                    .with_detail("expected 3, found 4")
                    .into())
            })?,
        )?;

        // JavaScript catches the host error and throws it on; Rust converts
        // the thrown value back into a host error.
        let err = ctx
            .eval::<()>(Source::from_bytes("try { fail(); } catch (e) { throw e; }"))
            .unwrap_err()
            .into_host_in(ctx);
        let host = err.as_host_error().expect("host error");
        assert_eq!(host.reason(), Some("revision_conflict"));
        assert_eq!(host.detail(), Some("expected 3, found 4"));

        // Thrown into JavaScript again, the fields are still there.
        ctx.global().set(
            "rethrow",
            JSFunc::new(ctx, move || -> JSResult<()> { Err(err.clone()) })?,
        )?;
        assert!(eval_bool(
            ctx,
            r#"
            (() => {
                try { rethrow(); } catch (e) {
                    return e.reason === 'revision_conflict'
                        && e.detail === 'expected 3, found 4';
                }
                return false;
            })()
            "#,
        )?);
        Ok(())
    });
}

#[test]
fn unregistered_names_keep_the_built_in_errors() {
    run(|ctx| {
        ctx.eval::<()>(Source::from_bytes(
            "globalThis.NetworkError = class NetworkError extends Error {};",
        ))?;
        ctx.global().set(
            "fail",
            JSFunc::new(ctx, || -> JSResult<()> {
                Err(HostError::new(rong::error::E_NETWORK, "offline")
                    .with_name("NetworkError")
                    .into())
            })?,
        )?;
        ctx.global().set(
            "failUri",
            JSFunc::new(ctx, || -> JSResult<()> {
                Err(HostError::new(rong::error::E_INVALID_ARG, "bad uri")
                    .with_name("URIError")
                    .into())
            })?,
        )?;
        // A global of the same name is not used: only registered classes are.
        assert!(eval_bool(
            ctx,
            r#"
            (() => {
                let net, uri;
                try { fail(); } catch (e) { net = e; }
                try { failUri(); } catch (e) { uri = e; }
                return !(net instanceof NetworkError) && net instanceof Error
                    && net.name === 'NetworkError' && net.code === 'E_NETWORK'
                    && uri instanceof URIError;
            })()
            "#,
        )?);
        Ok(())
    });
}
