use crate::stat::FileInfo;
use bytes::{Bytes, BytesMut};
use rong::{function::Optional, *};
use rong_stream::{JSReadableStream, WritableStream};
use std::path::Path;
use std::sync::Arc;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};
use tokio::sync::Mutex;
use tokio::sync::mpsc;

/// Seek origin constants exposed as `Rong.SeekMode`.
#[js_const_enum]
enum SeekMode {
    /// Seek from start of file (absolute position).
    Start = 0,
    /// Seek from current position (relative).
    Current = 1,
    /// Seek from end of file (usually a negative offset).
    End = 2,
}

#[derive(FromJSObj)]
pub(crate) struct FileOpenOption {
    pub(crate) read: Option<bool>,
    pub(crate) write: Option<bool>,
    pub(crate) append: Option<bool>,
    pub(crate) truncate: Option<bool>,
    pub(crate) create: Option<bool>,
    #[rename = "createNew"]
    pub(crate) create_new: Option<bool>,
    pub(crate) mode: Option<u32>,
}

#[js_export]
pub(crate) struct FileHandle {
    file: Arc<Mutex<Option<File>>>,
}

#[js_class]
impl FileHandle {
    #[js_method(constructor)]
    fn new() -> JSResult<Self> {
        rong::illegal_constructor("Not Allowed 'new FileHandle()', use Rong.file(path).open()")
    }

    #[js_method]
    async fn stat(&self) -> JSResult<FileInfo> {
        let file = self.file.lock().await;
        file.as_ref()
            .ok_or_else(|| HostError::new(rong::error::E_INVALID_STATE, "FileHandle is closed"))?
            .metadata()
            .await
            .map(FileInfo::from_metadata)
            .map_err(|e| HostError::new("FS_IO", format!("Failed to get file stats: {}", e)).into())
    }

    #[js_method]
    async fn read(&self, buffer: JSArrayBuffer) -> JSResult<Option<usize>> {
        let buf_len = buffer.len();
        if buf_len == 0 {
            return Ok(Some(0));
        }

        let mut buffer = buffer;
        let buffer_slice = buffer.as_mut_slice();

        let mut file = self.file.lock().await;
        let file = file
            .as_mut()
            .ok_or_else(|| HostError::new(rong::error::E_INVALID_STATE, "FileHandle is closed"))?;
        match file.read(buffer_slice).await {
            Ok(0) => Ok(None),
            Ok(bytes_read) => Ok(Some(bytes_read)),
            Err(e) => Err(HostError::new("FS_IO", format!("Failed to read file: {}", e)).into()),
        }
    }

    #[js_method]
    async fn write(&self, buffer: JSArrayBuffer) -> JSResult<usize> {
        let buf = buffer.as_slice();

        let mut file = self.file.lock().await;
        let file = file
            .as_mut()
            .ok_or_else(|| HostError::new(rong::error::E_INVALID_STATE, "FileHandle is closed"))?;
        file.write_all(buf)
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Failed to write file: {}", e)))?;

        Ok(buf.len())
    }

    #[js_method]
    async fn sync(&self) -> JSResult<()> {
        let file = self.file.lock().await;
        file.as_ref()
            .ok_or_else(|| HostError::new(rong::error::E_INVALID_STATE, "FileHandle is closed"))?
            .sync_all()
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Failed to sync file: {}", e)).into())
    }

    #[js_method]
    async fn truncate(&self, len: Optional<u64>) -> JSResult<()> {
        let length = len.0.unwrap_or(0);
        let file = self.file.lock().await;
        file.as_ref()
            .ok_or_else(|| HostError::new(rong::error::E_INVALID_STATE, "FileHandle is closed"))?
            .set_len(length)
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Failed to truncate file: {}", e)).into())
    }

    #[js_method]
    async fn seek(&self, offset: i64, whence: Optional<SeekMode>) -> JSResult<u64> {
        let seek_from = match whence.0.unwrap_or(SeekMode::Start) {
            SeekMode::Start => SeekFrom::Start(offset as u64),
            SeekMode::Current => SeekFrom::Current(offset),
            SeekMode::End => SeekFrom::End(offset),
        };

        let mut file = self.file.lock().await;
        let file = file
            .as_mut()
            .ok_or_else(|| HostError::new(rong::error::E_INVALID_STATE, "FileHandle is closed"))?;
        let new_position = file
            .seek(seek_from)
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Failed to seek: {}", e)))?;

        Ok(new_position)
    }

