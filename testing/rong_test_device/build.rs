//! Reads `tests/*.rs` from the workspace root, strips `#[test]` / `#[tokio::test]`
//! attributes, adjusts imports, and generates:
//!   - One module per test file in `$OUT_DIR/tests/<name>.rs`
//!   - A registry file `$OUT_DIR/registry.rs` with `all_tests() -> Vec<TestEntry>`

use regex::Regex;
use std::path::PathBuf;
use std::{env, fs};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let tests_dir = workspace_root.join("tests");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let gen_dir = out_dir.join("tests");
    fs::create_dir_all(&gen_dir).unwrap();

    // Tell cargo to re-run if any test file changes
    println!("cargo:rerun-if-changed={}", tests_dir.display());

    // Tests that cause native JSVM crashes (SIGABRT/SIGSEGV) which cannot be caught
    // by catch_unwind. These are excluded from registration to prevent app abort.
    // Entire files can be skipped via skip_files.
    let skip_files: std::collections::HashSet<&str> = [].into_iter().collect();
    let skip_fns: std::collections::HashSet<(&str, &str)> = [
        // ArkJS JSVM: accessing .stack property on thrown Error objects causes SIGABRT.
        // The JSVM internally aborts when reconstructing the stack trace from a
        // cleared exception. Related tests that don't access .stack pass fine.
        ("error", "test_error_stack"),
        ("error", "test_custom_error"),
        ("error", "test_error_display"),
    ]
    .into_iter()
    .collect();

    let test_attr = Regex::new(r"^#\[test\]\s*$").unwrap();
    let tokio_attr = Regex::new(r"^#\[tokio::test\]\s*$").unwrap();
    // Match function declarations that follow a stripped test attribute
    let fn_decl = Regex::new(r"^(\s*)(pub\s+)?(async\s+)?fn\s+").unwrap();

    let mut modules: Vec<ModuleInfo> = Vec::new();

    let mut entries: Vec<_> = fs::read_dir(&tests_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let stem = path.file_stem().unwrap().to_str().unwrap().to_string();
        let source = fs::read_to_string(&path).unwrap();

        // Skip files that don't use rong_test or rong
        if !source.contains("use rong_test::")
            && !source.contains("use rong_test;")
            && !source.contains("use rong::")
        {
            eprintln!("build.rs: skipping {} (no rong_test/rong import)", stem);
            continue;
        }

        // Skip files known to cause native JSVM crashes
        if skip_files.contains(stem.as_str()) {
            eprintln!(
                "build.rs: skipping {} (in skip list — causes JSVM abort)",
                stem
            );
            continue;
        }

        let (transformed, test_fns) = transform_source(
            &source,
            &test_attr,
            &tokio_attr,
            &fn_decl,
            &stem,
            &skip_fns,
            &tests_dir,
        );

        if test_fns.is_empty() {
            eprintln!("build.rs: skipping {} (no test functions found)", stem);
            continue;
        }

        let module_path = gen_dir.join(format!("{}.rs", stem));
        fs::write(&module_path, &transformed).unwrap();

        modules.push(ModuleInfo {
            name: stem,
            tests: test_fns,
        });
    }

    // Generate registry.rs
    let registry = generate_registry(&modules);
    fs::write(out_dir.join("registry.rs"), &registry).unwrap();

    // Generate mod.rs that declares all modules
    let mods = generate_mods(&modules);
    fs::write(gen_dir.join("mod.rs"), &mods).unwrap();
}

struct ModuleInfo {
    name: String,
    tests: Vec<TestFnInfo>,
}

struct TestFnInfo {
    name: String,
    is_async: bool,
}

