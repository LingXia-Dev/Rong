#![cfg(feature = "quickjs-jit")]
//! The `quickjs-jit` feature gives a QuickJS runtime an optional Cranelift
//! baseline JIT through `rong::quickjs::jit`.

use rong::JSEngine;
use rong::Source;
use rong::quickjs::{RongJS, jit};

#[test]
fn quickjs_jit_feature_compiles_and_runs_hot_functions() {
    let rt = RongJS::runtime();
    let ctx = rt.context();

    assert!(jit::stats(&ctx).is_none());

    let active = jit::init(
        &ctx,
        jit::JitConfig {
            hot_threshold: 2,
            ..jit::JitConfig::default()
        },
    )
    .expect("jit::init should succeed");
    assert_eq!(active, jit::is_supported());

    let res: i32 = ctx
        .eval(Source::from_bytes(
            r#"
            function fib(n) {
                if (n <= 1) return n;
                return fib(n - 1) + fib(n - 2);
            }
            fib(15);
            "#,
        ))
        .unwrap();
    assert_eq!(res, 610);

    let stats = jit::stats(&ctx).expect("stats should be available after init");
    assert_eq!(stats.supported, jit::is_supported());
    if jit::is_supported() {
        assert!(stats.compiled_functions >= 1);
        assert!(stats.jit_calls > 100);
    }
}
