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

/// Register `Rong.S3Client` and keep S3File internal-only.
pub fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_hidden_class::<S3File>()?;
    ctx.register_hidden_class::<S3Client>()?;
    let ctor = Class::lookup::<S3Client>(ctx)?.clone();
    ctx.host_namespace().set("S3Client", ctor)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;

    /// Spawn a local S3-compatible server backed by s3s-fs.
    /// Returns the `http://127.0.0.1:{port}` endpoint string.
    async fn spawn_s3_server() -> String {
        let tmp = tempfile::tempdir().expect("tempdir");
        let fs = s3s_fs::FileSystem::new(tmp.path()).expect("s3s-fs");

        // Pre-create the test bucket directory so S3 operations work immediately.
        std::fs::create_dir_all(tmp.path().join("test-bucket")).expect("create bucket dir");

        let mut auth = s3s::auth::SimpleAuth::new();
        auth.register("minioadmin".to_string(), "minioadmin".into());

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

        format!("http://127.0.0.1:{}", addr.port())
    }

    fn setup_s3_env(ctx: &JSContext, endpoint: &str) -> JSResult<()> {
        ctx.global().set("TEST_S3_ENDPOINT", endpoint)?;
        ctx.global().set("TEST_S3_ACCESS_KEY", "minioadmin")?;
        ctx.global().set("TEST_S3_SECRET_KEY", "minioadmin")?;
        ctx.global().set("TEST_S3_BUCKET", "test-bucket")?;

        rong_console::init(ctx)?;
        rong_assert::init(ctx)?;
        init(ctx)?;

        Ok(())
    }

    fn test_s3_config(endpoint: &str) -> S3Config {
        S3Config {
            access_key_id: "minioadmin".to_string(),
            secret_access_key: "minioadmin".to_string(),
            bucket: "test-bucket".to_string(),
            endpoint: Some(endpoint.to_string()),
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

            let endpoint = spawn_s3_server().await;
            setup_s3_env(&ctx, &endpoint)?;

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

            let endpoint = spawn_s3_server().await;
            setup_s3_env(&ctx, &endpoint)?;

            // Create a pre-configured client with namespace prefix from Rust,
            // then inject it as a global `s3` — JS never calls `new Rong.S3Client`.
            let client = S3Client::new(test_s3_config(&endpoint), Some("app1/".to_string()));
            let js_client = Class::lookup::<S3Client>(&ctx)?.instance(client);
            ctx.global().set("s3", js_client)?;

            let passed = UnitJSRunner::load_script(&ctx, "s3_namespace.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }
}
