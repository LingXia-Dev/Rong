//! # Rong Storage Module
//!
//! Asynchronous key-value storage backed by `redb`, can manage their own database
//! files explicitly.
//!
//! ## Features
//! - Promise-based async API
//! - Type-preserving JSON serialization
//! - Configurable storage location per instance via constructor path argument
//! - Iterator support for key listing
//! - Automatic database creation when the file does not exist
//! - Optional per-instance limits (key/value/data size caps)
//!
//! ## Supported Data Types
//! - **Strings**, **Numbers**, **BigInts**, **Booleans**, **null**
//! - **Objects** and **Arrays** serialized via `JSON.stringify`
//! - **Date** objects preserved via a lightweight metadata envelope
//!
//! ## Limitations
//! - `undefined` values are rejected
//! - Extremely large unsigned values like `u64::MAX` may not round-trip perfectly due to
//!   JavaScript BigInt to native type conversion
//! - Maximum key size: 1KB
//! - Maximum value size: 5MB
//! - Maximum total storage (including redb overhead): ~21.5MB

use redb::TableDefinition;
use rong::*;

mod storage;
pub use storage::*;

// Table definition shared by all storage instances
pub(crate) const STORAGE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("storage");

// Default size limits
const REDB_INIT_OVERHEAD: usize = (1.5 * 1024.0 * 1024.0) as usize; // ~1.5MB redb initialization overhead
pub(crate) const DEFAULT_MAX_TOTAL_SIZE: usize = 20 * 1024 * 1024; // 20MB user data limit
pub(crate) const DEFAULT_MAX_USER_DATA_SIZE: usize = DEFAULT_MAX_TOTAL_SIZE + REDB_INIT_OVERHEAD;
pub(crate) const DEFAULT_MAX_KEY_SIZE: usize = 1024; // 1KB
pub(crate) const DEFAULT_MAX_VALUE_SIZE: usize = 5 * 1024 * 1024; // 5MB

// size is in bytes
#[derive(IntoJSObject)]
/// Current storage usage and configured limits.
pub struct StorageInfo {
    /// Current serialized user-data size in bytes.
    #[js_name = "currentSize"]
    pub(crate) current_size: u32,
    /// Configured user-data size limit in bytes.
    #[js_name = "limitSize"]
    pub(crate) limit_size: u32,
    /// Number of stored keys.
    #[js_name = "keyCount"]
    pub(crate) key_count: u32,
}

/// Initialize the Storage module
pub fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_class::<Storage>()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;
    use std::env;

    fn workspace_root() -> std::path::PathBuf {
        env::current_dir()
            .expect("cwd")
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root")
            .to_path_buf()
    }

    #[test]
    fn test_storage() {
        async_run!(|ctx: JSContext| async move {
            let root = workspace_root();
            let storage_path = format!("{}/target/test-tmp/test_storage.db", root.display());
            ctx.global().set("TEST_STORAGE_DB_PATH", storage_path)?;

            rong_assert::init(&ctx)?;
            rong_console::init(&ctx)?;
            init(&ctx)?;

            let passed = UnitJSRunner::load_script(&ctx, "storage.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }

    #[test]
    fn test_storage_injected() {
        async_run!(|ctx: JSContext| async move {
            let root = workspace_root();
            let db_path = root.join("target/test-tmp/test_storage_injected.db");

            rong_assert::init(&ctx)?;
            rong_console::init(&ctx)?;
            init(&ctx)?;

            // Create a pre-configured Storage from Rust with custom limits,
            // then inject it as a global `storage` — JS never calls `new Storage`.
            let storage = Storage::new(
                db_path,
                StorageOptions {
                    max_data_size: Some(10 * 1024 * 1024),
                    ..Default::default()
                },
            )?;
            let js_storage = Class::lookup::<Storage>(&ctx)?.instance(storage);
            ctx.global().set("storage", js_storage)?;

            let passed = UnitJSRunner::load_script(&ctx, "storage_injected.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }

    #[test]
    fn rust_open_api_provides_handles() {
        let root = workspace_root();

        let default_path = root.join("target/test-tmp/rust_storage_default.db");
        Storage::new(default_path, StorageOptions::default()).expect("default open should succeed");

        let custom_path = root.join("target/test-tmp/rust_storage_custom.db");
        let options = StorageOptions {
            max_key_size: Some(16),
            ..Default::default()
        };
        Storage::new(custom_path, options).expect("custom open should succeed");
    }

    #[test]
    fn close_allows_reopen_same_path() {
        let root = workspace_root();

        let path = root.join("target/test-tmp/rust_storage_reopen.db");

        // First open and immediately close the database.
        let storage =
            Storage::new(&path, StorageOptions::default()).expect("initial open should succeed");
        storage.close();

        // After close, reopening the same path in the same process should succeed
        // without hitting redb's "Database already open. Cannot acquire lock" error.
        let _storage2 = Storage::new(&path, StorageOptions::default())
            .expect("reopen after close should succeed without lock error");
    }
}
