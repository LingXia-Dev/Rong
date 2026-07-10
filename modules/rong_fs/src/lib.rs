//! `rong_fs` exposes a layered filesystem API:
//!
//! - `Rong.file(path)` returns a `RongFile`, which is a lazy path reference for
//!   high-level whole-file operations such as `text()`, `json()`, `bytes()`,
//!   `stat()`, and `exists()`.
//! - `RongFile.open()` returns a `FileHandle`, which represents an opened file
//!   descriptor for random access, seek/truncate, and readable/writable streams.
//! - `RongFile.writer()` returns a `FileSink`, which is a write-only sink for
//!   append and incremental streaming writes when full handle semantics are not
//!   needed.
//!
//! `Rong.write(...)` is the convenience entry point for one-shot writes, while
//! directory/path operations stay on the top-level `Rong` namespace.

use rong::*;
use std::path::PathBuf;

mod dir;
mod file;
mod misc;
mod rong_file;
mod sink;
mod stat;
mod write;

rong::js_api! {
    fn register_fs_namespace(ctx) {
        namespace RongNamespace = ctx.host_namespace();
        fn file = rong_file::file;
        fn write(
            ts_params = "dest: string | RongFile, data: string | ArrayBufferView | ArrayBuffer | RongFile"
        ) = write::rong_write;
        fn mkdir(ts_params = "path: string, options?: MkdirOptions") = dir::mkdir;
        fn readDir(
            ts_params = "path: string",
            ts_return = "AsyncIterableIterator<DirEntry>"
        ) = dir::readdir;
        fn remove(ts_params = "path: string, options?: RemoveOptions") = dir::remove;
        fn chdir(ts_params = "path: string") = dir::chdir;
        fn symlink(ts_params = "target: string, path: string") = misc::symlink;
        fn readlink = misc::readlink;
        fn chmod(cfg = "unix") = misc::chmod;
        fn chown(cfg = "unix") = misc::chown;
        fn utime(ts_params = "path: string, options: UTimeOptions") = misc::utime;
        fn rename(ts_params = "oldPath: string, newPath: string") = misc::rename;
        fn realPath = misc::real_path;
        const SeekMode: "typeof SeekMode" = file::SeekMode::js_object(ctx)?;
    }
}

/// Internal function to resolve file paths.
fn grant_file_access(path: &str) -> JSResult<PathBuf> {
    Ok(PathBuf::from(path))
}

pub fn init(ctx: &JSContext) -> JSResult<()> {
    // Ensure stream classes are registered for readable/writable support
    rong_stream::init(ctx)?;
    stat::init(ctx)?;
    sink::init(ctx)?;
    file::init(ctx)?;
    rong_file::init(ctx)?;
    dir::init(ctx)?;

    register_fs_namespace(ctx)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;
    use std::env;
    use std::path::PathBuf;

    #[cfg(windows)]
    fn js_path(path: PathBuf) -> String {
        let path = path.to_string_lossy();
        path.strip_prefix(r"\\?\").unwrap_or(&path).to_owned()
    }

    #[cfg(not(windows))]
    fn js_path(path: PathBuf) -> String {
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn test_filesystem() {
        async_run!(|ctx: JSContext| async move {
            rong_encoding::init(&ctx)?;
            rong_console::init(&ctx)?;
            rong_assert::init(&ctx)?;
            rong_abort::init(&ctx)?;
            rong_exception::init(&ctx)?;
            init(&ctx)?;

            // Keep permission-sensitive cases (chmod) on the OS temp
            // filesystem. WSL's /mnt/c drvfs does not reliably preserve Unix
            // mode bits even when the repo itself lives there.
            let workspace_root_path =
                env::temp_dir().join(format!("rong-fs-tests-{}", std::process::id()));
            std::fs::create_dir_all(&workspace_root_path).unwrap();
            let workspace_root = js_path(
                workspace_root_path
                    .canonicalize()
                    .unwrap_or(workspace_root_path),
            );

            // Inject workspace root into JavaScript environment
            ctx.global().set("WORKSPACE_ROOT", workspace_root)?;

            let passed_fs = UnitJSRunner::load_script(&ctx, "filesystem.js")
                .await?
                .run()
                .await?;
            assert!(passed_fs);

            Ok(())
        });
    }
}
