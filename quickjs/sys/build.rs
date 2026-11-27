use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn checkout_submodule() {
    let output = Command::new("git")
        .args(["submodule", "status"])
        .output()
        .expect("Failed to exec `git submodule status`");

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    if stdout.lines().any(|line| line.starts_with('-')) {
        Command::new("git")
            .args(["submodule", "update", "--init"])
            .output()
            .expect("Failed to checkout quickjs-ng");
    }
}

fn harmony_setup(build: &mut cc::Build) {
    let ndk = env::var("OHOS_NDK_HOME").expect("OHOS_NDK_HOME is not set!");

    build.compiler(format!(
        "{ndk}/native/llvm/bin/aarch64-unknown-linux-ohos-clang"
    ));
    build.archiver(format!("{ndk}/native/llvm/bin/llvm-ar"));

    unsafe {
        env::set_var(
            "BINDGEN_EXTRA_CLANG_ARGS",
            format!("--sysroot {ndk}/native/sysroot"),
        );
    }
}

fn android_setup(build: &mut cc::Build) {
    let ndk = env::var("ANDROID_NDK_ROOT").expect("ANDROID_NDK_ROOT is not set!");
    let arch = env::consts::ARCH;
    let os = if env::consts::OS == "macos" {
        "Darwin"
    } else {
        env::consts::OS
    };
    let api = env::var("ANDROID_API_LEVEL").unwrap_or("33".to_string());

    let target = env::var("TARGET").unwrap_or_default();
    let cc_target = if target.contains("armv7") {
        "armv7a-linux-androideabi"
    } else {
        "aarch64-linux-android"
    };

    build.compiler(format!(
        "{ndk}/toolchains/llvm/prebuilt/{os}-{arch}/bin/clang"
    ));
    build.archiver(format!(
        "{ndk}/toolchains/llvm/prebuilt/{os}-{arch}/bin/llvm-ar"
    ));
    build.flag("-target");
    build.flag(format!("{cc_target}{api}"));

    unsafe {
        env::set_var(
            "BINDGEN_EXTRA_CLANG_ARGS",
            format!("--sysroot {ndk}/toolchains/llvm/prebuilt/{os}-{arch}/sysroot"),
        );
    }
}

fn ios_setup(build: &mut cc::Build) {
    let sdk = Command::new("xcrun")
        .args(["--show-sdk-path", "--sdk", "iphoneos"])
        .output()
        .expect("failed to execute xcrun")
        .stdout;
    let sdk_path = String::from_utf8_lossy(&sdk).trim().to_string();

    let clang = Command::new("xcrun")
        .args(["--find", "clang"])
        .output()
        .expect("failed to find clang")
        .stdout;
    let clang_path = String::from_utf8_lossy(&clang).trim().to_string();

    build.compiler(&clang_path);
    build.flag("-isysroot");
    build.flag(&sdk_path);
    build.flag("-arch");
    build.flag("arm64");
    build.flag("-mios-version-min=11.0");

    let ar = Command::new("xcrun")
        .args(["--find", "ar"])
        .output()
        .expect("failed to find ar")
        .stdout;
    let ar_path = String::from_utf8_lossy(&ar).trim().to_string();
    build.archiver(&ar_path);

    unsafe {
        env::set_var(
            "BINDGEN_EXTRA_CLANG_ARGS",
            format!("-isysroot {}", sdk_path),
        );
    }
}

fn inject_once(src: &mut String, anchor: &str, replacement: &str, label: &str) {
    assert!(
        src.contains(anchor),
        "Failed to locate JIT hook anchor `{label}` in quickjs-ng/quickjs.c"
    );
    *src = src.replacen(anchor, replacement, 1);
}