fn transform_source(
    source: &str,
    test_attr: &Regex,
    tokio_attr: &Regex,
    fn_decl: &Regex,
    module_name: &str,
    skip_fns: &std::collections::HashSet<(&str, &str)>,
    tests_dir: &std::path::Path,
) -> (String, Vec<TestFnInfo>) {
    let mut output = String::with_capacity(source.len());
    let mut test_fns = Vec::new();
    let fn_name_re = Regex::new(r"fn\s+(\w+)").unwrap();

    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;
    let mut prev_was_test = false;
    let mut prev_was_tokio_test = false;
    let mut prev_was_cfg_gated = false;
    let mut cfg_test_brace_depth: i32 = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Track braces inside a flattened `mod tests { ... }` block.
        // When the opening brace depth returns to 0, skip the closing `}`.
        if cfg_test_brace_depth > 0 {
            let opens = trimmed.chars().filter(|&c| c == '{').count() as i32;
            let closes = trimmed.chars().filter(|&c| c == '}').count() as i32;
            cfg_test_brace_depth += opens - closes;
            if cfg_test_brace_depth <= 0 {
                // This is the closing `}` of the mod tests block — skip it
                cfg_test_brace_depth = 0;
                i += 1;
                continue;
            }
        }

        // Replace rong_test imports with our prelude
        if trimmed.starts_with("use rong_test::*") || trimmed == "use rong_test::*;" {
            output.push_str("use crate::prelude::*;\n");
            output.push_str("use crate::async_run;\n");
            i += 1;
            continue;
        }
        if trimmed.starts_with("use rong_test::function::") {
            let replaced = line.replace(
                "use rong_test::function::",
                "use crate::prelude::function::",
            );
            output.push_str(&replaced);
            output.push('\n');
            i += 1;
            continue;
        }
        // `use rong::` and `use rong_macro::` are left as-is because
        // both `rong` (with arkjs feature) and `rong_macro` are real
        // crate dependencies.

        // Strip `#[cfg(test)] mod tests { use super::*; ... }` — flatten into
        // the parent module so test functions are accessible at module root.
        if trimmed == "#[cfg(test)]" {
            i += 1;
            // Consume `mod tests {`
            if i < lines.len() && lines[i].trim().starts_with("mod tests") {
                cfg_test_brace_depth += 1;
                i += 1;
            }
            // Consume `use super::*;`
            if i < lines.len() && lines[i].trim() == "use super::*;" {
                i += 1;
            }
            continue;
        }

        // Track #[cfg(feature = "...")] — keep the attribute but mark so the
        // following test function is NOT registered (it will be cfg'd out at
        // compile time).
        if trimmed.starts_with("#[cfg(feature") {
            prev_was_cfg_gated = true;
            output.push_str(line);
            output.push('\n');
            i += 1;
            continue;
        }

        // Strip #[test]
        if test_attr.is_match(trimmed) {
            prev_was_test = true;
            i += 1;
            continue;
        }

        // Strip #[tokio::test]
        if tokio_attr.is_match(trimmed) {
            prev_was_tokio_test = true;
            i += 1;
            continue;
        }

        // If previous line was a test attribute, this should be the fn declaration
        if (prev_was_test || prev_was_tokio_test) && fn_decl.is_match(line) {
            let is_async = prev_was_tokio_test || line.contains("async fn ");
            let skip_registration = prev_was_cfg_gated;
            prev_was_test = false;
            prev_was_tokio_test = false;
            prev_was_cfg_gated = false;

            // Extract function name
            if let Some(cap) = fn_name_re.captures(line) {
                let fn_name = cap[1].to_string();

                // Make it pub and ensure correct return type
                let publine = if line.contains("pub ") {
                    line.to_string()
                } else {
                    fn_decl.replace(line, "${1}pub ${3}fn ").to_string()
                };
                output.push_str(&publine);
                output.push('\n');

                // Don't register cfg-gated or skip-listed functions
                let is_skipped = skip_fns.contains(&(module_name, fn_name.as_str()));
                if is_skipped {
                    eprintln!(
                        "build.rs: skipping {}.{} (in skip list)",
                        module_name, fn_name
                    );
                }
                if !skip_registration && !is_skipped {
                    test_fns.push(TestFnInfo {
                        name: fn_name,
                        is_async,
                    });
                }
            } else {
                output.push_str(line);
                output.push('\n');
            }
            i += 1;
            continue;
        }

        // Reset if we got an attribute but the next line wasn't a fn
        prev_was_test = false;
        prev_was_tokio_test = false;

        output.push_str(&rewrite_fixture_include(line, tests_dir));
        output.push('\n');
        i += 1;
    }

    (output, test_fns)
}

fn rewrite_fixture_include(line: &str, tests_dir: &std::path::Path) -> String {
    let marker = "include_str!(\"";
    let Some(marker_start) = line.find(marker) else {
        return line.to_string();
    };
    let path_start = marker_start + marker.len();
    let Some(path_end) = line[path_start..].find("\")") else {
        return line.to_string();
    };
    let relative = &line[path_start..path_start + path_end];
    if !relative.starts_with("unit/") {
        return line.to_string();
    }

    let fixture = tests_dir.join(relative);
    line.replacen(&format!("\"{relative}\""), &format!("{fixture:?}"), 1)
}

fn generate_mods(modules: &[ModuleInfo]) -> String {
    let mut out = String::new();
    for m in modules {
        // `macro` is a reserved keyword — use raw identifier
        let mod_name = if is_rust_keyword(&m.name) {
            format!("r#{}", m.name)
        } else {
            m.name.clone()
        };
        out.push_str(&format!(
            "#[allow(unused_imports, dead_code, unused_variables, unreachable_code, unexpected_cfgs)]\n\
             #[path = \"{}.rs\"]\n\
             pub mod {};\n\n",
            m.name, mod_name
        ));
    }
    out
}

fn is_rust_keyword(name: &str) -> bool {
    matches!(
        name,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
            | "try"
    )
}

fn generate_registry(modules: &[ModuleInfo]) -> String {
    let mut out = String::new();
    out.push_str("use rong_test_harness::{TestEntry, TestFn};\n\n");
    out.push_str("pub fn all_tests() -> Vec<TestEntry> {\n");
    out.push_str("    vec![\n");

    for m in modules {
        let mod_ident = if is_rust_keyword(&m.name) {
            format!("r#{}", m.name)
        } else {
            m.name.clone()
        };
        for t in &m.tests {
            let full_name = format!("{}.{}", m.name, t.name);
            if t.is_async {
                out.push_str(&format!(
                    "        TestEntry {{ name: \"{full_name}\", run: TestFn::Async(|| Box::pin(async {{ super::generated::{mod_ident}::{func}().await.map_err(|e| e.to_string()) }})) }},\n",
                    func = t.name,
                ));
            } else {
                out.push_str(&format!(
                    "        TestEntry {{ name: \"{full_name}\", run: TestFn::Sync(|| {{ super::generated::{mod_ident}::{func}(); Ok(()) }}) }},\n",
                    func = t.name,
                ));
            }
        }
    }

    out.push_str("    ]\n");
    out.push_str("}\n");
    out
}
