//! Comprehensive integration test suite for `rong::quickjs::jit`.

use rong::quickjs::jit::{self, JitConfig};
use rong::{JSEngine, Source};

fn new_jit_runtime(hot_threshold: u32) -> (rong::quickjs::JSRuntime, rong::quickjs::JSContext) {
    let rt = rong::quickjs::RongJS::runtime();
    let ctx = rt.context();
    let active = jit::init(
        &ctx,
        JitConfig {
            hot_threshold,
            ..JitConfig::default()
        },
    )
    .expect("jit::init should succeed");
    assert_eq!(active, jit::is_supported());
    (rt, ctx)
}

#[test]
fn int32_and_float64_arithmetic_and_overflow() {
    let (_rt, ctx) = new_jit_runtime(1);
    let ok: bool = ctx
        .eval(Source::from_bytes(
            r#"
            function add(a, b) { return a + b; }
            function sub(a, b) { return a - b; }
            function mul(a, b) { return a * b; }
            function div(a, b) { return a / b; }
            function mod(a, b) { return a % b; }
            function neg(a) { return -a; }
            function pow(a, b) { return a ** b; }

            const checks = [
                add(20, 22) === 42,
                add(2147483647, 1) === 2147483648,
                sub(-2147483648, 1) === -2147483649,
                mul(100000, 100000) === 10000000000,
                Object.is(mul(0, -1), -0),
                Object.is(mul(-1, 0), -0),
                Object.is(neg(0), -0),
                neg(-2147483648) === 2147483648,
                neg(42) === -42,
                div(7, 2) === 3.5,
                div(1, 0) === Infinity,
                mod(17, 5) === 2,
                mod(-17, 5) === -2,
                add(1.5, 2.25) === 3.75,
                add(1, 2.5) === 3.5,
                sub(5.5, 2) === 3.5,
                mul(2.5, 4) === 10,
                pow(2, 10) === 1024,
            ];
            checks.every(Boolean);
            "#,
        ))
        .unwrap();
    assert!(ok, "arithmetic checks failed");

    if jit::is_supported() {
        let stats = jit::stats(&ctx).unwrap();
        assert!(stats.compiled_functions >= 7, "stats={stats:?}");
        assert!(stats.jit_calls >= 18, "stats={stats:?}");
    }
}

#[test]
fn bitwise_and_shift_operations() {
    let (_rt, ctx) = new_jit_runtime(1);
    let ok: bool = ctx
        .eval(Source::from_bytes(
            r#"
            function band(a, b) { return a & b; }
            function bor(a, b) { return a | b; }
            function bxor(a, b) { return a ^ b; }
            function bnot(a) { return ~a; }
            function shl(a, b) { return a << b; }
            function sar(a, b) { return a >> b; }
            function shr(a, b) { return a >>> b; }

            [
                band(0b1100, 0b1010) === 0b1000,
                bor(0b1100, 0b1010) === 0b1110,
                bxor(0b1100, 0b1010) === 0b0110,
                bnot(0) === -1,
                bnot(-1) === 0,
                shl(1, 5) === 32,
                sar(-16, 2) === -4,
                shr(-1, 0) === 4294967295,
                shr(16, 2) === 4,
            ].every(Boolean);
            "#,
        ))
        .unwrap();
    assert!(ok);
}

#[test]
fn comparisons_and_nan_semantics() {
    let (_rt, ctx) = new_jit_runtime(1);
    let ok: bool = ctx
        .eval(Source::from_bytes(
            r#"
            function cmp(a, b) {
                return [
                    a < b,
                    a <= b,
                    a > b,
                    a >= b,
                    a == b,
                    a != b,
                    a === b,
                    a !== b,
                ];
            }
            const r1 = cmp(1, 2);
            const r2 = cmp(NaN, 1);
            const r3 = cmp(1.5, 1.5);
            const r4 = cmp("1", 1);

            r1[0] && r1[1] && !r1[2] && !r1[3] && !r1[4] && r1[5] && !r1[6] && r1[7]
                && !r2[0] && !r2[1] && !r2[2] && !r2[3] && !r2[4] && r2[5] && !r2[6] && r2[7]
                && !r3[0] && r3[1] && !r3[2] && r3[3] && r3[4] && !r3[5] && r3[6] && !r3[7]
                && r4[4] && !r4[5] && !r4[6] && r4[7];
            "#,
        ))
        .unwrap();
    assert!(ok);
}