/// Inject minimal `#ifdef CONFIG_RONG_JIT` hooks into the build-time merged
/// `quickjs.c` so the vendored `quickjs-ng` submodule stays untouched.
fn apply_jit_hooks(src: &mut String) {
    // 1. Include qjs.h right after quickjs.h so hook typedefs are visible in struct JSRuntime.
    inject_once(
        src,
        "#include \"quickjs.h\"",
        "#include \"quickjs.h\"\n#include \"qjs.h\"",
        "include qjs.h",
    );

    // 2. Add JIT hook fields to struct JSRuntime.
    inject_once(
        src,
        "    JSRuntimeFinalizerState *finalizers;\n};",
        "    JSRuntimeFinalizerState *finalizers;\n#ifdef CONFIG_RONG_JIT\n    void *jit_opaque;\n    QJSJitCallHook jit_call;\n    QJSJitFreeBytecodeHook jit_free_bytecode;\n    QJSJitFreeRuntimeHook jit_free_runtime;\n    uint8_t jit_enabled;\n    uint64_t jit_direct_calls;\n#endif\n};",
        "JSRuntime jit fields",
    );

    // 3. Add JIT metadata fields to struct JSFunctionBytecode.
    inject_once(
        src,
        "    char *source;\n} JSFunctionBytecode;",
        "    char *source;\n#ifdef CONFIG_RONG_JIT\n    void *jit_data;\n    QJSJitNativeFn jit_entry;\n    uint32_t jit_call_count;\n#endif\n} JSFunctionBytecode;",
        "JSFunctionBytecode jit fields",
    );

    // 4. Intercept normal bytecode function calls in JS_CallInternal.
    inject_once(
        src,
        "    b = p->u.func.function_bytecode;\n\n    if (unlikely(argc < b->arg_count || (flags & JS_CALL_FLAG_COPY_ARGV))) {",
        "    b = p->u.func.function_bytecode;\n\n#ifdef CONFIG_RONG_JIT\n    if (unlikely(rt->jit_call != NULL) && b->func_kind == JS_FUNC_NORMAL && !(flags & JS_CALL_FLAG_CONSTRUCTOR)) {\n        JSValue jit_ret;\n        if (rt->jit_call(caller_ctx, rt->jit_opaque, b, &func_obj, &this_obj, argc, argv, flags, &jit_ret)) {\n            return jit_ret;\n        }\n    }\n#endif\n\n    if (unlikely(argc < b->arg_count || (flags & JS_CALL_FLAG_COPY_ARGV))) {",
        "JS_CallInternal jit hook",
    );

    // 5. Notify JIT when a JSFunctionBytecode is freed.
    inject_once(
        src,
        "static void free_function_bytecode(JSRuntime *rt, JSFunctionBytecode *b)\n{\n    int i;",
        "static void free_function_bytecode(JSRuntime *rt, JSFunctionBytecode *b)\n{\n    int i;\n#ifdef CONFIG_RONG_JIT\n    b->jit_entry = NULL;\n    if (b->jit_data != NULL && rt->jit_free_bytecode != NULL) {\n        rt->jit_free_bytecode(rt->jit_opaque, b, b->jit_data);\n        b->jit_data = NULL;\n    }\n#endif",
        "free_function_bytecode jit hook",
    );

    // 6. Notify JIT when the runtime is freed (after JS_RunGC frees all bytecodes).
    inject_once(
        src,
        "    init_list_head(&rt->job_list);\n\n    JS_RunGC(rt);",
        "    init_list_head(&rt->job_list);\n\n    JS_RunGC(rt);\n#ifdef CONFIG_RONG_JIT\n    if (rt->jit_free_runtime != NULL) {\n        QJSJitFreeRuntimeHook free_rt = rt->jit_free_runtime;\n        void *opaque = rt->jit_opaque;\n        rt->jit_call = NULL;\n        rt->jit_free_bytecode = NULL;\n        rt->jit_free_runtime = NULL;\n        rt->jit_opaque = NULL;\n        free_rt(opaque);\n    }\n#endif",
        "JS_FreeRuntime jit hook",
    );
}

/// Merge quickjs.c + extra.c into a single file
/// extra.c accesses quickjs internals so it must be compiled in the same translation unit.
fn merge_quickjs_source(out_dir: &Path, jit_enabled: bool) -> PathBuf {
    let mut quickjs_c = std::fs::read_to_string("quickjs-ng/quickjs.c")
        .expect("Failed to read quickjs-ng/quickjs.c")
        .replace("\r\n", "\n");
    let extra_c = std::fs::read_to_string("patch/extra.c").expect("Failed to read patch/extra.c");

    if jit_enabled {
        apply_jit_hooks(&mut quickjs_c);
    }

    let merged_path = out_dir.join("new_quickjs.c");
    std::fs::write(&merged_path, format!("{quickjs_c}\n{extra_c}"))
        .expect("Failed to write merged quickjs source");
    merged_path
}

