use crate::config::S3Config;
use crate::file::{S3File, resolve_body};
use crate::namespace::S3Namespace;
use crate::types::{
    S3ClientListOptions, S3ClientOptions, S3ClientPresignOptions, S3ClientWriteOptions,
    S3ListEntry, S3ListResult, S3StatResult,
};
use rong::function::*;
use rong::*;
use std::borrow::Cow;
use std::rc::Rc;

fn s3_error(msg: impl Into<String>) -> RongJSError {
    HostError::new("ERR_S3", msg).with_name("S3Error").into()
}

/// S3-compatible object storage client.
#[js_class]
pub struct S3Client {
    pub(crate) config: Rc<S3Config>,
    namespace: S3Namespace,
    config_locked: bool,
}

impl S3Client {
    /// Create an unnamespaced host-configured client with locked configuration.
    pub fn new(config: S3Config) -> Self {
        Self {
            config: config.into_rc(),
            namespace: S3Namespace::None,
            config_locked: true,
        }
    }

    /// Apply a fixed object-key namespace to this host-configured client.
    pub fn with_namespace(mut self, prefix: impl Into<String>) -> JSResult<Self> {
        self.namespace = S3Namespace::fixed(prefix)?;
        Ok(self)
    }

    /// Resolve this client's object-key namespace for each network-capable operation.
    /// Lazy `S3File` references retain the resolver and resolve it when used.
    pub fn with_namespace_resolver<F>(mut self, resolver: F) -> Self
    where
        F: Fn(&JSContext) -> JSResult<String> + 'static,
    {
        self.namespace = S3Namespace::dynamic(resolver);
        self
    }

    /// Convert this host-configured client into an injectable JavaScript object.
    /// Required classes are registered without exposing their constructors.
    pub fn into_js_object(self, ctx: &JSContext) -> JSResult<JSObject> {
        crate::register_s3_classes(ctx)?;
        Ok(Class::lookup::<Self>(ctx)?.instance(self))
    }

    fn resolved_namespace<'a>(&'a self, ctx: &JSContext) -> JSResult<Option<Cow<'a, str>>> {
        self.namespace.resolve(ctx)
    }

    fn reject_config_override(
        &self,
        options: &Optional<JSObject>,
        allowed_non_config_keys: &[&str],
    ) -> JSResult<()> {
        if !self.config_locked {
            return Ok(());
        }

        let Some(obj) = options.0.as_ref() else {
            return Ok(());
        };

        for key in [
            "accessKeyId",
            "secretAccessKey",
            "sessionToken",
            "region",
            "endpoint",
            "bucket",
            "acl",
            "virtualHostedStyle",
        ] {
            if obj.has_property(key)? {
                return Err(HostError::new(
                    "E_INVALID_ARG",
                    format!("Cannot override S3 config field '{key}' on a host-injected S3Client"),
                )
                .with_name("TypeError")
                .into());
            }
        }

        for key in obj.keys_as::<String>()? {
            if !allowed_non_config_keys.contains(&key.as_str()) {
                return Err(HostError::new(
                    "E_INVALID_ARG",
                    format!("Option '{key}' is not allowed on a host-injected S3Client"),
                )
                .with_name("TypeError")
                .into());
            }
        }

        Ok(())
    }
}

fn typed_options<T>(options: &Optional<JSObject>) -> JSResult<Option<T>>
where
    T: FromJSValue<JSEngineValue>,
{
    options
        .0
        .as_ref()
        .map(|object| {
            let ctx = object.context();
            T::from_js_value(&ctx, object.clone().into_js_value())
        })
        .transpose()
}

/// S3-compatible object-storage client.
#[js_class]
impl S3Client {
    /// Construct a client from explicit options or the process environment.
    #[js_method(constructor)]
    fn js_new(options: Optional<S3ClientOptions>) -> JSResult<Self> {
        let config = S3Config::from_options(options.0);
        Ok(Self {
            config: config.into_rc(),
            namespace: S3Namespace::None,
            config_locked: false,
        })
    }