#[test]
fn loops_locals_and_recursion() {
    let (_rt, ctx) = new_jit_runtime(1);
    let res: i32 = ctx
        .eval(Source::from_bytes(
            r#"
            function sumTo(n) {
                let acc = 0;
                for (let i = 1; i <= n; i++) {
                    acc += i;
                }
                return acc;
            }
            function fib(n) {
                if (n <= 1) return n;
                return fib(n - 1) + fib(n - 2);
            }
            sumTo(100) + fib(20);
            "#,
        ))
        .unwrap();
    assert_eq!(res, 5050 + 6765);

    if jit::is_supported() {
        let stats = jit::stats(&ctx).unwrap();
        assert!(stats.compiled_functions >= 2, "stats={stats:?}");
        assert!(stats.jit_calls > 1000, "stats={stats:?}");
    }
}

#[test]
fn arrays_objects_methods_and_closures() {
    let (_rt, ctx) = new_jit_runtime(1);
    let res: i32 = ctx
        .eval(Source::from_bytes(
            r#"
            function sumArray(arr) {
                let s = 0;
                for (let i = 0; i < arr.length; i++) {
                    s += arr[i];
                }
                return s;
            }
            function doubleInPlace(arr) {
                for (let i = 0; i < arr.length; i++) {
                    arr[i] = arr[i] * 2;
                }
                return arr.length;
            }
            const obj = {
                base: 10,
                addBase(x) { return this.base + x; }
            };
            function callMethod(o, v) {
                return o.addBase(v);
            }
            let outer = 5;
            function bumpOuter(delta) {
                outer = outer + delta;
                return outer;
            }

            const a = [1, 2, 3, 4, 5];
            doubleInPlace(a);
            sumArray(a) + callMethod(obj, 7) + bumpOuter(3);
            "#,
        ))
        .unwrap();
    // sumArray([2,4,6,8,10]) = 30; callMethod = 17; bumpOuter = 8 => 55
    assert_eq!(res, 55);
}

#[test]
fn string_slow_path_and_refcount_stress() {
    let (rt, ctx) = new_jit_runtime(1);
    let out: String = ctx
        .eval(Source::from_bytes(
            r#"
            function join(a, b) {
                return a + b;
            }
            let last = "";
            for (let i = 0; i < 200; i++) {
                last = join("item-", i);
            }
            last;
            "#,
        ))
        .unwrap();
    assert_eq!(out, "item-199");
    rt.run_gc();
    drop(ctx);
    drop(rt);
}

#[test]
fn missing_and_extra_arguments_and_implicit_return() {
    let (_rt, ctx) = new_jit_runtime(1);
    let ok: bool = ctx
        .eval(Source::from_bytes(
            r#"
            function pick(a, b) {
                if (b === undefined) return a;
                return a + b;
            }
            function noReturn(x) {
                let y = x + 1;
            }
            pick(10) === 10
                && pick(10, 5) === 15
                && pick(10, 5, 999) === 15
                && noReturn(1) === undefined;
            "#,
        ))
        .unwrap();
    assert!(ok);
}

#[test]
fn exceptions_and_stack_overflow_unwind_cleanly() {
    let (_rt, ctx) = new_jit_runtime(1);
    let ok: bool = ctx
        .eval(Source::from_bytes(
            r#"
            function boom(obj) {
                let tmp = "allocated-" + "string";
                return obj.x + tmp.length;
            }
            function infinite(n) {
                return infinite(n + 1);
            }

            let caughtType = false;
            try { boom(null); } catch (e) { caughtType = e instanceof TypeError; }

            let caughtStack = false;
            try { infinite(0); } catch (e) { caughtStack = e instanceof RangeError; }

            caughtType && caughtStack;
            "#,
        ))
        .unwrap();
    assert!(ok);
}

