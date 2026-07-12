use crate::config::S3Config;
use crate::namespace::S3Namespace;
use crate::types::{S3PresignOptions, S3StatResult, S3WriteOptions};
use rong::function::*;
use rong::*;
use std::rc::Rc;

fn s3_error(msg: impl Into<String>) -> RongJSError {
    HostError::new("ERR_S3", msg).with_name("S3Error").into()
}

fn type_error(msg: impl Into<String>) -> RongJSError {
    HostError::new(rong::error::E_TYPE, msg)
        .with_name("TypeError")
        .into()
}

/// Lazy reference to an S3 object. No network request on creation.
#[js_class]
pub struct S3File {
    config: Rc<S3Config>,
    namespace: S3Namespace,
    path: String,
    display_name: String,
    range_start: Option<u64>,
    range_end: Option<u64>,
}

/// Lazy reference to an S3 object. No network request on creation.
#[js_class]
impl S3File {
    #[js_method(constructor, private)]
    fn new() -> JSResult<Self> {
        rong::illegal_constructor(
            "S3File cannot be constructed directly. Use S3Client.file() instead.",
        )
    }

    pub(crate) fn create(config: Rc<S3Config>, namespace: S3Namespace, path: String) -> Self {
        Self {
            config,
            namespace,
            display_name: path.clone(),
            path,
            range_start: None,
            range_end: None,
        }
    }

    fn resolved_key(&self, ctx: &JSContext) -> JSResult<String> {
        let namespace = self.namespace.resolve(ctx)?;
        Ok(S3Namespace::apply(namespace.as_deref(), &self.path))
    }

    /// Object key as exposed by this reference.
    #[js_method(getter)]
    fn name(&self) -> String {
        self.display_name.clone()
    }

    /// S3 objects don't have synchronous size — use stat() instead.
    #[js_method(getter)]
    fn size(&self) -> f64 {
        f64::NAN
    }

    /// Download the object as UTF-8 text.
    #[js_method]
    async fn text(&self, ctx: JSContext) -> JSResult<String> {
        let key = self.resolved_key(&ctx)?;
        let bucket = self.config.create_bucket()?;
        let response = bucket
            .get_object(&key)
            .await
            .map_err(|e| s3_error(format!("GET {}: {}", key, e)))?;

        let bytes = self.apply_range(response.bytes());
        String::from_utf8(bytes.to_vec()).map_err(|e| s3_error(format!("invalid UTF-8: {}", e)))
    }

    /// Download and parse the object as JSON.
    #[js_method]
    async fn json(&self, ctx: JSContext) -> JSResult<JSValue> {
        let text = Self::text(self, ctx.clone()).await?;
        let obj = JSObject::from_json_string(&ctx, &text)?;
        Ok(JSValue::from_rust(&ctx, obj))
    }

    /// Download the object as an ArrayBuffer.
    #[js_method(ts_return = "ArrayBuffer")]
    async fn bytes(&self, ctx: JSContext) -> JSResult<JSValue> {
        let key = self.resolved_key(&ctx)?;
        let bucket = self.config.create_bucket()?;
        let response = bucket
            .get_object(&key)
            .await
            .map_err(|e| s3_error(format!("GET {}: {}", key, e)))?;

        let data = self.apply_range(response.bytes());
        let ab = JSArrayBuffer::from_bytes(&ctx, data)
            .map_err(|e| s3_error(format!("ArrayBuffer: {}", e)))?;
        Ok(JSValue::from_rust(&ctx, ab))
    }

    /// Download the object as an ArrayBuffer (alias of `bytes`).
    #[js_method(rename = "arrayBuffer", ts_return = "ArrayBuffer")]
    async fn array_buffer(&self, ctx: JSContext) -> JSResult<JSValue> {
        Self::bytes(self, ctx).await
    }

    /// Write data to this S3 object.
    #[js_method(ts_params = "data: string | ArrayBuffer | Uint8Array, options?: S3WriteOptions")]
    async fn write(
        &self,
        ctx: JSContext,
        data: JSValue,
        options: Optional<S3WriteOptions>,
    ) -> JSResult<f64> {
        let key = self.resolved_key(&ctx)?;
        let bucket = self.config.create_bucket()?;
        let (content_bytes, content_type) = resolve_body(&data)?;
        let ct = if let Some(opts) = options.0 {
            opts.content_type.or(content_type)
        } else {
            content_type
        };
        let ct_str = ct.as_deref().unwrap_or("application/octet-stream");

        bucket
            .put_object_with_content_type(&key, &content_bytes, ct_str)
            .await
            .map_err(|e| s3_error(format!("PUT {}: {}", key, e)))?;

        Ok(content_bytes.len() as f64)
    }

