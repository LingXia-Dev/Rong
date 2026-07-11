use rong::*;
use std::{fs, time::SystemTime};

#[js_class]
pub(crate) struct FileInfo {
    is_file: bool,
    is_directory: bool,
    is_symlink: bool,
    size: f64,
    modified: Option<SystemTime>,
    accessed: Option<SystemTime>,
    created: Option<SystemTime>,
    mode: Option<u32>,
}

impl FileInfo {
    /// Create a new FileInfo from fs::Metadata
    pub(crate) fn from_metadata(metadata: fs::Metadata) -> Self {
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::MetadataExt;
            Some(metadata.mode())
        };
        #[cfg(not(unix))]
        let mode = None;

        Self {
            is_file: metadata.is_file(),
            is_directory: metadata.is_dir(),
            is_symlink: metadata.is_symlink(),
            size: metadata.len() as f64,
            modified: metadata.modified().ok(),
            accessed: metadata.accessed().ok(),
            created: metadata.created().ok(),
            mode,
        }
    }
}

/// File or directory metadata returned by stat operations.
#[js_class]
impl FileInfo {
    #[js_method(constructor, private)]
    fn new() -> JSResult<Self> {
        rong::illegal_constructor(
            "FileInfo cannot be constructed directly. Use stat(), lstat(), or FileHandle.stat().",
        )
    }

    /// Whether this metadata describes a regular file.
    #[js_method(getter, rename = "isFile")]
    fn is_file(&self) -> bool {
        self.is_file
    }

    /// Whether this metadata describes a directory.
    #[js_method(getter, rename = "isDirectory")]
    fn is_directory(&self) -> bool {
        self.is_directory
    }

    /// Whether this metadata describes a symbolic link.
    #[js_method(getter, rename = "isSymlink")]
    fn is_symlink(&self) -> bool {
        self.is_symlink
    }

    /// Size in bytes.
    #[js_method(getter)]
    fn size(&self) -> f64 {
        self.size
    }

    /// Last modification time in Unix epoch milliseconds, when available.
    #[js_method(getter)]
    fn modified(&self) -> Option<f64> {
        self.modified
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as f64)
    }

    /// Last access time in Unix epoch milliseconds, when available.
    #[js_method(getter)]
    fn accessed(&self) -> Option<f64> {
        self.accessed
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as f64)
    }

    /// Creation time in Unix epoch milliseconds, when available.
    #[js_method(getter)]
    fn created(&self) -> Option<f64> {
        self.created
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as f64)
    }

    /// Unix permissions mode, or null on unsupported platforms.
    #[js_method(getter)]
    fn mode(&self) -> Option<u32> {
        self.mode
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

pub(crate) fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_hidden_class::<FileInfo>()?;
    Ok(())
}
