use crate::file::{FileHandle, FileOpenOption, open_file_internal};
use crate::grant_file_access;
use crate::sink::{FileSink, FileSinkOptions};
use crate::stat::FileInfo;
use bytes::{Bytes, BytesMut};
use rong::{function::Optional, *};
use rong_stream::JSReadableStream;
use std::path::PathBuf;
use tokio::sync::mpsc;

#[js_export]
pub(crate) struct RongFile {
    path: String,
    resolved: PathBuf,
}

impl RongFile {
    pub(crate) fn resolved(&self) -> &PathBuf {
        &self.resolved
    }
}

#[js_class]
impl RongFile {
    #[js_method(constructor)]
    fn new() -> JSResult<Self> {
        rong::illegal_constructor("Not Allowed 'new RongFile()', use Rong.file(path)")
    }

    #[js_method(getter)]
    fn name(&self) -> String {
        self.path.clone()
    }

    #[js_method]
    async fn text(&self) -> JSResult<String> {
        tokio::fs::read_to_string(&self.resolved)
            .await
            .map_err(|e| HostError::new("FS_IO", e.to_string()).into())
    }

    #[js_method]
    async fn json(&self, ctx: JSContext) -> JSResult<JSValue> {
        let text = tokio::fs::read_to_string(&self.resolved)
            .await
            .map_err(|e| HostError::new("FS_IO", e.to_string()))?;

        text.as_str().json_to_js_value(&ctx)
    }

    #[js_method]
    async fn bytes(&self, ctx: JSContext) -> JSResult<JSTypedArray> {
        let data = tokio::fs::read(&self.resolved)
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Failed to read file: {}", e)))?;

        let len = data.len();
        let ab = JSArrayBuffer::from_bytes_owned(&ctx, data)?;
        JSTypedArray::from_array_buffer(&ctx, ab, 0, Some(len))
    }

    #[js_method(rename = "arrayBuffer")]
    async fn array_buffer(&self, ctx: JSContext) -> JSResult<JSArrayBuffer> {
        let data = tokio::fs::read(&self.resolved)
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Failed to read file: {}", e)))?;

        JSArrayBuffer::from_bytes_owned(&ctx, data)
    }

    #[js_method(ts_return = "ReadableStream<Uint8Array>")]
    fn stream(&self, ctx: JSContext) -> Option<JSObject> {
        let resolved = self.resolved.clone();
        let (tx, rx) = mpsc::channel::<Result<Bytes, String>>(16);
        let chunk_size = 64 * 1024;

        tokio::task::spawn(async move {
            let file = match tokio::fs::File::open(&resolved).await {
                Ok(f) => f,
                Err(e) => {
                    let _ = tx.send(Err(e.to_string())).await;
                    return;
                }
            };
            let mut reader = tokio::io::BufReader::new(file);
            let mut buf = BytesMut::with_capacity(chunk_size);
            loop {
                buf.clear();
                match tokio::io::AsyncReadExt::read_buf(&mut reader, &mut buf).await {
                    Ok(0) => break,
                    Ok(_) => {
                        if tx.send(Ok(buf.split().freeze())).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string())).await;
                        break;
                    }
                }
            }
        });

        JSReadableStream::from_receiver(&ctx, rx)
            .map(|jsrs| jsrs.into_object())
            .ok()
    }

    #[js_method]
    async fn exists(&self) -> bool {
        tokio::fs::metadata(&self.resolved).await.is_ok()
    }

    #[js_method]
    async fn delete(&self) -> JSResult<()> {
        tokio::fs::remove_file(&self.resolved)
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Failed to delete file: {}", e)).into())
    }

    #[js_method]
    async fn stat(&self) -> JSResult<FileInfo> {
        tokio::fs::metadata(&self.resolved)
            .await
            .map(FileInfo::from_metadata)
            .map_err(|e| HostError::new("FS_IO", format!("Failed to get file info: {}", e)).into())
    }

    #[js_method]
    async fn lstat(&self) -> JSResult<FileInfo> {
        tokio::fs::symlink_metadata(&self.resolved)
            .await
            .map(FileInfo::from_metadata)
            .map_err(|e| HostError::new("FS_IO", format!("Failed to get file info: {}", e)).into())
    }

    #[js_method]
    async fn open(&self, option: Optional<FileOpenOption>) -> JSResult<FileHandle> {
        open_file_internal(&self.resolved, &self.path, option.0).await
    }

    #[js_method]
    async fn writer(&self, option: Optional<FileSinkOptions>) -> JSResult<FileSink> {
        FileSink::create(&self.resolved, &self.path, option.0).await
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

fn file(path: String) -> JSResult<RongFile> {
    let resolved = grant_file_access(&path)?;
    Ok(RongFile { path, resolved })
}

pub(crate) fn init(ctx: &JSContext) -> JSResult<()> {
    let rong = ctx.host_namespace();

    ctx.register_hidden_class::<RongFile>()?;

    let file_fn = JSFunc::new(ctx, file)?.name("file")?;
    rong.set("file", file_fn)?;

    Ok(())
}
