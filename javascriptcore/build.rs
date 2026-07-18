fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // Declared so rustc's unexpected_cfgs lint stays quiet.
    println!("cargo:rustc-check-cfg=cfg(jsc_source)");
    println!("cargo:rustc-check-cfg=cfg(jsc_interrupt)");

    // `rong_jscore_sys` (links = "jscore") publishes the backend it actually
    // linked via `cargo::metadata=backend=...`, surfaced to this build script as
    // `DEP_JSCORE_BACKEND`. Mirror it into a cfg so the source-only code in this
    // crate (JSC global init, the bytecode bridge, engine identity) tracks the
    // backend the sys crate built — instead of an independently-set cargo
    // feature that silently disagrees on non-Apple targets (where the sys crate
    // picks the source backend by default, with no `source` feature involved).
    println!("cargo:rerun-if-env-changed=DEP_JSCORE_BACKEND");
    let is_source = std::env::var("DEP_JSCORE_BACKEND").as_deref() == Ok("source");
    if is_source {
        println!("cargo:rustc-cfg=jsc_source");
    }

    // Source/JSCOnly artifacts always expose and use hard interruption. Apple's
    // system framework keeps the private-SPI reference behind an explicit
    // feature so production iOS consumers can leave it out.
    if is_source || std::env::var_os("CARGO_FEATURE_INTERRUPT_SPI").is_some() {
        println!("cargo:rustc-cfg=jsc_interrupt");
    }

    println!("cargo:rerun-if-env-changed=DEP_JSCORE_WEBKIT_REVISION");
    if let Ok(revision) = std::env::var("DEP_JSCORE_WEBKIT_REVISION") {
        println!("cargo:rustc-env=RONG_JSC_WEBKIT_REVISION={revision}");
    }
}
