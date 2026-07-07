use crate::config::{S3Config, S3ConfigOverlay};
use crate::file::{S3File, resolve_body};
use crate::types::{
    S3ClientListOptions, S3ClientOptions, S3ClientPresignOptions, S3ClientWriteOptions,
    S3ListEntry, S3ListResult, S3StatResult,
};
use rong::function::*;
use rong::*;
use std::rc::Rc;

fn s3_error(msg: impl Into<String>) -> RongJSError {
    HostError::new("ERR_S3", msg).with_name("S3Error").into()
}

/// S3-compatible object storage client.
#[js_export]
pub struct S3Client {
    pub(crate) config: Rc<S3Config>,
    namespace_prefix: Option<String>,
}

impl S3Client {
    /// Create an `S3Client` from Rust with a pre-built config.
    ///
    /// This is the primary Rust-side API for creating pre-configured clients,
    /// useful for environments that inject instances via a platform namespace
    /// instead of exposing the JS constructor.
    pub fn new(config: S3Config, namespace_prefix: Option<String>) -> Self {
        Self {
            config: config.into_rc(),
            namespace_prefix,
        }
    }

    fn prefixed_path(&self, path: &str) -> String {
        match self.namespace_prefix.as_deref() {
            Some(prefix) if !prefix.is_empty() => format!("{prefix}{path}"),
            _ => path.to_string(),
        }
    }

    fn namespaced(&self) -> bool {
        matches!(self.namespace_prefix.as_deref(), Some(prefix) if !prefix.is_empty())
    }

    fn reject_config_override<T: S3ConfigOverlay>(&self, options: &Optional<T>) -> JSResult<()> {
        if !self.namespaced() {
            return Ok(());
        }

        let Some(options) = options.0.as_ref() else {
            return Ok(());
        };

        if let Some(key) = options.config_override_key() {
            return Err(HostError::new(
                "E_INVALID_ARG",
                format!(
                    "Cannot override S3 config field '{key}' on a namespaced injected S3Client"
                ),
            )
            .with_name("TypeError")
            .into());
        }

        Ok(())
    }
}

#[js_class]
impl S3Client {
    #[js_method(constructor)]
    fn js_new(options: Optional<S3ClientOptions>) -> JSResult<Self> {
        let config = S3Config::from_options(options.0);
        Ok(Self {
            config: config.into_rc(),
            namespace_prefix: None,
        })
    }

    /// Lazy file reference — no network request.
    #[js_method(ts_return = "S3File")]
    fn file(
        &self,
        ctx: JSContext,
        path: String,
        options: Optional<S3ClientOptions>,
    ) -> JSResult<JSObject> {
        self.reject_config_override(&options)?;
        let config = if let Some(ref options) = options.0 {
            self.config.merge_options(Some(options)).into_rc()
        } else {
            self.config.clone()
        };
        let file = S3File::create(config, self.prefixed_path(&path), path);
        Ok(Class::lookup::<S3File>(&ctx)?.instance(file))
    }

