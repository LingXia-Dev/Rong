//! S3-compatible object storage module for RongJS.
//!
//! S3 client API inspired by Bun's `S3Client`, adapted for RongJS.

mod client;
mod config;
mod file;
mod types;

pub use client::S3Client;
pub use config::S3Config;
pub use file::S3File;
pub use types::{
    S3ClientOptions, S3ListEntry, S3ListOptions, S3ListResult, S3PresignOptions, S3StatResult,
    S3WriteOptions,
};

use rong::*;

rong::js_api! {
    fn register_s3_namespace(ctx) {
        namespace RongNamespace = ctx.host_namespace();
        class S3Client = S3Client;
    }
}

pub(crate) fn register_s3_classes(ctx: &JSContext) -> JSResult<()> {
    ctx.register_hidden_class::<S3File>()?;
    ctx.register_hidden_class::<S3Client>()?;
    Ok(())
}

/// Register `Rong.S3Client` and keep S3File internal-only.
pub fn init(ctx: &JSContext) -> JSResult<()> {
    register_s3_classes(ctx)?;
    register_s3_namespace(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestS3Server {
        endpoint: String,
        access_key_id: String,
        secret_access_key: String,
    }

    /// Spawn a local S3-compatible server backed by s3s-fs.
    /// Returns the endpoint and credentials generated for this server instance.
    async fn spawn_s3_server() -> TestS3Server {
        let tmp = tempfile::tempdir().expect("tempdir");
        let fs = s3s_fs::FileSystem::new(tmp.path()).expect("s3s-fs");

        // Pre-create the test bucket directory so S3 operations work immediately.
        std::fs::create_dir_all(tmp.path().join("test-bucket")).expect("create bucket dir");

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let access_key_id = format!("test-access-{}-{nonce}", std::process::id());
        let secret_access_key = format!("test-secret-{}-{nonce}", std::process::id());
        let mut auth = s3s::auth::SimpleAuth::new();
        auth.register(access_key_id.clone(), secret_access_key.clone().into());

        let mut builder = s3s::service::S3ServiceBuilder::new(fs);
        builder.set_auth(auth);
        let service = builder.build();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local_addr");

        // Leak tempdir so it lives for the duration of the process.
        let _tmp = Box::leak(Box::new(tmp));

        tokio::spawn(async move {
            let http_server =
                hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new());
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    continue;
                };
                let service = service.clone();
                let builder = http_server.clone();
                tokio::spawn(async move {
                    let _ = builder
                        .serve_connection(hyper_util::rt::TokioIo::new(stream), service)
                        .await;
                });
            }
        });

        TestS3Server {
            endpoint: format!("http://127.0.0.1:{}", addr.port()),
            access_key_id,
            secret_access_key,
        }
    }

    fn setup_s3_env(ctx: &JSContext, server: &TestS3Server) -> JSResult<()> {
        ctx.global()
            .set("TEST_S3_ENDPOINT", server.endpoint.as_str())?;
        ctx.global()
            .set("TEST_S3_ACCESS_KEY", server.access_key_id.as_str())?;
        ctx.global()
            .set("TEST_S3_SECRET_KEY", server.secret_access_key.as_str())?;
        ctx.global().set("TEST_S3_BUCKET", "test-bucket")?;

        rong_console::init(ctx)?;
        rong_assert::init(ctx)?;
        init(ctx)?;

        Ok(())
    }

    fn test_s3_config(server: &TestS3Server) -> S3Config {
        S3Config {
            access_key_id: server.access_key_id.clone(),
            secret_access_key: server.secret_access_key.clone(),
            bucket: "test-bucket".to_string(),
            endpoint: Some(server.endpoint.clone()),
            ..Default::default()
        }
    }

    #[test]
    fn test_s3() {
        let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        std::env::set_current_dir(&workspace_root).expect("set cwd");

        async_run!(|ctx: JSContext| async move {
            unsafe {
                std::env::set_var("NO_PROXY", "127.0.0.1,localhost");
            }

            let server = spawn_s3_server().await;
            setup_s3_env(&ctx, &server)?;

            let passed = UnitJSRunner::load_script(&ctx, "s3.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }

    #[test]
    fn test_s3_namespace() {
        let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        std::env::set_current_dir(&workspace_root).expect("set cwd");

        async_run!(|ctx: JSContext| async move {
            unsafe {
                std::env::set_var("NO_PROXY", "127.0.0.1,localhost");
            }

            let server = spawn_s3_server().await;
            setup_s3_env(&ctx, &server)?;

            // Create a pre-configured client with namespace prefix from Rust.
            // JS never calls `new Rong.S3Client`.
            let client = S3Client::new(test_s3_config(&server)).with_namespace("app1/")?;
            ctx.host_namespace()
                .set("s3", client.into_js_object(&ctx)?)?;

            let passed = UnitJSRunner::load_script(&ctx, "s3_namespace.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }

    #[test]
    fn test_s3_injected_api() {
        async_run!(|ctx: JSContext| async move {
            assert!(
                S3Client::new(S3Config::default())
                    .with_namespace("")
                    .is_err()
            );
            let client = S3Client::new(S3Config::default());
            ctx.host_namespace()
                .set("s3", client.into_js_object(&ctx)?)?;
            assert!(ctx.eval::<bool>(Source::from_bytes(
                r#"typeof Rong.s3.write === "function"
                    && typeof Rong.s3.file === "function"
                    && typeof Rong.S3Client === "undefined"
                    && typeof S3File === "undefined"
                    && (() => {
                        try {
                            Rong.s3.file("key", { endpoint: "http://untrusted.invalid" });
                            return false;
                        } catch (error) {
                            return error.name === "TypeError"
                                && error.message.includes("host-injected S3Client");
                        }
                    })()"#,
            ))?);
            Ok(())
        });
    }
}
