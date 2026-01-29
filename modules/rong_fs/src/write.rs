use crate::JSTypedArray;
use rong::{function::Optional, *};
use tokio::io::AsyncWriteExt;
use tokio::{fs, select};

use crate::grant_file_access;
use rong_abort::AbortSignal;

#[derive(FromJSObj, Default)]
struct WriteFileOptions {
    // If set to true, will append to a file instead of overwriting previous contents
    append: Option<bool>,

    // If set to true, no file, directory, or symlink is allowed to exist at the
    // target location. When createNew is set to true, create is ignored.
    #[rename = "createNew"]
    create_new: Option<bool>,

    // Permissions always applied to file.
    mode: Option<u32>,

    // An abort signal to allow cancellation of the file write operation.
    signal: Option<AbortSignal>,
}

async fn write_text_file(
    file: String,
    text: String,
    option: Optional<WriteFileOptions>,
) -> JSResult<()> {
    let resolved = grant_file_access(&file)?;
    let options = option.0.unwrap_or_default();

    // Handle createNew option
    if options.create_new.unwrap_or(false) && fs::metadata(&resolved).await.is_ok() {
        return Err(HostError::new(rong::error::E_ALREADY_EXISTS, "File already exists").into());
    }

    // Handle append option
    let mut open_options = fs::OpenOptions::new();
    open_options
        .write(true)
        .create(true)
        .truncate(!options.append.unwrap_or(false))
        .append(options.append.unwrap_or(false));

    // Handle mode option (Unix-like systems, including macOS)
    #[cfg(unix)]
    if let Some(mode) = options.mode {
        open_options.mode(mode);
    }

    if let Some(signal) = options.signal {
        let mut abort = signal.subscribe();

        select! {
            result = async {
                let mut file = open_options.open(&resolved).await
                    .map_err(|e| HostError::new("FS_IO", format!("Failed to open file: {}", e)))?;
                file.write_all(text.as_bytes()).await
                    .map_err(|e| HostError::new("FS_IO", format!("Write failed: {}", e)))?;
                file.flush()
                    .await
                    .map_err(|e| HostError::new("FS_IO", format!("Flush failed: {}", e)))?;
                Ok(())
            } => {
                result
            }

            abort_reason = abort.recv() => {
                // println!("write_text_file: Received abort signal");
                Err(RongJSError::from_thrown_value(abort_reason))
            }
        }
    } else {
        let mut file = open_options
            .open(&resolved)
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Failed to open file: {}", e)))?;
        file.write_all(text.as_bytes())
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Write failed: {}", e)))?;
        file.flush()
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Flush failed: {}", e)).into())
    }
}

async fn write_file(
    file: String,
    data: JSTypedArray,
    option: Optional<WriteFileOptions>,
) -> JSResult<()> {
    let resolved = grant_file_access(&file)?;
    let options = option.0.unwrap_or_default();

    // Get bytes from TypedArray
    let bytes = data.as_bytes().ok_or_else(|| {
        HostError::new(rong::error::E_INVALID_ARG, "Invalid TypedArray data").with_name("TypeError")
    })?;

    // Handle createNew option
    if options.create_new.unwrap_or(false) && fs::metadata(&resolved).await.is_ok() {
        return Err(HostError::new(rong::error::E_ALREADY_EXISTS, "File already exists").into());
    }

    // Handle append option
    let mut open_options = fs::OpenOptions::new();
    open_options
        .write(true)
        .create(true)
        .truncate(!options.append.unwrap_or(false))
        .append(options.append.unwrap_or(false));

    // Handle mode option (Unix-like systems, including macOS)
    #[cfg(unix)]
    if let Some(mode) = options.mode {
        open_options.mode(mode);
    }

    if let Some(signal) = options.signal {
        let mut abort = signal.subscribe();

        select! {
            result = async {
                let mut file = open_options.open(&resolved).await
                    .map_err(|e| HostError::new("FS_IO", format!("Failed to open file: {}", e)))?;
                file.write_all(bytes).await
                    .map_err(|e| HostError::new("FS_IO", format!("Write failed: {}", e)))?;
                file.flush()
                    .await
                    .map_err(|e| HostError::new("FS_IO", format!("Flush failed: {}", e)))?;
                Ok(())
            } => {
                result
            }

            abort_reason = abort.recv() => {
                Err(RongJSError::from_thrown_value(abort_reason))
            }
        }
    } else {
        let mut file = open_options
            .open(&resolved)
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Failed to open file: {}", e)))?;
        file.write_all(bytes)
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Write failed: {}", e)))?;
        file.flush()
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Flush failed: {}", e)).into())
    }
}

async fn copy_file(from: String, to: String) -> JSResult<()> {
    let resolved_from = grant_file_access(&from)?;
    let resolved_to = grant_file_access(&to)?;
    fs::copy(&resolved_from, &resolved_to)
        .await
        .map(|_| ())
        .map_err(|e| HostError::new("FS_IO", format!("Failed to copy file: {}", e)).into())
}

async fn truncate(path: String, len: Optional<f64>) -> JSResult<()> {
    let resolved = grant_file_access(&path)?;
    let len = len.unwrap_or(0.0);
    fs::OpenOptions::new()
        .write(true)
        .open(&resolved)
        .await
        .map_err(|e| HostError::new("FS_IO", format!("Failed to open file: {}", e)))?
        .set_len(len as u64)
        .await
        .map_err(|e| HostError::new("FS_IO", format!("Failed to truncate file: {}", e)))?;
    Ok(())
}

pub(crate) fn init(ctx: &JSContext) -> JSResult<()> {
    let rong = ctx.rong();

    let write_text = JSFunc::new(ctx, write_text_file)?.name("writeTextFile")?;
    rong.set("writeTextFile", write_text)?;

    let write = JSFunc::new(ctx, write_file)?.name("writeFile")?;
    rong.set("writeFile", write)?;

    let truncate_fn = JSFunc::new(ctx, truncate)?.name("truncate")?;
    rong.set("truncate", truncate_fn)?;

    let copy = JSFunc::new(ctx, copy_file)?.name("copyFile")?;
    rong.set("copyFile", copy)?;

    Ok(())
}
