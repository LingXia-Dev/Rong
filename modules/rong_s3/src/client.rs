use crate::config::S3Config;
use crate::file::{S3File, resolve_body};
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

    fn reject_config_override(
        &self,
        options: &Optional<JSObject>,
        allowed_non_config_keys: &[&str],
    ) -> JSResult<()> {
        if !self.namespaced() {
            return Ok(());
        }

        let Some(obj) = options.0.as_ref() else {
            return Ok(());
        };

        let forbidden = [
            "accessKeyId",
            "secretAccessKey",
            "sessionToken",
            "region",
            "endpoint",
            "bucket",
            "acl",
            "virtualHostedStyle",
        ];

        for key in forbidden {
            if obj.has_property(key)? {
                return Err(HostError::new(
                    "E_INVALID_ARG",
                    format!(
                        "Cannot override S3 config field '{key}' on a namespaced injected S3Client"
                    ),
                )
                .with_name("TypeError")
                .into());
            }
        }

        for key_str in obj.keys_as::<String>()? {
            if !allowed_non_config_keys.contains(&key_str.as_str()) {
                return Err(HostError::new(
                    "E_INVALID_ARG",
                    format!("Option '{key_str}' is not allowed on a namespaced injected S3Client"),
                )
                .with_name("TypeError")
                .into());
            }
        }

        Ok(())
    }
}

#[js_class]
impl S3Client {
    #[js_method(constructor, ts_args = "options?: S3ClientOptions")]
    fn js_new(options: Optional<JSObject>) -> JSResult<Self> {
        let config = match options.0 {
            Some(ref obj) => S3Config::from_js_options(obj)?,
            None => S3Config::default(),
        };
        Ok(Self {
            config: config.into_rc(),
            namespace_prefix: None,
        })
    }

    /// Lazy file reference — no network request.
    #[js_method(
        ts_return = "S3File",
        ts_args = "path: string, options?: S3ClientOptions"
    )]
    fn file(
        &self,
        ctx: JSContext,
        path: String,
        options: Optional<JSObject>,
    ) -> JSResult<JSObject> {
        self.reject_config_override(&options, &[])?;
        let config = if let Some(ref obj) = options.0 {
            self.config.merge_js_options(obj)?.into_rc()
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
        options: Optional<JSObject>,
    ) -> JSResult<f64> {
        self.reject_config_override(&options, &["type"])?;
        let path = self.prefixed_path(&path);
        let config = if let Some(ref obj) = options.0 {
            self.config.merge_js_options(obj)?
        } else {
            (*self.config).clone()
        };
        let bucket = config.create_bucket()?;
        let (content_bytes, content_type) = resolve_body(&data)?;
        let ct = if let Some(ref opts) = options.0 {
            opts.get::<_, String>("type").ok().or(content_type)
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

    #[js_method(ts_return = "S3StatResult")]
    async fn stat(&self, ctx: JSContext, path: String) -> JSResult<JSObject> {
        let path = self.prefixed_path(&path);
        let bucket = self.config.create_bucket()?;
        let (head, _status) = bucket
            .head_object(&path)
            .await
            .map_err(|e| s3_error(format!("HEAD {}: {}", path, e)))?;

        let result = JSObject::new(&ctx);
        if let Some(etag) = head.e_tag {
            result.set("etag", etag)?;
        }
        if let Some(last_modified) = head.last_modified {
            result.set("lastModified", last_modified)?;
        }
        if let Some(ct) = head.content_type {
            result.set("type", ct)?;
        }
        result.set("size", head.content_length.unwrap_or(0) as f64)?;
        Ok(result)
    }

    #[js_method(ts_args = "path: string, options?: S3PresignOptions & S3ClientOptions")]
    async fn presign(&self, path: String, options: Optional<JSObject>) -> JSResult<String> {
        self.reject_config_override(&options, &["expiresIn", "method"])?;
        let path = self.prefixed_path(&path);
        let config = if let Some(ref obj) = options.0 {
            self.config.merge_js_options(obj)?
        } else {
            (*self.config).clone()
        };
        let bucket = config.create_bucket()?;

        let expires_in = options
            .0
            .as_ref()
            .and_then(|o| o.get::<_, f64>("expiresIn").ok())
            .map(|v| v as u32)
            .unwrap_or(86400);

        let method = options
            .0
            .as_ref()
            .and_then(|o| o.get::<_, String>("method").ok())
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

    #[js_method(
        ts_return = "S3ListResult",
        ts_args = "options?: S3ListOptions & S3ClientOptions"
    )]
    async fn list(&self, ctx: JSContext, options: Optional<JSObject>) -> JSResult<JSObject> {
        self.reject_config_override(&options, &["prefix", "maxKeys", "startAfter"])?;
        let config = if let Some(ref obj) = options.0 {
            self.config.merge_js_options(obj)?
        } else {
            (*self.config).clone()
        };
        let bucket = config.create_bucket()?;

        let user_prefix = options
            .0
            .as_ref()
            .and_then(|o| o.get::<_, String>("prefix").ok())
            .unwrap_or_default();

        // Combine namespace prefix with user-provided prefix
        let prefix = self.prefixed_path(&user_prefix);

        let max_keys = options
            .0
            .as_ref()
            .and_then(|o| o.get::<_, f64>("maxKeys").ok())
            .map(|v| v as usize);

        let start_after = options
            .0
            .as_ref()
            .and_then(|o| o.get::<_, String>("startAfter").ok())
            .map(|s| self.prefixed_path(&s));

        let results = bucket
            .list(prefix, None)
            .await
            .map_err(|e| s3_error(format!("LIST: {}", e)))?;

        let result_obj = JSObject::new(&ctx);
        let contents = JSArray::new(&ctx)?;
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

                let entry = JSObject::new(&ctx);
                entry.set("key", key)?;
                entry.set("size", obj.size as f64)?;
                entry.set("lastModified", obj.last_modified.as_str())?;
                if let Some(ref etag) = obj.e_tag {
                    entry.set("etag", etag.as_str())?;
                }
                contents.push(JSValue::from_rust(&ctx, entry))?;
                total_count += 1;
            }
        }

        result_obj.set("contents", contents)?;
        result_obj.set("isTruncated", is_truncated)?;
        Ok(result_obj)
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}
