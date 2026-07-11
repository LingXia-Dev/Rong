use crate::grant_file_access;
use crate::rong_file::RongFile;
use rong::*;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

/// Resolve destination to a PathBuf. Accepts a string path or a RongFile object.
fn resolve_dest(dest: &JSValue) -> JSResult<PathBuf> {
    // Try as string first
    if dest.is_string() {
        let path: String = dest.clone().to_rust()?;
        return grant_file_access(&path);
    }

    // Try as RongFile object
    if let Some(obj) = dest.clone().into_object()
        && let Ok(rf) = obj.borrow::<RongFile>()
    {
        return Ok(rf.resolved().clone());
    }

    Err(HostError::new(
        rong::error::E_INVALID_ARG,
        "destination must be a string path or RongFile",
    )
    .with_name("TypeError")
    .into())
}

/// Universal write: Rong.write(dest, data) -> Promise<number>
///
/// dest: string path or RongFile
/// data: string, TypedArray, ArrayBuffer, or RongFile (copy)
pub(crate) async fn rong_write(_ctx: JSContext, dest: JSValue, data: JSValue) -> JSResult<f64> {
    let resolved = resolve_dest(&dest)?;

    // Dispatch on data type
    if data.is_string() {
        // Write string as UTF-8
        let text: String = data.to_rust()?;
        let bytes = text.as_bytes();
        let len = bytes.len();

        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&resolved)
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Failed to open file: {}", e)))?;
        file.write_all(bytes)
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Write failed: {}", e)))?;
        file.flush()
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Flush failed: {}", e)))?;

        return Ok(len as f64);
    }

    if data.is_array_buffer() {
        // Write ArrayBuffer
        let ab: JSArrayBuffer = data.to_rust()?;
        let bytes = ab.as_slice();
        let len = bytes.len();

        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&resolved)
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Failed to open file: {}", e)))?;
        file.write_all(bytes)
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Write failed: {}", e)))?;
        file.flush()
            .await
            .map_err(|e| HostError::new("FS_IO", format!("Flush failed: {}", e)))?;

        return Ok(len as f64);
    }

    // Try as object (TypedArray or RongFile)
    if let Some(obj) = data.clone().into_object() {
        // Try as RongFile (copy semantics)
        let source = {
            if let Ok(rf) = obj.borrow::<RongFile>() {
                Some(rf.resolved().clone())
            } else {
                None
            }
        };
        if let Some(source) = source {
            let bytes_copied = tokio::fs::copy(&source, &resolved)
                .await
                .map_err(|e| HostError::new("FS_IO", format!("Failed to copy file: {}", e)))?;
            return Ok(bytes_copied as f64);
        }

        // Try as TypedArray (Uint8Array, etc.)
        if let Some(ta) = AnyJSTypedArray::from_object(obj)
            && let Some(bytes) = ta.as_bytes()
        {
            let len = bytes.len();

            let mut file = tokio::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&resolved)
                .await
                .map_err(|e| HostError::new("FS_IO", format!("Failed to open file: {}", e)))?;
            file.write_all(bytes)
                .await
                .map_err(|e| HostError::new("FS_IO", format!("Write failed: {}", e)))?;
            file.flush()
                .await
                .map_err(|e| HostError::new("FS_IO", format!("Flush failed: {}", e)))?;

            return Ok(len as f64);
        }
    }

    Err(HostError::new(
        rong::error::E_INVALID_ARG,
        "data must be a string, ArrayBuffer, TypedArray, or RongFile",
    )
    .with_name("TypeError")
    .into())
}
