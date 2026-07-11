use super::blob::Blob;
use bytes::Bytes;
use rong::{function::Optional, *};
use std::time::{SystemTime, UNIX_EPOCH};

#[js_class(clone)]
pub struct File {
    blob: Blob,
    filename: String,
    last_modified: i64,
}

#[js_class]
impl File {
    fn now_ms() -> JSResult<i64> {
        let duration = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| {
            RongJSError::from(HostError::new(
                rong::error::E_INTERNAL,
                "Failed to get current time",
            ))
        })?;
        Ok(duration.as_millis() as i64)
    }

    #[js_method(constructor)]
    fn new(data: JSArray, filename: String, options: Optional<JSObject>) -> JSResult<Self> {
        // Validate filename
        if filename.is_empty() {
            return Err(
                HostError::new(rong::error::E_INVALID_ARG, "File name cannot be empty")
                    .with_name("TypeError")
                    .into(),
            );
        }

        // Get current time as default last_modified
        let default_time = Self::now_ms()?;

        // Extract options
        let (last_modified, blob_options) = if let Some(ref opts) = options.0 {
            let time = match opts.get::<_, f64>("lastModified") {
                Ok(t) => t as i64,
                Err(_) => default_time,
            };
            (time, options)
        } else {
            (default_time, Optional(None))
        };

        let blob = Blob::new(Optional(Some(data)), blob_options)?;

        Ok(Self {
            blob,
            filename,
            last_modified,
        })
    }

    pub fn from_parts(
        mime_type: String,
        data: impl Into<Bytes>,
        filename: String,
        last_modified: Option<i64>,
    ) -> JSResult<Self> {
        if filename.is_empty() {
            return Err(
                HostError::new(rong::error::E_INVALID_ARG, "File name cannot be empty")
                    .with_name("TypeError")
                    .into(),
            );
        }

        let default_time = Self::now_ms()?;
        Ok(Self {
            blob: Blob::from_parts(mime_type, data),
            filename,
            last_modified: last_modified.unwrap_or(default_time),
        })
    }

    pub fn bytes_ref(&self) -> &Bytes {
        self.blob.bytes_ref()
    }

    #[js_method(getter)]
    fn size(&self) -> usize {
        self.blob.size()
    }

    #[js_method(getter)]
    pub fn name(&self) -> String {
        self.filename.clone()
    }

    #[js_method(getter, rename = "type")]
    pub fn mime_type(&self) -> String {
        self.blob.mime_type()
    }

    #[js_method(getter, rename = "lastModified")]
    fn last_modified(&self) -> f64 {
        self.last_modified as f64
    }

    #[js_method]
    fn slice(
        &self,
        start: Optional<i64>,
        end: Optional<i64>,
        content_type: Optional<String>,
    ) -> JSResult<Blob> {
        self.blob.slice(start, end, content_type)
    }

    #[js_method]
    async fn text(&self) -> JSResult<String> {
        self.blob.text().await
    }

    #[js_method(rename = "arrayBuffer")]
    async fn array_buffer(&self, ctx: JSContext) -> JSResult<JSArrayBuffer> {
        self.blob.array_buffer(ctx).await
    }

    #[js_method]
    async fn bytes(&self, ctx: JSContext) -> JSResult<JSTypedArray> {
        self.blob.bytes(ctx).await
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}
