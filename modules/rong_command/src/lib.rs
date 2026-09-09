//! Command execution APIs attached to `globalThis.Rong`.

mod child_process;
mod io;
mod shell;
mod sync_process;

use rong::{
    HostError, IntoJSValue, JSArray, JSContext, JSContextService, JSObject, JSResult, JSValue,
};
use std::env;
use std::sync::Arc;
use std::time::Duration;

/// Native authority checked at each process operation and while children run.
///
/// The JavaScript namespace cannot install or replace this object. Embedders
/// should bind it to the exact live session that owns process execution.
/// A failing `authorize` is fail-closed: in-flight children are terminated and
/// retained handles refuse further work.
pub trait ProcessAuthority: Send + Sync + 'static {
    fn authorize(&self) -> Result<(), String>;
}

#[derive(Clone)]
struct ProcessAuthorityService(Arc<dyn ProcessAuthority>);

impl JSContextService for ProcessAuthorityService {}

pub(crate) fn process_authority(ctx: &JSContext) -> Option<Arc<dyn ProcessAuthority>> {
    ctx.get_service::<ProcessAuthorityService>()
        .map(|service| Arc::clone(&service.0))
}

pub(crate) fn authorize_process(ctx: &JSContext) -> JSResult<()> {
    authorize_installed_process(process_authority(ctx))
}

fn authorize_installed_process(authority: Option<Arc<dyn ProcessAuthority>>) -> JSResult<()> {
    match authority {
        Some(authority) => authorize_process_with(authority.as_ref()),
        None => Ok(()),
    }
}

pub(crate) fn authorize_process_with(authority: &dyn ProcessAuthority) -> JSResult<()> {
    authority
        .authorize()
        .map_err(|message| HostError::new(rong::error::E_PERMISSION_DENIED, message).into())
}

