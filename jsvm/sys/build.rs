use std::env;
use std::path::PathBuf;

fn write_placeholder_bindings() {
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    std::fs::write(
        out_path.join("bindings.rs"),
        "// Placeholder bindings generated for non-Harmony/doc builds.\n",
    )
    .expect("Couldn't write placeholder bindings!");
}

fn harmony_setup() {
    let ndk = env::var("OHOS_NDK_HOME").expect("OHOS_NDK_HOME is not set!");
    unsafe {
        env::set_var(
            "CC",
            format!("{ndk}/native/llvm/bin/aarch64-unknown-linux-ohos-clang"),
        );

        env::set_var("AR", format!("{ndk}/native/llvm/bin/llvm-ar"));

        // for bindgen
        env::set_var(
            "BINDGEN_EXTRA_CLANG_ARGS",
            format!("--sysroot {ndk}/native/sysroot"),
        );
    }
}

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let docs_rs = env::var_os("DOCS_RS").is_some();

    if docs_rs {
        println!("cargo:warning=Generating placeholder JSVM bindings for docs.rs");
        write_placeholder_bindings();
        return;
    }

    if target_os != "linux" || target_env != "ohos" {
        write_placeholder_bindings();
        return;
    }

    // This binding is only for HarmonyOS Ark JS.
    harmony_setup();
    build_harmony_jsvm();
}

fn build_harmony_jsvm() {
    let ohos_ndk_home = env::var("OHOS_NDK_HOME")
        .expect("OHOS_NDK_HOME environment variable must be set for HarmonyOS builds");

    // Path to Ark Runtime headers
    let ark_runtime_path = PathBuf::from(&ohos_ndk_home)
        .join("native")
        .join("sysroot")
        .join("usr")
        .join("include")
        .join("ark_runtime");

    println!(
        "cargo:warning=Ark Runtime header path: {:?}",
        ark_runtime_path
    );

    if !ark_runtime_path.exists() {
        panic!(
            "Ark Runtime headers not found at: {}. Please ensure OHOS_NDK_HOME is correctly set.",
            ark_runtime_path.to_string_lossy()
        );
    }

    // Check for required header files
    let jsvm_header = ark_runtime_path.join("jsvm.h");
    let jsvm_types_header = ark_runtime_path.join("jsvm_types.h");

    if !jsvm_header.exists() {
        panic!("jsvm.h not found at: {}", jsvm_header.to_string_lossy());
    }

    if !jsvm_types_header.exists() {
        panic!(
            "jsvm_types.h not found at: {}",
            jsvm_types_header.to_string_lossy()
        );
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=OHOS_NDK_HOME");

    // Link against the Ark Runtime library
    let lib_path = PathBuf::from(&ohos_ndk_home)
        .join("native")
        .join("sysroot")
        .join("usr")
        .join("lib");

    println!(
        "cargo:rustc-link-search=native={}",
        lib_path.to_string_lossy()
    );
    println!("cargo:rustc-link-lib=dylib=jsvm");

    let bindings = bindgen::Builder::default()
        .header(jsvm_header.to_string_lossy())
        .clang_arg(format!("-I{}", ark_runtime_path.to_string_lossy()))
        .clang_arg(format!("--sysroot={}/native/sysroot", ohos_ndk_home))
        // Allow JSVM functions, types, and constants
        .allowlist_function("JSVM_.*")
        .allowlist_type("JSVM_.*")
        .allowlist_var("JSVM_.*")
        // Also include any OH_ prefixed items (OpenHarmony)
        .allowlist_function("OH_.*")
        .allowlist_type("OH_.*")
        .allowlist_var("OH_.*")
        .generate()
        .expect("Unable to generate bindings for Harmony Ark JS");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