#[test]
fn unsupported_opcodes_fall_back_to_interpreter() {
    let (_rt, ctx) = new_jit_runtime(1);
    let ok: bool = ctx
        .eval(Source::from_bytes(
            r#"
            function withTryCatch(x) {
                try {
                    if (x < 0) throw new Error("neg");
                    return x * 2;
                } catch (e) {
                    return -1;
                }
            }
            function makeAdder(a) {
                return function(b) { return a + b; };
            }
            const add5 = makeAdder(5);
            withTryCatch(10) === 20 && withTryCatch(-1) === -1 && add5(7) === 12;
            "#,
        ))
        .unwrap();
    assert!(ok);

    if jit::is_supported() {
        let stats = jit::stats(&ctx).unwrap();
        // `withTryCatch` (OP_catch) and `makeAdder` (OP_fclosure / var_ref_count > 0)
        // are rejected and executed by the QuickJS interpreter.
        assert!(stats.rejected_functions >= 2, "stats={stats:?}");
    }
}

#[test]
fn hot_threshold_and_disable_toggle() {
    if !jit::is_supported() {
        return;
    }
    let (_rt, ctx) = new_jit_runtime(5);
    ctx.eval::<()>(Source::from_bytes(
        "globalThis.inc = function(x) { return x + 1; };",
    ))
    .unwrap();

    // 4 calls -> below threshold 5, not yet compiled.
    let v: i32 = ctx
        .eval(Source::from_bytes("inc(1) + inc(2) + inc(3) + inc(4)"))
        .unwrap();
    assert_eq!(v, 14);
    assert_eq!(jit::stats(&ctx).unwrap().compiled_functions, 0);

    // 5th call triggers compilation and executes in JIT.
    let v: i32 = ctx.eval(Source::from_bytes("inc(41)")).unwrap();
    assert_eq!(v, 42);
    let s1 = jit::stats(&ctx).unwrap();
    assert_eq!(s1.compiled_functions, 1);
    assert_eq!(s1.jit_calls, 1);

    // Disable JIT -> subsequent calls run in interpreter.
    assert!(jit::set_enabled(&ctx, false));
    let v: i32 = ctx.eval(Source::from_bytes("inc(99)")).unwrap();
    assert_eq!(v, 100);
    let s2 = jit::stats(&ctx).unwrap();
    assert_eq!(s2.jit_calls, 1);

    // Re-enable JIT -> runs compiled code again without recompiling.
    assert!(jit::set_enabled(&ctx, true));
    let v: i32 = ctx.eval(Source::from_bytes("inc(100)")).unwrap();
    assert_eq!(v, 101);
    let s3 = jit::stats(&ctx).unwrap();
    assert_eq!(s3.compiled_functions, 1);
    assert_eq!(s3.jit_calls, 2);
}

#[test]
fn jit_infinite_loop_is_interruptible() {
    let (rt, ctx) = new_jit_runtime(1);
    let handle = rt.interrupt_handle();

    let cancel = handle.clone();
    let timer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        cancel.interrupt();
    });

    let err = ctx.eval::<()>(Source::from_bytes(
        r#"
        function spin() {
            let i = 0;
            while (true) {
                i++;
            }
        }
        spin();
        "#,
    ));
    timer.join().unwrap();
    assert!(err.is_err(), "infinite JIT loop must be interrupted");
}

#[test]
fn tail_call_functions_compile_without_fallback() {
    if !jit::is_supported() {
        return;
    }
    let (_rt, ctx) = new_jit_runtime(1);
    ctx.eval::<()>(Source::from_bytes(
        r#"
        function helper(x) {
            return x + 1;
        }
        function tailFn(n) {
            return helper(n * 2);
        }
        function tailMethodFn(x) {
            return Math.round(x + 0.25);
        }
        "#,
    ))
    .unwrap();

    let before = jit::stats(&ctx).unwrap();
    let res: i32 = ctx
        .eval(Source::from_bytes("tailFn(10) + tailMethodFn(20.9);"))
        .unwrap();
    assert_eq!(res, 21 + 21);

    let after = jit::stats(&ctx).unwrap();
    assert_eq!(
        after.rejected_functions, before.rejected_functions,
        "after={after:?}, before={before:?}"
    );
    assert_eq!(
        after.compiled_functions - before.compiled_functions,
        4,
        "expected eval + tailFn + helper + tailMethodFn to compile; after={after:?}, before={before:?}"
    );
    assert_eq!(
        after.jit_calls - before.jit_calls,
        4,
        "after={after:?}, before={before:?}"
    );
}