    #[js_method(
        ts_args = "path: string, data: string | ArrayBuffer | Uint8Array, options?: S3WriteOptions & S3ClientOptions"
    )]
    async fn write(
        &self,
        path: String,
        data: JSValue,
        options: Optional<S3ClientWriteOptions>,
    ) -> JSResult<f64> {
        self.reject_config_override(&options)?;
        let path = self.prefixed_path(&path);
        let config = if let Some(ref options) = options.0 {
            self.config.merge_options(Some(options))
        } else {
            (*self.config).clone()
        };
        let bucket = config.create_bucket()?;
        let (content_bytes, content_type) = resolve_body(&data)?;
        let ct = if let Some(ref options) = options.0 {
            options.content_type.clone().or(content_type)
        } else {
            content_type
        };
        let ct_str = ct.as_deref().unwrap_or("application/octet-stream");

        bucket
            .put_object_with_content_type(&path, &content_bytes, ct_str)
            .await
            .map_err(|e| s3_error(format!("PUT {}: {}", path, e)))?;

        Ok(content_bytes.len() as f64)
    }

    #[js_method]
    async fn delete(&self, path: String) -> JSResult<()> {
        let path = self.prefixed_path(&path);
        let bucket = self.config.create_bucket()?;
        bucket
            .delete_object(&path)
            .await
            .map_err(|e| s3_error(format!("DELETE {}: {}", path, e)))?;
        Ok(())
    }

    #[js_method]
    async fn unlink(&self, path: String) -> JSResult<()> {
        Self::delete(self, path).await
    }

    #[js_method]
    async fn exists(&self, path: String) -> JSResult<bool> {
        let path = self.prefixed_path(&path);
        let bucket = self.config.create_bucket()?;
        match bucket.head_object(&path).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    #[js_method]
    async fn size(&self, path: String) -> JSResult<f64> {
        let path = self.prefixed_path(&path);
        let bucket = self.config.create_bucket()?;
        let (head, _status) = bucket
            .head_object(&path)
            .await
            .map_err(|e| s3_error(format!("HEAD {}: {}", path, e)))?;
        Ok(head.content_length.unwrap_or(0) as f64)
    }

    #[js_method]
    async fn stat(&self, path: String) -> JSResult<S3StatResult> {
        let path = self.prefixed_path(&path);
        let bucket = self.config.create_bucket()?;
        let (head, _status) = bucket
            .head_object(&path)
            .await
            .map_err(|e| s3_error(format!("HEAD {}: {}", path, e)))?;

        Ok(S3StatResult {
            etag: head.e_tag,
            last_modified: head.last_modified,
            size: head.content_length.unwrap_or(0) as f64,
            content_type: head.content_type,
        })
    }

    #[js_method(ts_args = "path: string, options?: S3PresignOptions & S3ClientOptions")]
    async fn presign(
        &self,
        path: String,
        options: Optional<S3ClientPresignOptions>,
    ) -> JSResult<String> {
        self.reject_config_override(&options)?;
        let path = self.prefixed_path(&path);
        let config = if let Some(ref options) = options.0 {
            self.config.merge_options(Some(options))
        } else {
            (*self.config).clone()
        };
        let bucket = config.create_bucket()?;

        let expires_in = options
            .0
            .as_ref()
            .and_then(|o| o.expires_in)
            .map(|v| v as u32)
            .unwrap_or(86400);

        let method = options
            .0
            .as_ref()
            .and_then(|o| o.method.clone())
            .unwrap_or_else(|| "GET".to_string());

        match method.to_uppercase().as_str() {
            "GET" => bucket
                .presign_get(&path, expires_in, None)
                .await
                .map_err(|e| s3_error(format!("presign GET: {}", e))),
            "PUT" => bucket
                .presign_put(&path, expires_in, None, None)
                .await
                .map_err(|e| s3_error(format!("presign PUT: {}", e))),
            "DELETE" => bucket
                .presign_delete(&path, expires_in)
                .await
                .map_err(|e| s3_error(format!("presign DELETE: {}", e))),
            other => Err(HostError::new(
                "ERR_S3_INVALID_METHOD",
                format!("Unsupported presign method: {}", other),
            )
            .into()),
        }
    }

    #[js_method(ts_args = "options?: S3ListOptions & S3ClientOptions")]
    async fn list(&self, options: Optional<S3ClientListOptions>) -> JSResult<S3ListResult> {
        self.reject_config_override(&options)?;
        let config = if let Some(ref options) = options.0 {
            self.config.merge_options(Some(options))
        } else {
            (*self.config).clone()
        };
        let bucket = config.create_bucket()?;

        let user_prefix = options
            .0
            .as_ref()
            .and_then(|o| o.prefix.clone())
            .unwrap_or_default();

        // Combine namespace prefix with user-provided prefix
        let prefix = self.prefixed_path(&user_prefix);

        let max_keys = options
            .0
            .as_ref()
            .and_then(|o| o.max_keys)
            .map(|v| v as usize);

        let start_after = options
            .0
            .as_ref()
            .and_then(|o| o.start_after.clone())
            .map(|s| self.prefixed_path(&s));

        let results = bucket
            .list(prefix, None)
            .await
            .map_err(|e| s3_error(format!("LIST: {}", e)))?;

        let mut contents = Vec::new();
        let mut total_count = 0usize;
        let mut is_truncated = false;

        let ns_prefix = self.namespace_prefix.as_deref().unwrap_or("");

        'outer: for page in &results {
            for obj in &page.contents {
                if let Some(ref after) = start_after
                    && obj.key <= *after
                {
                    continue;
                }
                if let Some(max) = max_keys
                    && total_count >= max
                {
                    is_truncated = true;
                    break 'outer;
                }

                // Strip namespace prefix from returned keys
                let key = obj.key.strip_prefix(ns_prefix).unwrap_or(&obj.key);

                contents.push(S3ListEntry {
                    key: key.to_string(),
                    size: obj.size as f64,
                    last_modified: obj.last_modified.to_string(),
                    etag: obj.e_tag.clone(),
                });
                total_count += 1;
            }
        }

        Ok(S3ListResult {
            contents,
            is_truncated,
        })
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}
