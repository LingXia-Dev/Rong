use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

const ENV_WEBKIT_ROOT: &str = "RONG_JSC_WEBKIT_ROOT";
const ENV_WEBKIT_INCLUDE_DIR: &str = "RONG_JSC_WEBKIT_INCLUDE_DIR";
const ENV_WEBKIT_LIB_DIR: &str = "RONG_JSC_WEBKIT_LIB_DIR";
const ENV_WEBKIT_LIB_NAME: &str = "RONG_JSC_WEBKIT_LIB_NAME";
const ENV_WEBKIT_LINK_KIND: &str = "RONG_JSC_WEBKIT_LINK_KIND";
const ENV_WEBKIT_EXTRA_LIBS: &str = "RONG_JSC_WEBKIT_EXTRA_LIBS";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed={}", ENV_WEBKIT_ROOT);
    println!("cargo:rerun-if-env-changed={}", ENV_WEBKIT_INCLUDE_DIR);
    println!("cargo:rerun-if-env-changed={}", ENV_WEBKIT_LIB_DIR);
    println!("cargo:rerun-if-env-changed={}", ENV_WEBKIT_LIB_NAME);
    println!("cargo:rerun-if-env-changed={}", ENV_WEBKIT_LINK_KIND);
    println!("cargo:rerun-if-env-changed={}", ENV_WEBKIT_EXTRA_LIBS);

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let use_webkit_provider = env::var_os("CARGO_FEATURE_PROVIDER_WEBKIT").is_some();

    if use_webkit_provider {
        configure_webkit_provider(&target_os);
    } else {
        configure_system_provider(&target_os);
    }
}

fn configure_system_provider(target_os: &str) {
    let sdk_name = match target_os {
        "macos" => "macosx",
        "ios" => "iphoneos",
        other => {
            panic!(
                "JavaScriptCore system provider is only available on Apple targets. \
Enable feature `provider-webkit` for non-Apple builds. target_os={}",
                other
            );
        }
    };

    let sdk_path_output = Command::new("xcrun")
        .args(["--sdk", sdk_name, "--show-sdk-path"])
        .output()
        .expect("Failed to execute xcrun to get SDK path");

    if !sdk_path_output.status.success() {
        panic!(
            "xcrun failed to get SDK path for SDK '{}': {:?}",
            sdk_name,
            String::from_utf8_lossy(&sdk_path_output.stderr)
        );
    }

    let sdk_path = String::from_utf8(sdk_path_output.stdout)
        .expect("Failed to parse xcrun output as UTF-8")
        .trim()
        .to_string();

    // full path JavaScriptCore.framework/Headers
    let framework_path = "System/Library/Frameworks/JavaScriptCore.framework/Headers";
    let header_path = PathBuf::from(&sdk_path).join(framework_path);

    let header_file = header_path.join("JavaScript.h");

    if !header_file.exists() {
        panic!("Header file not found: {}", header_file.to_string_lossy());
    }

    println!("cargo:rustc-link-lib=framework=JavaScriptCore");

    generate_bindings(
        &header_file,
        vec![
            format!("-I{}", header_path.to_string_lossy()),
            format!("-isysroot{}", sdk_path),
        ],
    );
}

fn configure_webkit_provider(target_os: &str) {
    if target_os == "ios" {
        panic!(
            "Feature `provider-webkit` is not supported on iOS. Use the system JavaScriptCore provider instead."
        );
    }

    let root_dir = env::var(ENV_WEBKIT_ROOT).ok().map(PathBuf::from);
    let include_dir = resolve_include_dir(root_dir.as_deref());
    let lib_dir = resolve_lib_dir(root_dir.as_deref());
    let lib_name = env::var(ENV_WEBKIT_LIB_NAME).unwrap_or_else(|_| "JavaScriptCore".to_string());
    let link_kind = env::var(ENV_WEBKIT_LINK_KIND).unwrap_or_else(|_| "dylib".to_string());
    let link_kind = normalize_link_kind(&link_kind);

    let header_file = resolve_javascript_header(&include_dir);
    let mut clang_args = vec![format!("-I{}", include_dir.to_string_lossy())];
    if let Some(parent) = include_dir.parent() {
        clang_args.push(format!("-I{}", parent.to_string_lossy()));
    }

    if link_kind == "framework" {
        println!("cargo:rustc-link-search=framework={}", lib_dir.to_string_lossy());
        clang_args.push(format!("-F{}", lib_dir.to_string_lossy()));
        println!("cargo:rustc-link-lib=framework={}", lib_name);
    } else {
        println!("cargo:rustc-link-search=native={}", lib_dir.to_string_lossy());
        println!("cargo:rustc-link-lib={}={}", link_kind, lib_name);
    }

    if let Ok(extra_libs) = env::var(ENV_WEBKIT_EXTRA_LIBS) {
        for lib in extra_libs.split(',').map(str::trim).filter(|lib| !lib.is_empty()) {
            println!("cargo:rustc-link-lib={}", lib);
        }
    }

    generate_bindings(&header_file, clang_args);
}