#[test]
fn reentrant_host_callbacks_and_div_jsvalue_identity() {
    use rong::{JSFunc, JSValue};
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let (_rt, ctx) = new_jit_runtime(1);

    // Host callback invoked from inside an active JIT frame that inspects stats
    // and toggles/re-initializes the JIT re-entrantly.
    let probe = JSFunc::new(&ctx, |ctx: rong::quickjs::JSContext, x: i32| {
        let s = jit::stats(&ctx).expect("stats available inside callback");
        if jit::is_supported() {
            assert!(s.jit_calls >= 1);
            assert!(jit::set_enabled(&ctx, false));
            assert!(jit::set_enabled(&ctx, true));
            jit::init(
                &ctx,
                JitConfig {
                    hot_threshold: 1,
                    ..JitConfig::default()
                },
            )?;
        }
        Ok::<i32, rong::RongJSError>(x + 10)
    })
    .unwrap();
    ctx.global().set("hostProbe", probe).unwrap();

    let res: i32 = ctx
        .eval(Source::from_bytes(
            r#"
            function outerJit(a) {
                return hostProbe(a * 2) + 1;
            }
            outerJit(15) + outerJit(20);
            "#,
        ))
        .unwrap();
    assert_eq!(res, 41 + 51);

    // Exact integer division in JIT must produce `JS_TAG_INT` matching QuickJS `js_number`,
    // preserving Rust `JSValue` PartialEq and Hash identity.
    ctx.eval::<()>(Source::from_bytes(
        "function jitDiv(a, b) { return a / b; } function jitFDiv(a, b) { return (a + 0.5 - 0.5) / b; }",
    ))
    .unwrap();
    let v_lit: JSValue = ctx.eval(Source::from_bytes("2")).unwrap();
    let v_idiv: JSValue = ctx.eval(Source::from_bytes("jitDiv(4, 2)")).unwrap();
    let v_fdiv: JSValue = ctx.eval(Source::from_bytes("jitFDiv(9.0, 3.0)")).unwrap();
    let lit_three: JSValue = ctx.eval(Source::from_bytes("3")).unwrap();
    assert_eq!(v_idiv, v_lit);
    assert_eq!(v_fdiv, lit_three);

    let hash_of = |v: &JSValue| {
        let mut h = DefaultHasher::new();
        v.hash(&mut h);
        h.finish()
    };
    assert_eq!(hash_of(&v_idiv), hash_of(&v_lit));
    assert_eq!(hash_of(&v_fdiv), hash_of(&lit_three));

    let neg_zero_ok: bool = ctx
        .eval(Source::from_bytes(
            "Object.is(jitDiv(0, -1), -0) && jitDiv(-2147483648, 1) === -2147483648 && jitDiv(-2147483648, -1) === 2147483648",
        ))
        .unwrap();
    assert!(neg_zero_ok);
}

#[test]
fn large_frames_use_bulk_helpers_and_oversized_frames_fall_back() {
    let (_rt, ctx) = new_jit_runtime(1);

    // 40 locals (>16 threshold for QJS_JitInitUndefined & >8 threshold for QJS_JitFreeSlots)
    // with multiple return paths and heap-allocated strings.
    let mut medium_vars = String::new();
    for i in 0..40 {
        medium_vars.push_str(&format!("let v{i} = 's-' + (x + {i});\n"));
    }
    let script = format!(
        r#"
        function mediumFrame(x) {{
            {medium_vars}
            if (x < 0) return v0.length;
            if (x === 0) return v19.length;
            return v39.length;
        }}
        mediumFrame(-1) + mediumFrame(0) + mediumFrame(100);
        "#
    );
    let out: i32 = ctx.eval(Source::from_bytes(script.as_bytes())).unwrap();
    assert_eq!(out, 4 + 4 + 5);

    // >512 locals exceeds MAX_FRAME_SLOTS and must cleanly fall back to interpreter.
    let mut huge_vars = String::new();
    for i in 0..530 {
        huge_vars.push_str(&format!("let h{i} = {i};\n"));
    }
    let before = jit::stats(&ctx);
    let huge_script = format!(
        r#"
        function oversizedFrame(x) {{
            {huge_vars}
            return x + h0 + h529;
        }}
        oversizedFrame(10);
        "#
    );
    let huge_out: i32 = ctx
        .eval(Source::from_bytes(huge_script.as_bytes()))
        .unwrap();
    assert_eq!(huge_out, 539);
    if jit::is_supported() {
        let after = jit::stats(&ctx).unwrap();
        assert!(after.rejected_functions > before.unwrap().rejected_functions);
    }
}
