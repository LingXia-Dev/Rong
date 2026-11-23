//! Runs test262's `intl402` tests when `TEST262_DIR` names a checkout.
//! `TEST262_PATHS` (comma-separated, relative to the checkout) picks the
//! directories or files; every failure is listed with its reason. The run
//! fails only when `TEST262_REQUIRE_PASS` is set, so pass rates can be
//! reported while a constructor is still being built.

use rong_test::*;
use std::io::Write;
use std::path::{Path, PathBuf};

const BLOB: &[u8] = include_bytes!("data/intl-test.postcard");
const DEFAULT_PATHS: &str =
    "test/intl402/Intl/getCanonicalLocales,test/intl402/Intl/builtin.js,test/intl402/PluralRules";

/// The host-defined `$262` the tests may use, as far as rong can give it.
const HOST: &str = r#"
var $262 = {
  global: globalThis,
  evalScript(source) { return (0, eval)(source); },
  createRealm() { throw new Test262Error('$262.createRealm is not available'); },
  detachArrayBuffer() { throw new Test262Error('$262.detachArrayBuffer is not available'); },
  gc() {},
  agent: {},
};
"#;

struct Meta {
    includes: Vec<String>,
    flags: Vec<String>,
    negative: Option<String>,
}

fn list(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_meta(source: &str) -> Meta {
    let mut meta = Meta {
        includes: Vec::new(),
        flags: Vec::new(),
        negative: None,
    };
    let Some(start) = source.find("/*---") else {
        return meta;
    };
    let Some(end) = source[start..].find("---*/") else {
        return meta;
    };
    let mut section = "";
    for line in source[start + 5..start + end].lines() {
        let trimmed = line.trim();
        if let Some(item) = trimmed.strip_prefix("- ") {
            if section == "includes" {
                meta.includes.push(item.trim().to_string());
            } else if section == "flags" {
                meta.flags.push(item.trim().to_string());
            }
            continue;
        }
        if !line.starts_with(' ')
            && let Some((key, value)) = trimmed.split_once(':')
        {
            section = key;
            match key {
                "includes" => meta.includes.extend(list(value)),
                "flags" => meta.flags.extend(list(value)),
                _ => {}
            }
        } else if section == "negative"
            && let Some(kind) = trimmed.strip_prefix("type:")
        {
            meta.negative = Some(kind.trim().to_string());
        }
    }
    meta
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(path)
            .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).collect())
            .unwrap_or_default();
        entries.sort();
        for entry in entries {
            collect(&entry, out);
        }
    } else if path.extension().is_some_and(|e| e == "js")
        && !path.to_string_lossy().contains("_FIXTURE")
    {
        out.push(path.to_path_buf());
    }
}

/// `Ok(())` on a pass, or why it failed.
fn run_one(rt: &JSRuntime, root: &Path, file: &Path) -> Result<(), String> {
    let source = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    let meta = parse_meta(&source);
    if meta.flags.iter().any(|f| f == "async" || f == "module") {
        return Err(String::from(
            "skipped: async and module tests are not run yet",
        ));
    }
    let ctx = rt.context();
    install_intl(&ctx)?;
    let mut prelude = String::new();
    for include in ["assert.js", "sta.js"]
        .iter()
        .map(|s| s.to_string())
        .chain(meta.includes.clone())
    {
        let text = std::fs::read_to_string(root.join("harness").join(&include))
            .map_err(|e| format!("harness {include}: {e}"))?;
        prelude.push_str(&text);
        prelude.push('\n');
    }
    prelude.push_str(HOST);
    ctx.eval::<()>(Source::from_bytes(prelude))
        .map_err(|e| format!("harness failed: {e}"))?;
    let strict = meta.flags.iter().any(|f| f == "onlyStrict");
    let body = if strict {
        format!("'use strict';\n{source}")
    } else {
        source
    };
    let outcome = ctx.eval::<()>(Source::from_bytes(body));
    match (outcome, &meta.negative) {
        (Ok(()), None) => Ok(()),
        (Ok(()), Some(kind)) => Err(format!("expected {kind}, nothing was thrown")),
        (Err(e), negative) => {
            let name = thrown_object_prop::<String>(&ctx, &e, "name").ok();
            let text = thrown_error_message(&ctx, &e).unwrap_or_else(|_| e.to_string());
            match (negative, name) {
                (Some(kind), Some(name)) if *kind == name => Ok(()),
                _ => Err(format!(
                    "{} {}",
                    thrown_object_prop::<String>(&ctx, &e, "name").unwrap_or_default(),
                    text
                )),
            }
        }
    }
}

#[test]
fn intl402() -> Result<(), String> {
    let Some(root) = std::env::var_os("TEST262_DIR").map(PathBuf::from) else {
        return Ok(());
    };
    let paths = std::env::var("TEST262_PATHS").unwrap_or_else(|_| DEFAULT_PATHS.to_string());
    let rt = RongJS::runtime();
    let mut out = std::io::stderr();
    let mut failed_total = 0;
    for path in paths.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let mut files = Vec::new();
        collect(&root.join(path), &mut files);
        let mut failures = Vec::new();
        for file in &files {
            if let Err(reason) = run_one(&rt, &root, file) {
                let name = file
                    .strip_prefix(&root)
                    .unwrap_or(file)
                    .display()
                    .to_string();
                failures.push((name, reason));
            }
        }
        let passed = files.len() - failures.len();
        let _ = writeln!(
            out,
            "[test262] {} | {path}: {passed}/{} passed",
            RongJS::name(),
            files.len()
        );
        for (name, reason) in &failures {
            let _ = writeln!(out, "[test262] FAIL {name}: {reason}");
        }
        failed_total += failures.len();
    }
    if failed_total > 0 && std::env::var_os("TEST262_REQUIRE_PASS").is_some() {
        return Err(format!("{failed_total} test262 tests failed"));
    }
    Ok(())
}

/// QuickJS gets rong's `Intl`; an engine with its own (the reference) is left
/// as it is.
fn install_intl(ctx: &JSContext) -> Result<(), String> {
    let native: bool = ctx
        .eval(Source::from_bytes("typeof globalThis.Intl !== 'undefined'"))
        .map_err(|e| e.to_string())?;
    if native {
        return Ok(());
    }
    let data =
        rong_quickjs_intl::IntlData::try_from_static_blob(BLOB).map_err(|e| e.to_string())?;
    rong_quickjs_intl::configure(ctx, data, rong_quickjs_intl::IntlSettings::default());
    rong_quickjs_intl::init::<RongJS>(ctx).map_err(|e| e.to_string())
}