fn resolve_include_dir(root_dir: Option<&Path>) -> PathBuf {
    if let Ok(include_dir) = env::var(ENV_WEBKIT_INCLUDE_DIR) {
        return PathBuf::from(include_dir);
    }

    if let Some(root) = root_dir {
        if let Some(resolved) = detect_include_dir(root) {
            return resolved;
        }
    }

    panic!(
        "Missing WebKit include configuration. Set `{}` or provide `{}` with a valid include layout.",
        ENV_WEBKIT_INCLUDE_DIR, ENV_WEBKIT_ROOT
    );
}

fn resolve_lib_dir(root_dir: Option<&Path>) -> PathBuf {
    if let Ok(lib_dir) = env::var(ENV_WEBKIT_LIB_DIR) {
        return PathBuf::from(lib_dir);
    }

    if let Some(root) = root_dir {
        if let Some(resolved) = detect_lib_dir(root) {
            return resolved;
        }
    }

    panic!(
        "Missing WebKit library configuration. Set `{}` or provide `{}` with a valid library layout.",
        ENV_WEBKIT_LIB_DIR, ENV_WEBKIT_ROOT
    );
}

fn normalize_link_kind(kind: &str) -> &'static str {
    match kind.to_ascii_lowercase().as_str() {
        "dylib" => "dylib",
        "static" => "static",
        "framework" => "framework",
        _ => panic!(
            "Unsupported {} value: {}. Supported values: dylib, static, framework",
            ENV_WEBKIT_LINK_KIND, kind
        ),
    }
}

fn detect_include_dir(root: &Path) -> Option<PathBuf> {
    let candidates = [
        root.to_path_buf(),
        root.join("include"),
        root.join("Headers"),
        root.join("JavaScriptCore.framework").join("Headers"),
        root.join("Frameworks")
            .join("JavaScriptCore.framework")
            .join("Headers"),
        root.join("WebKitBuild")
            .join("Release")
            .join("JavaScriptCore.framework")
            .join("Headers"),
        root.join("WebKitBuild")
            .join("Debug")
            .join("JavaScriptCore.framework")
            .join("Headers"),
    ];

    candidates
        .into_iter()
        .find(|candidate| contains_javascript_header(candidate))
}

fn detect_lib_dir(root: &Path) -> Option<PathBuf> {
    let candidates = [
        root.join("lib"),
        root.join("lib64"),
        root.join("Frameworks"),
        root.join("WebKitBuild").join("Release"),
        root.join("WebKitBuild").join("Debug"),
        root.to_path_buf(),
    ];

    candidates.into_iter().find(|candidate| candidate.is_dir())
}

fn contains_javascript_header(include_dir: &Path) -> bool {
    include_dir.join("JavaScript.h").exists()
        || include_dir.join("JavaScriptCore").join("JavaScript.h").exists()
}

fn resolve_javascript_header(include_dir: &Path) -> PathBuf {
    let root = include_dir;
    let direct = root.join("JavaScript.h");
    if direct.exists() {
        return direct;
    }

    let nested = root.join("JavaScriptCore").join("JavaScript.h");
    if nested.exists() {
        return nested;
    }

    panic!(
        "Cannot find JavaScript.h under {}. Tried JavaScript.h and JavaScriptCore/JavaScript.h",
        include_dir.to_string_lossy()
    );
}

fn generate_bindings(header_file: &PathBuf, clang_args: Vec<String>) {
    let mut builder = bindgen::Builder::default()
        .header(header_file.to_string_lossy())
        .allowlist_function("JS.*")
        .allowlist_type("JS.*")
        .allowlist_var("JS.*")
        .allowlist_var("kJS.*");

    for arg in clang_args {
        builder = builder.clang_arg(arg);
    }

    let bindings = builder
        .generate()
        .expect("Unable to generate bindings for JavaScriptCore");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
