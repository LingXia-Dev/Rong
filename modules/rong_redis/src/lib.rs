//! # Rong Redis Module
//!
//! Redis client API inspired by Bun's RedisClient, adapted for RongJS.
//!
//! Provides `Rong.RedisClient` with promise-based methods
//! for strings, hashes, sets, lists, pub/sub, and raw commands.

use rong::*;

mod redis;
pub use redis::*;

rong::js_api! {
    fn register_redis_namespace(ctx) {
        namespace RongNamespace = ctx.host_namespace();
        class RedisClient = RedisClient;
    }
}

pub(crate) fn register_redis_classes(ctx: &JSContext) -> JSResult<()> {
    ctx.register_hidden_class::<RedisClient>()?;
    ctx.register_hidden_class::<RedisSubscription>()?;
    Ok(())
}

/// Initialize the Redis module — exposes `Rong.RedisClient`.
pub fn init(ctx: &JSContext) -> JSResult<()> {
    register_redis_classes(ctx)?;
    register_redis_namespace(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;

    #[derive(Clone)]
    struct TestNamespaceState(Rc<RefCell<String>>);

    /// Start a redis-server on a random port. Returns (url, child).
    /// The child is killed when dropped via `kill_on_drop(true)`.
    async fn read_child_stderr(child: &mut tokio::process::Child) -> String {
        let Some(mut stderr) = child.stderr.take() else {
            return String::new();
        };

        let mut buf = Vec::new();
        if stderr.read_to_end(&mut buf).await.is_err() {
            return String::new();
        }

        String::from_utf8_lossy(&buf).trim().to_string()
    }

    async fn start_test_redis() -> Result<(String, tokio::process::Child), String> {
        // Find a free port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("cannot bind: {}", e))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("no local addr: {}", e))?
            .port();
        drop(listener);

        // Try common redis-server locations
        let mut candidates = vec![
            "redis-server",
            "/usr/local/opt/redis/bin/redis-server",
            "/opt/homebrew/opt/redis/bin/redis-server",
            "/usr/bin/redis-server",
            "/usr/local/bin/redis-server",
        ];
        if cfg!(windows) {
            candidates.insert(1, "redis-server.exe");
        }

        let bin = candidates.iter().find(|c| {
            std::process::Command::new(c)
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok()
        });

        let bin = match bin {
            Some(b) => *b,
            None => return Err("redis-server not found".to_string()),
        };

        let mut child = tokio::process::Command::new(bin)
            .args([
                "--bind",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--save",
                "",
                "--appendonly",
                "no",
                "--loglevel",
                "warning",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("failed to spawn redis-server: {}", e))?;

        // Wait for server to accept connections
        for _ in 0..150 {
            if let Some(status) = child
                .try_wait()
                .map_err(|e| format!("failed to poll redis-server: {}", e))?
            {
                let stderr = read_child_stderr(&mut child).await;
                let details = if stderr.is_empty() {
                    format!("redis-server exited early with status {}", status)
                } else {
                    format!(
                        "redis-server exited early with status {}: {}",
                        status, stderr
                    )
                };
                return Err(details);
            }

            if tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
                .await
                .is_ok()
            {
                let url = format!("redis://127.0.0.1:{}", port);
                return Ok((url, child));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let _ = child.kill().await;
        let _ = child.wait().await;
        let stderr = read_child_stderr(&mut child).await;
        let details = if stderr.is_empty() {
            "redis-server did not start in time".to_string()
        } else {
            format!("redis-server did not start in time: {}", stderr)
        };
        Err(details)
    }

    async fn setup_redis_env(
        ctx: &JSContext,
    ) -> Result<(String, tokio::process::Child), RongJSError> {
        let (url, child) = start_test_redis().await.map_err(|msg| {
            HostError::new(
                "E_TEST_SETUP",
                format!("Failed to start redis-server for rong_redis tests: {}", msg),
            )
        })?;

        ctx.global().set("TEST_REDIS_URL", url.as_str())?;
        rong_assert::init(ctx)?;
        rong_console::init(ctx)?;
        rong_abort::init(ctx)?;
        rong_timer::init(ctx)?;
        init(ctx)?;

        Ok((url, child))
    }

    fn is_missing_redis_server(err: &RongJSError) -> bool {
        format!("{err}").contains("redis-server not found")
    }

    #[test]
    fn test_redis() {
        async_run!(|ctx: JSContext| async move {
            let (_url, _child) = match setup_redis_env(&ctx).await {
                Ok(values) => values,
                Err(err) if is_missing_redis_server(&err) => return Ok(()),
                Err(err) => return Err(err),
            };

            let passed = UnitJSRunner::load_script(&ctx, "redis.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }

    #[test]
    fn test_redis_namespace() {
        async_run!(|ctx: JSContext| async move {
            let (url, _child) = match setup_redis_env(&ctx).await {
                Ok(values) => values,
                Err(err) if is_missing_redis_server(&err) => return Ok(()),
                Err(err) => return Err(err),
            };

            // Create a pre-configured client with namespace prefix from Rust.
            // JS never calls `new Rong.RedisClient`.
            let client = RedisClient::new(url).with_namespace("app1:")?;
            ctx.host_namespace()
                .set("redis", client.into_js_object(&ctx)?)?;

            let passed = UnitJSRunner::load_script(&ctx, "redis_namespace.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }

    #[test]
    fn test_redis_dynamic_namespace() {
        async_run!(|ctx: JSContext| async move {
            let (url, _child) = match setup_redis_env(&ctx).await {
                Ok(values) => values,
                Err(err) if is_missing_redis_server(&err) => return Ok(()),
                Err(err) => return Err(err),
            };

            let namespace_state = TestNamespaceState(Rc::new(RefCell::new("scope-a".to_owned())));
            ctx.set_state(namespace_state.clone());

            let client = RedisClient::new(url).with_namespace_resolver(|ctx| {
                let state = ctx.get_state::<TestNamespaceState>().ok_or_else(|| {
                    HostError::new("E_INVALID_STATE", "missing host namespace context")
                })?;
                Ok(format!("{}:", state.0.borrow().as_str()))
            });
            ctx.host_namespace()
                .set("redis", client.into_js_object(&ctx)?)?;

            let set_test_namespace = JSFunc::new(&ctx, move |namespace: String| {
                *namespace_state.0.borrow_mut() = namespace;
            })?;
            ctx.host_namespace()
                .set("__setTestNamespace", set_test_namespace)?;

            let empty_namespace = RedisClient::new("redis://127.0.0.1:1")
                .with_namespace_resolver(|_| Ok(String::new()));
            ctx.host_namespace()
                .set("emptyNamespaceRedis", empty_namespace.into_js_object(&ctx)?)?;

            let resolver_error =
                RedisClient::new("not a redis URL").with_namespace_resolver(|_| {
                    Err(HostError::new("E_TEST_NAMESPACE", "namespace unavailable")
                        .with_name("TypeError")
                        .into())
                });
            ctx.host_namespace()
                .set("resolverErrorRedis", resolver_error.into_js_object(&ctx)?)?;

            let passed = UnitJSRunner::load_script(&ctx, "redis_dynamic_namespace.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }

    #[test]
    fn test_redis_injected_api() {
        async_run!(|ctx: JSContext| async move {
            assert!(
                RedisClient::new("redis://127.0.0.1:1")
                    .with_namespace("")
                    .is_err()
            );
            let client = RedisClient::new("redis://127.0.0.1:1");
            ctx.host_namespace()
                .set("redis", client.into_js_object(&ctx)?)?;
            assert!(ctx.eval::<bool>(Source::from_bytes(
                r#"typeof Rong.redis.get === "function"
                    && typeof Rong.redis.subscribe === "function"
                    && typeof Rong.RedisClient === "undefined"
                    && typeof RedisSubscription === "undefined""#,
            ))?);
            Ok(())
        });
    }
}