    /// Lazy file reference — no network request.
    #[js_method(
        ts_return = "S3File",
        ts_params = "path: string, options?: S3ClientOptions"
    )]
    fn file(
        &self,
        ctx: JSContext,
        path: String,
        options: Optional<JSObject>,
    ) -> JSResult<JSObject> {
        self.reject_config_override(&options, &[])?;
        let options = typed_options::<S3ClientOptions>(&options)?;
        let config = if let Some(ref options) = options {
            self.config.merge_options(Some(options)).into_rc()
        } else {
            self.config.clone()
        };
        let file = S3File::create(config, self.namespace.clone(), path);
        Ok(Class::lookup::<S3File>(&ctx)?.instance(file))
    }

    /// Write an object and return the number of bytes uploaded.
    #[js_method(
        ts_params = "path: string, data: string | ArrayBuffer | Uint8Array, options?: S3WriteOptions & S3ClientOptions"
    )]
    async fn write(
        &self,
        ctx: JSContext,
        path: String,
        data: JSValue,
        options: Optional<JSObject>,
    ) -> JSResult<f64> {
        self.reject_config_override(&options, &["type"])?;
        let options = typed_options::<S3ClientWriteOptions>(&options)?;
        let namespace = self.resolved_namespace(&ctx)?;
        let path = S3Namespace::apply(namespace.as_deref(), &path);
        let config = if let Some(ref options) = options {
            self.config.merge_options(Some(options))
        } else {
            (*self.config).clone()
        };
        let bucket = config.create_bucket()?;
        let (content_bytes, content_type) = resolve_body(&data)?;
        let ct = if let Some(ref options) = options {
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

    /// Delete an object.
    #[js_method]
    async fn delete(&self, ctx: JSContext, path: String) -> JSResult<()> {
        let namespace = self.resolved_namespace(&ctx)?;
        let path = S3Namespace::apply(namespace.as_deref(), &path);
        let bucket = self.config.create_bucket()?;
        bucket
            .delete_object(&path)
            .await
            .map_err(|e| s3_error(format!("DELETE {}: {}", path, e)))?;
        Ok(())
    }

    /// Delete an object (alias of `delete`).
    #[js_method]
    async fn unlink(&self, ctx: JSContext, path: String) -> JSResult<()> {
        Self::delete(self, ctx, path).await
    }

    /// Test whether an object exists.
    #[js_method]
    async fn exists(&self, ctx: JSContext, path: String) -> JSResult<bool> {
        let namespace = self.resolved_namespace(&ctx)?;
        let path = S3Namespace::apply(namespace.as_deref(), &path);
        let bucket = self.config.create_bucket()?;
        match bucket.head_object(&path).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Return an object's size in bytes.
    #[js_method]
    async fn size(&self, ctx: JSContext, path: String) -> JSResult<f64> {
        let namespace = self.resolved_namespace(&ctx)?;
        let path = S3Namespace::apply(namespace.as_deref(), &path);
        let bucket = self.config.create_bucket()?;
        let (head, _status) = bucket
            .head_object(&path)
            .await
            .map_err(|e| s3_error(format!("HEAD {}: {}", path, e)))?;
        Ok(head.content_length.unwrap_or(0) as f64)
    }

    /// Read object metadata without downloading its body.
    #[js_method]
    async fn stat(&self, ctx: JSContext, path: String) -> JSResult<S3StatResult> {
        let namespace = self.resolved_namespace(&ctx)?;
        let path = S3Namespace::apply(namespace.as_deref(), &path);
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

    /// Generate a presigned object URL.
    #[js_method(ts_params = "path: string, options?: S3PresignOptions & S3ClientOptions")]
    async fn presign(
        &self,
        ctx: JSContext,
        path: String,
        options: Optional<JSObject>,
    ) -> JSResult<String> {
        self.reject_config_override(&options, &["expiresIn", "method"])?;
        let options = typed_options::<S3ClientPresignOptions>(&options)?;
        let namespace = self.resolved_namespace(&ctx)?;
        let path = S3Namespace::apply(namespace.as_deref(), &path);
        let config = if let Some(ref options) = options {
            self.config.merge_options(Some(options))
        } else {
            (*self.config).clone()
        };
        let bucket = config.create_bucket()?;

        let expires_in = options
            .as_ref()
            .and_then(|o| o.expires_in)
            .map(|v| v as u32)
            .unwrap_or(86400);

        let method = options
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

    /// List objects in the configured bucket.
    #[js_method(ts_params = "options?: S3ListOptions & S3ClientOptions")]
    async fn list(&self, ctx: JSContext, options: Optional<JSObject>) -> JSResult<S3ListResult> {
        self.reject_config_override(&options, &["prefix", "maxKeys", "startAfter"])?;
        let options = typed_options::<S3ClientListOptions>(&options)?;
        let namespace = self.resolved_namespace(&ctx)?;
        let config = if let Some(ref options) = options {
            self.config.merge_options(Some(options))
        } else {
            (*self.config).clone()
        };
        let bucket = config.create_bucket()?;

        let user_prefix = options
            .as_ref()
            .and_then(|o| o.prefix.clone())
            .unwrap_or_default();

        let prefix = S3Namespace::apply(namespace.as_deref(), &user_prefix);

        let max_keys = options
            .as_ref()
            .and_then(|o| o.max_keys)
            .map(|v| v as usize);

        let start_after = options
            .as_ref()
            .and_then(|o| o.start_after.clone())
            .map(|s| S3Namespace::apply(namespace.as_deref(), &s));

        let results = bucket
            .list(prefix, None)
            .await
            .map_err(|e| s3_error(format!("LIST: {}", e)))?;

        let mut contents = Vec::new();
        let mut total_count = 0usize;
        let mut is_truncated = false;

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

                // Treat an out-of-namespace response as a server isolation
                // violation instead of exposing the raw key to JavaScript.
                let key = S3Namespace::strip(namespace.as_deref(), &obj.key)?;

                contents.push(S3ListEntry {
                    key,
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
