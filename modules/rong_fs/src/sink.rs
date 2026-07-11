use rong::*;
use std::path::Path;
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

#[derive(FromJSObject, Default)]
pub(crate) struct FileSinkOptions {
    /// If true, open file in append mode. Default is truncate (overwrite).
    pub(crate) append: Option<bool>,
    /// Unix file permissions mode.
    #[cfg(unix)]
    pub(crate) mode: Option<u32>,
}

#[js_class]
pub(crate) struct FileSink {
    file: Arc<Mutex<Option<tokio::fs::File>>>,
}

impl FileSink {
    pub(crate) async fn create(
        resolved: &Path,
        display_path: &str,
        options: Option<FileSinkOptions>,
    ) -> JSResult<Self> {
        let opts = options.unwrap_or_default();
        let append = opts.append.unwrap_or(false);

        let mut open_opts = OpenOptions::new();
        open_opts.write(true).create(true);
        if append {
            open_opts.append(true);
        } else {
            open_opts.truncate(true);
        }

        #[cfg(unix)]
        if let Some(mode) = opts.mode {
            open_opts.mode(mode);
        }

        let file = open_opts.open(resolved).await.map_err(|e| {
            HostError::new(
                "FS_IO",
                format!("Failed to open file '{}': {}", display_path, e),
            )
        })?;

        Ok(FileSink {
            file: Arc::new(Mutex::new(Some(file))),
        })
    }
}

/// Incremental writer returned by `RongFile.writer`.
#[js_class]
impl FileSink {
    #[js_method(constructor, private)]
    fn new() -> JSResult<Self> {
        rong::illegal_constructor("Not Allowed 'new FileSink()', use Rong.file(path).writer()")
    }

    /// Write data and return the number of bytes written.
    #[js_method(ts_params = "data: string | ArrayBufferView | ArrayBuffer")]
    async fn write(&self, data: JSValue) -> JSResult<f64> {
        // String
        if data.is_string() {
            let text: String = data.to_rust()?;
            let bytes = text.as_bytes();
            let len = bytes.len();
            let mut file = self.file.lock().await;
            let file = file.as_mut().ok_or_else(|| {
                HostError::new(rong::error::E_INVALID_STATE, "FileSink is closed")
            })?;
            file.write_all(bytes)
                .await
                .map_err(|e| HostError::new("FS_IO", format!("Write failed: {}", e)))?;
            return Ok(len as f64);
        }

        // ArrayBuffer
        if data.is_array_buffer() {
            let ab: JSArrayBuffer = data.to_rust()?;
            let bytes = ab.as_slice();
            let len = bytes.len();
            let mut file = self.file.lock().await;
            let file = file.as_mut().ok_or_else(|| {
                HostError::new(rong::error::E_INVALID_STATE, "FileSink is closed")
            })?;
            file.write_all(bytes)
                .await
                .map_err(|e| HostError::new("FS_IO", format!("Write failed: {}", e)))?;
            return Ok(len as f64);
        }

        // TypedArray (Uint8Array etc.)
        if let Some(obj) = data.into_object()
            && let Some(ta) = AnyJSTypedArray::from_object(obj)
        {
            let bytes = ta.as_bytes().ok_or_else(|| {
                HostError::new(rong::error::E_INVALID_ARG, "Invalid TypedArray data")
                    .with_name("TypeError")
            })?;
            let len = bytes.len();
            let mut file = self.file.lock().await;
            let file = file.as_mut().ok_or_else(|| {
                HostError::new(rong::error::E_INVALID_STATE, "FileSink is closed")
            })?;
            file.write_all(bytes)
                .await
                .map_err(|e| HostError::new("FS_IO", format!("Write failed: {}", e)))?;
            return Ok(len as f64);
        }

        Err(HostError::new(
            rong::error::E_INVALID_ARG,
            "data must be a string, ArrayBuffer, or TypedArray",
        )
        .with_name("TypeError")
        .into())
    }

    /// Flush buffered data to the file.
    #[js_method]
    async fn flush(&self) -> JSResult<()> {
        let mut file = self.file.lock().await;
        file.as_mut()
            .ok_or_else(|| HostError::new(rong::error::E_INVALID_STATE, "FileSink is closed"))?
            .flush()
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Flush failed: {}", e)).into())
    }

    /// Flush and close the writer.
    #[js_method]
    async fn end(&self) -> JSResult<()> {
        let mut file = self.file.lock().await;
        let Some(mut file) = file.take() else {
            return Ok(());
        };
        file.flush()
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Flush failed: {}", e)))?;
        file.sync_all()
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Sync failed: {}", e)).into())
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

pub(crate) fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_hidden_class::<FileSink>()?;
    Ok(())
}