    /// Delete this object.
    #[js_method]
    async fn delete(&self, ctx: JSContext) -> JSResult<()> {
        let key = self.resolved_key(&ctx)?;
        let bucket = self.config.create_bucket()?;
        bucket
            .delete_object(&key)
            .await
            .map_err(|e| s3_error(format!("DELETE {}: {}", key, e)))?;
        Ok(())
    }

    /// Delete this object (alias of `delete`).
    #[js_method]
    async fn unlink(&self, ctx: JSContext) -> JSResult<()> {
        Self::delete(self, ctx).await
    }

    /// Test whether this object exists.
    #[js_method]
    async fn exists(&self, ctx: JSContext) -> JSResult<bool> {
        let key = self.resolved_key(&ctx)?;
        let bucket = self.config.create_bucket()?;
        match bucket.head_object(&key).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Read object metadata without downloading its body.
    #[js_method]
    async fn stat(&self, ctx: JSContext) -> JSResult<S3StatResult> {
        let key = self.resolved_key(&ctx)?;
        let bucket = self.config.create_bucket()?;
        let (head, _status) = bucket
            .head_object(&key)
            .await
            .map_err(|e| s3_error(format!("HEAD {}: {}", key, e)))?;

        Ok(S3StatResult {
            etag: head.e_tag,
            last_modified: head.last_modified,
            size: head.content_length.unwrap_or(0) as f64,
            content_type: head.content_type,
        })
    }

    /// Generate a presigned URL (async in rust-s3).
    #[js_method]
    async fn presign(
        &self,
        ctx: JSContext,
        options: Optional<S3PresignOptions>,
    ) -> JSResult<String> {
        let key = self.resolved_key(&ctx)?;
        let bucket = self.config.create_bucket()?;
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
                .presign_get(&key, expires_in, None)
                .await
                .map_err(|e| s3_error(format!("presign GET: {}", e))),
            "PUT" => bucket
                .presign_put(&key, expires_in, None, None)
                .await
                .map_err(|e| s3_error(format!("presign PUT: {}", e))),
            "DELETE" => bucket
                .presign_delete(&key, expires_in)
                .await
                .map_err(|e| s3_error(format!("presign DELETE: {}", e))),
            other => Err(HostError::new(
                "ERR_S3_INVALID_METHOD",
                format!("Unsupported presign method: {}", other),
            )
            .into()),
        }
    }

    /// Create a lazy reference to a byte range of this object.
    #[js_method(ts_return = "S3File")]
    fn slice(&self, ctx: JSContext, start: f64, end: Optional<f64>) -> JSResult<JSObject> {
        let file = S3File {
            config: self.config.clone(),
            namespace: self.namespace.clone(),
            path: self.path.clone(),
            display_name: self.display_name.clone(),
            range_start: Some(start as u64),
            range_end: end.0.map(|v| v as u64),
        };
        let obj = Class::lookup::<S3File>(&ctx)?.instance(file);
        Ok(obj)
    }

    fn apply_range<'a>(&self, data: &'a [u8]) -> &'a [u8] {
        let start = self.range_start.unwrap_or(0) as usize;
        let end = self
            .range_end
            .map(|e| (e as usize).min(data.len()))
            .unwrap_or(data.len());
        if start >= data.len() {
            return &[];
        }
        &data[start..end]
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

/// Resolve a JS value to bytes + optional content type.
pub(crate) fn resolve_body(data: &JSValue) -> JSResult<(Vec<u8>, Option<String>)> {
    if data.is_string() {
        let s: String = data
            .clone()
            .to_rust()
            .map_err(|_| type_error("invalid string"))?;
        return Ok((s.into_bytes(), Some("text/plain;charset=utf-8".to_string())));
    }
    if data.is_array_buffer() {
        let ab: JSArrayBuffer = data
            .clone()
            .to_rust()
            .map_err(|_| type_error("invalid ArrayBuffer"))?;
        return Ok((ab.as_bytes().to_vec(), None));
    }
    if let Some(obj) = data.clone().into_object()
        && let Some(ta) = AnyJSTypedArray::from_object(obj)
        && let Some(bytes) = ta.as_bytes()
    {
        return Ok((bytes.to_vec(), None));
    }
    Err(type_error(
        "data must be a string, ArrayBuffer, or Uint8Array",
    ))
}