pub(crate) async fn wait_for_process_revocation(authority: Arc<dyn ProcessAuthority>) {
    loop {
        if authority.authorize().is_err() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn create_env_object(ctx: &JSContext) -> JSResult<JSObject> {
    let env_obj = JSObject::new(ctx);
    for (key, value) in env::vars() {
        env_obj.set(key.as_str(), value)?;
    }
    Ok(env_obj)
}

fn create_string_array(
    ctx: &JSContext,
    values: impl IntoIterator<Item = String>,
) -> JSResult<JSValue> {
    let array = JSArray::new(ctx)?;
    for value in values {
        array.push(value)?;
    }
    Ok(array.into_js_value(ctx))
}

fn install_command_apis(ctx: &JSContext) -> JSResult<()> {
    let rong = ctx.host_namespace();
    rong.set("env", create_env_object(ctx)?)?;
    rong.set("argv", create_string_array(ctx, env::args())?)?;
    rong.set("args", create_string_array(ctx, env::args().skip(2))?)?;

    rong_buffer::init(ctx)?;
    rong_encoding::init(ctx)?;
    rong_abort::init(ctx)?;
    rong_stream::init(ctx)?;
    io::init(ctx)?;
    child_process::init(ctx)?;
    sync_process::init(ctx)?;
    shell::init(ctx)?;
    Ok(())
}

/// Install command APIs with no host authority.
///
/// Process operations are unrestricted. Prefer [`init_with_authority`] when the
/// embedder must bind execution to a live session grant.
pub fn init(ctx: &JSContext) -> JSResult<()> {
    install_command_apis(ctx)
}

/// Initialize the command namespace with a sealed, per-context authority.
///
/// A second installation is rejected so later native modules cannot silently
/// replace the session authority selected by the embedder.
pub fn init_with_authority(ctx: &JSContext, authority: Arc<dyn ProcessAuthority>) -> JSResult<()> {
    if ctx.get_service::<ProcessAuthorityService>().is_some() {
        return Err(HostError::new(
            rong::error::E_ALREADY_EXISTS,
            "process authority is already sealed for this context",
        )
        .into());
    }
    authority
        .authorize()
        .map_err(|message| HostError::new(rong::error::E_PERMISSION_DENIED, message))?;
    ctx.set_service(ProcessAuthorityService(authority));
    install_command_apis(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong::Source;
    use rong_test::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct RevocableAuthority(AtomicBool);

    impl ProcessAuthority for RevocableAuthority {
        fn authorize(&self) -> Result<(), String> {
            self.0
                .load(Ordering::SeqCst)
                .then_some(())
                .ok_or_else(|| "revoked".to_string())
        }
    }

    fn run_unit_suite(unit: &str) {
        let unit = unit.to_string();
        async_run!(|ctx: JSContext| async move {
            rong_assert::init(&ctx)?;
            rong_console::init(&ctx)?;
            init(&ctx)?;

            let passed = UnitJSRunner::load_script(&ctx, &unit).await?.run().await?;
            assert!(passed);

            Ok(())
        });
    }

    fn run_stdio_suite(mode: &str) {
        let mode = mode.to_string();
        async_run!(|ctx: JSContext| async move {
            rong_assert::init(&ctx)?;
            rong_console::init(&ctx)?;
            let stdio = io::install_captured_stdio(&ctx, b"stdin-payload".to_vec());
            init(&ctx)?;
            ctx.global().set("__stdioMode", mode.clone())?;

            let passed = UnitJSRunner::load_script(&ctx, "stdio.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            if mode == "text" {
                assert_eq!(
                    String::from_utf8(stdio.stdout_bytes()).unwrap(),
                    "hello-out"
                );
                assert_eq!(String::from_utf8(stdio.stderr_bytes()).unwrap(), "warn-err");
            }

            Ok(())
        });
    }

    #[test]
    fn test_command_namespace() {
        for unit in ["spawn.js", "shell.js"] {
            run_unit_suite(unit);
        }
        run_stdio_suite("text");
        run_stdio_suite("bytes");
    }

    #[tokio::test]
    async fn revocation_waiter_observes_authority_change() {
        let authority = Arc::new(RevocableAuthority(AtomicBool::new(true)));
        let task = tokio::spawn(wait_for_process_revocation(authority.clone()));
        authority.0.store(false, Ordering::SeqCst);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("revocation waiter must finish")
            .expect("waiter task");
    }

    #[test]
    fn allow_and_stale_authorities_are_checked_on_every_operation() {
        let authority = RevocableAuthority(AtomicBool::new(true));
        assert!(authorize_process_with(&authority).is_ok());
        authority.0.store(false, Ordering::SeqCst);
        let error = authorize_process_with(&authority).expect_err("stale authority must fail");
        assert!(error.to_string().contains("revoked"));
    }

    #[test]
    fn missing_authority_leaves_process_unrestricted() {
        authorize_installed_process(None).expect("init() embeddings stay unrestricted");
    }

    #[test]
    fn sealed_authority_cannot_be_replaced() {
        async_run!(|ctx: JSContext| async move {
            let first = Arc::new(RevocableAuthority(AtomicBool::new(true)));
            init_with_authority(&ctx, first)?;
            let second = Arc::new(RevocableAuthority(AtomicBool::new(true)));
            let error = init_with_authority(&ctx, second)
                .expect_err("a second installer must not replace the sealed authority");
            assert!(error.to_string().contains("already sealed"));
            Ok(())
        });
    }

    #[test]
    fn sealed_denial_rejects_spawn() {
        async_run!(|ctx: JSContext| async move {
            let authority = Arc::new(RevocableAuthority(AtomicBool::new(true)));
            init_with_authority(&ctx, authority.clone())?;
            authority.0.store(false, Ordering::SeqCst);
            let message: String = ctx.eval(Source::from_bytes(
                r#"
                (() => {
                  try {
                    Rong.spawn(['true']);
                    return 'did-not-throw';
                  } catch (e) {
                    return (e && e.message) ? String(e.message) : String(e);
                  }
                })()
                "#,
            ))?;
            assert_ne!(message, "did-not-throw");
            assert!(
                message.contains("revoked") || message.contains("PERMISSION"),
                "{message}"
            );
            Ok(())
        });
    }
}
