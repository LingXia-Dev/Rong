pub use rong::*;
// Re-export commonly used types for tests
pub use rong::function::{Constructor, Optional, Rest, This, ThisMut};

#[cfg(feature = "http")]
pub mod http {
    pub use axum;

    pub async fn spawn_axum(app: axum::Router) -> std::io::Result<std::net::SocketAddr> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;

        tokio::spawn(async move {
            if let Err(err) = axum::serve(listener, app).await {
                eprintln!("axum test server exited with error: {}", err);
            }
        });

        Ok(addr)
    }
}

pub fn thrown_js_value(ctx: &JSContext, err: &RongJSError) -> JSResult<JSValue> {
    err.thrown_value(ctx)
        .ok_or_else(|| HostError::new(rong::error::E_INTERNAL, "Expected thrown JS value").into())
}

pub fn thrown_object(ctx: &JSContext, err: &RongJSError) -> JSResult<JSObject> {
    let thrown = thrown_js_value(ctx, err)?;
    thrown.into_object().ok_or_else(|| {
        HostError::new(
            rong::error::E_INTERNAL,
            "Expected thrown value to be an object",
        )
        .into()
    })
}

pub fn thrown_object_prop<T>(ctx: &JSContext, err: &RongJSError, key: &str) -> JSResult<T>
where
    T: FromJSValue<JSEngineValue>,
{
    thrown_object(ctx, err)?.get(key)
}

pub fn thrown_error_message(ctx: &JSContext, err: &RongJSError) -> JSResult<String> {
    thrown_object_prop(ctx, err, "message")
}

pub fn thrown_error_stack(ctx: &JSContext, err: &RongJSError) -> JSResult<String> {
    thrown_object_prop(ctx, err, "stack")
}

// Helper function to run tests with JS context
#[allow(dead_code)]
pub fn run<F: FnOnce(&JSContext) -> JSResult<()>>(f: F) {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    f(&ctx).unwrap();
}

#[macro_export]
macro_rules! async_run {
    ($user_fn:expr) => {{
        let rong = Rong::<RongJS>::builder().shared().build().unwrap();

        let call_closure = |runtime: JSRuntime, _receiver| {
            let ctx = runtime.context();
            $user_fn(ctx)
        };

        rong.call_blocking::<_, _, ()>(call_closure).unwrap();
    }};
}

pub struct UnitJSRunner<'a> {
    ctx: &'a JSContext,
}

const TEST_RUNTIME_JS: &str = include_str!("../../../packages/test/src/runtime.js");

impl<'a> UnitJSRunner<'a> {
    async fn load_runtime(ctx: &JSContext) -> JSResult<()> {
        ctx.eval_async::<()>(Source::from_bytes(TEST_RUNTIME_JS))
            .await
    }

    /// Load the shared test runtime and evaluate JavaScript test source.
    pub async fn load_source(ctx: &'a JSContext, source: &str) -> JSResult<Self> {
        Self::load_runtime(ctx).await?;
        ctx.eval_async::<()>(Source::from_bytes(source)).await?;
        Ok(Self { ctx })
    }

    /// Load the shared test runtime and a script from `tests/unit`.
    pub async fn load_script(ctx: &'a JSContext, unit: &str) -> JSResult<Self> {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let base_path = manifest_dir.join("../../tests/unit");

        let test_base_dir = match std::fs::canonicalize(&base_path) {
            Ok(path) => path,
            Err(e) => {
                return Err(HostError::new(
                    rong::error::E_INTERNAL,
                    format!(
                        "Failed to canonicalize test base path '{}': {}",
                        base_path.display(),
                        e
                    ),
                )
                .into());
            }
        };

        Self::load_runtime(ctx).await?;
        let test_path = test_base_dir.join(unit);
        let source = Source::from_path(ctx, test_path).await?;
        ctx.eval_async::<()>(source).await?;

        Ok(Self { ctx })
    }

    /// Run all tests and return true if all tests passed
    pub async fn run(&self) -> JSResult<bool> {
        let result: bool = self
            .ctx
            .eval_async(Source::from_bytes(
                r#"(async () => {
                    const report = await globalThis.__RONG_TEST__.run();
                    globalThis.__RONG_TEST_REPORT__ = report;
                    return report.failed === 0;
                })()"#,
            ))
            .await?;

        if !result {
            let details: String = self
                .ctx
                .eval_async(Source::from_bytes(
                    "JSON.stringify(globalThis.__RONG_TEST_REPORT__)",
                ))
                .await
                .unwrap_or_else(|_| "<failed to serialize test report>".to_string());
            eprintln!("JS unit tests failed: {}", details);
        }

        Ok(result)
    }
}