    #[js_method]
    async fn close(&self) -> JSResult<()> {
        let mut file = self.file.lock().await;
        file.take();
        Ok(())
    }

    #[js_method(getter, ts_return = "ReadableStream<Uint8Array>")]
    fn readable(&self, ctx: JSContext) -> Option<JSObject> {
        let file = self.file.clone();
        let (tx, rx) = mpsc::channel::<Result<Bytes, String>>(16);
        let chunk_size = 64 * 1024;
        tokio::task::spawn(async move {
            let mut buf = BytesMut::with_capacity(chunk_size);
            let mut guard = file.lock().await;
            let Some(f) = guard.as_mut() else {
                let _ = tx.send(Err("FileHandle is closed".to_string())).await;
                return;
            };
            loop {
                buf.clear();
                match tokio::io::AsyncReadExt::read_buf(f, &mut buf).await {
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

    #[js_method(getter, ts_return = "WritableStream<Uint8Array>")]
    fn writable(&self) -> JSResult<WritableStream> {
        let file = self.file.clone();
        let (tx, mut rx) = mpsc::channel::<Bytes>(128);
        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
        tokio::task::spawn(async move {
            let mut guard = file.lock().await;
            let Some(f) = guard.as_mut() else {
                let _ = done_tx.send(Err("FileHandle is closed".to_string()));
                return;
            };
            let mut error: Option<String> = None;
            while let Some(chunk) = rx.recv().await {
                if let Err(e) = f.write_all(&chunk).await {
                    error = Some(e.to_string());
                    break;
                }
            }
            if let Err(e) = f.flush().await
                && error.is_none()
            {
                error = Some(e.to_string());
            }
            let _ = done_tx.send(match error {
                Some(e) => Err(e),
                None => Ok(()),
            });
        });
        Ok(rong_stream::writable_stream_to_sender_with_done(
            tx, done_rx,
        ))
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

/// Internal helper used by RongFile.open()
pub(crate) async fn open_file_internal(
    resolved: &Path,
    display_path: &str,
    option: Option<FileOpenOption>,
) -> JSResult<FileHandle> {
    let opts = option.unwrap_or(FileOpenOption {
        read: None,
        write: None,
        append: None,
        truncate: None,
        create: None,
        create_new: None,
        mode: None,
    });

    let read = opts.read.unwrap_or(true);
    let write = opts.write.unwrap_or(false);
    let append = opts.append.unwrap_or(false);
    let truncate = opts.truncate.unwrap_or(false);
    let create = opts.create.unwrap_or(false);
    let create_new = opts.create_new.unwrap_or(false);
    let mode = opts.mode;

    let file_handle = if cfg!(unix) && mode.is_some() {
        #[cfg(unix)]
        {
            let resolved = resolved.to_path_buf();
            let display_path = display_path.to_string();
            let mode = mode.unwrap_or(0);
            let handle = tokio::task::spawn_blocking(move || {
                let mut open_options = std::fs::OpenOptions::new();
                open_options
                    .read(read)
                    .write(write)
                    .append(append)
                    .truncate(truncate)
                    .create(create)
                    .create_new(create_new);
                std::os::unix::fs::OpenOptionsExt::mode(&mut open_options, mode);
                open_options.open(&resolved)
            })
            .await
            .map_err(|e| {
                HostError::new(
                    "FS_IO",
                    format!("Failed to open file '{}': {}", display_path, e),
                )
            })?
            .map_err(|e| {
                HostError::new(
                    "FS_IO",
                    format!("Failed to open file '{}': {}", display_path, e),
                )
            })?;
            File::from_std(handle)
        }
        #[cfg(not(unix))]
        {
            unreachable!("mode should be ignored on non-unix platforms");
        }
    } else {
        let mut open_options = OpenOptions::new();
        open_options
            .read(read)
            .write(write)
            .append(append)
            .truncate(truncate)
            .create(create)
            .create_new(create_new);

        open_options.open(resolved).await.map_err(|e| {
            HostError::new(
                "FS_IO",
                format!("Failed to open file '{}': {}", display_path, e),
            )
        })?
    };

    Ok(FileHandle {
        file: Arc::new(Mutex::new(Some(file_handle))),
    })
}

pub(crate) fn init(ctx: &JSContext) -> JSResult<()> {
    let rong = ctx.host_namespace();

    ctx.register_hidden_class::<FileHandle>()?;

    rong.set("SeekMode", SeekMode::js_object(ctx)?)?;

    Ok(())
}