fn build_quickjs(out_dir: &Path, jit_enabled: bool) {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let profile = env::var("PROFILE").unwrap();

    let mut build = cc::Build::new();

    // Platform-specific setup
    match target_os.as_str() {
        "linux" if target_env == "ohos" => harmony_setup(&mut build),
        "android" => android_setup(&mut build),
        "ios" => ios_setup(&mut build),
        _ => {} // macOS, Windows, etc. — cc crate auto-detects
    }

    // Include paths
    build.include("quickjs-ng");
    build.include("patch");
    build.define("QUICKJS_NG_BUILD", None);
    build.define("_GNU_SOURCE", None);
    if jit_enabled {
        build.define("CONFIG_RONG_JIT", None);
    }

    // Optimization
    if profile == "release" {
        build.opt_level(2);
        build.define("NDEBUG", None);
    } else {
        build.opt_level(0);
        build.debug(true);
    }

    // Warnings
    build.warnings(true);

    if target_os == "windows" {
        build.define("WIN32_LEAN_AND_MEAN", None);
        build.define("NOMINMAX", None);
    }
    if target_os == "windows" && target_env == "msvc" {
        build.flag_if_supported("/std:c11");
        build.flag_if_supported("/experimental:c11atomics");
        build.flag_if_supported("/J");
    }

    // Merge quickjs.c + extra.c (extra.c accesses quickjs internals)
    let merged = merge_quickjs_source(out_dir, jit_enabled);

    // Source files
    build.file(&merged); // quickjs.c + extra.c merged
    build.file("quickjs-ng/libregexp.c");
    build.file("quickjs-ng/libunicode.c");
    build.file("quickjs-ng/dtoa.c");
    build.file("patch/qjs.c");
    build.file("patch/inline.c");

    // Build static library named "quickjs"
    build.compile("quickjs");
}

fn generate_binding(out_dir: &str, jit_enabled: bool) {
    let mut builder = bindgen::Builder::default()
        .header("quickjs.wrapper.h")
        .clang_arg("-I./quickjs-ng")
        .clang_arg("-I./patch")
        .allowlist_var("JS_.*")
        .blocklist_type("JSClassID")
        .blocklist_type("JSClass")
        .blocklist_type("JSClassDef")
        .blocklist_type("JSCFunctionListEntry.*")
        .blocklist_type("JSClassExoticMethods")
        .opaque_type("FILE")
        .blocklist_type("FILE")
        .allowlist_function("[Q]*JS_.*")
        .blocklist_function("JS_.*Class.*")
        .blocklist_function("JS_GetOpaque")
        .blocklist_function("JS_SetOpaque")
        .blocklist_function("JS_GetOpaque2")
        .blocklist_function("JS_GetAnyOpaque")
        .blocklist_function("JS_NewCFunc.*")
        .blocklist_function("JS_.*List")
        .blocklist_function("JS_DumpMemoryUsage")
        .blocklist_item("JSCFunctionEnum.*")
        .blocklist_item("JSCFunctionType");

    if jit_enabled {
        builder = builder
            .clang_arg("-DCONFIG_RONG_JIT")
            .allowlist_type("QJSJit.*")
            .allowlist_var("QJS_OP_.*");
    }

    let bindings = builder
        .generate()
        .expect("Unable to generating bingdings for quickjs");

    bindings
        .write_to_file(Path::new(out_dir).join("quickjs.bindings.rs"))
        .expect("Failed to write bindings!");
}

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let jit_enabled = env::var_os("CARGO_FEATURE_JIT").is_some();

    checkout_submodule();
    build_quickjs(Path::new(&out_dir), jit_enabled);
    generate_binding(&out_dir, jit_enabled);

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=quickjs.wrapper.h");
    // Watch directories so edits/additions don't require updating this list.
    println!("cargo:rerun-if-changed=patch");
    println!("cargo:rerun-if-changed=quickjs-ng");
    println!("cargo:rerun-if-env-changed=NUM_JOBS");
    println!("cargo:rerun-if-env-changed=OHOS_NDK_HOME");
    println!("cargo:rerun-if-env-changed=ANDROID_NDK_ROOT");
    println!("cargo:rerun-if-env-changed=ANDROID_API_LEVEL");
}
