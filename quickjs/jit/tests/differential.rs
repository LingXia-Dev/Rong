//! Differential (JIT ON vs JIT OFF) and production-hardening test suite.
//!
//! Every differential case runs the exact same JavaScript program once in the
//! pure QuickJS interpreter (JIT disabled) and once with the Cranelift JIT
//! (`hot_threshold = 1`), asserting identical serialized outputs and exception
//! behavior.

use rong::quickjs::jit::{self, JitConfig};
use rong::{JSEngine, Source};

fn eval_with_jit(script: &str, enable_jit: bool) -> (String, Option<jit::JitStats>) {
    let rt = rong::quickjs::RongJS::runtime();
    let ctx = rt.context();
    if enable_jit {
        jit::init(
            &ctx,
            JitConfig {
                hot_threshold: 1,
                ..JitConfig::default()
            },
        )
        .expect("jit::init should succeed");
    }
    let out: String = ctx
        .eval(Source::from_bytes(script))
        .expect("script should evaluate without uncaught exception");
    let stats = jit::stats(&ctx);
    (out, stats)
}

fn assert_jit_matches_interpreter(name: &str, script: &str) {
    let (interp_out, _) = eval_with_jit(script, false);
    let (jit_out, stats) = eval_with_jit(script, true);
    assert_eq!(
        jit_out, interp_out,
        "JIT diverged from interpreter in `{name}`"
    );
    if jit::is_supported() {
        let stats = stats.expect("stats must exist when JIT is enabled");
        assert!(
            stats.compiled_functions >= 1 && stats.jit_calls >= 1,
            "expected `{name}` to actually execute JIT code, got {stats:?}"
        );
    }
}

#[test]
fn differential_numeric_boundaries_and_special_values() {
    assert_jit_matches_interpreter(
        "numeric_boundaries",
        r#"
        function fmt(v) {
            if (Object.is(v, -0)) return "-0";
            if (Number.isNaN(v)) return "NaN";
            return String(v);
        }
        function opAdd(a, b) { return a + b; }
        function opSub(a, b) { return a - b; }
        function opMul(a, b) { return a * b; }
        function opDiv(a, b) { return a / b; }
        function opMod(a, b) { return a % b; }
        function opNeg(a) { return -a; }
        function opPlus(a) { return +a; }
        function opNot(a) { return ~a; }
        function opAnd(a, b) { return a & b; }
        function opOr(a, b) { return a | b; }
        function opXor(a, b) { return a ^ b; }
        function opShl(a, b) { return a << b; }
        function opSar(a, b) { return a >> b; }
        function opShr(a, b) { return a >>> b; }
        function opLt(a, b) { return a < b; }
        function opLte(a, b) { return a <= b; }
        function opGt(a, b) { return a > b; }
        function opGte(a, b) { return a >= b; }
        function opEq(a, b) { return a == b; }
        function opNeq(a, b) { return a != b; }
        function opSeq(a, b) { return a === b; }
        function opSneq(a, b) { return a !== b; }
        function opLnot(a) { return !a; }

        const pairs = [
            [0, 0],
            [0, -1],
            [-1, 0],
            [2147483647, 1],
            [-2147483648, 1],
            [-2147483648, -1],
            [100000, 100000],
            [1.5, -2.5],
            [NaN, 1],
            [NaN, NaN],
            [Infinity, -Infinity],
            [Infinity, 0],
            [17, -5],
            [-17, -5],
            [null, undefined],
            ["42", 42],
            [true, false],
        ];
        pairs.map(([a, b]) => [
            fmt(opAdd(a, b)), fmt(opSub(a, b)), fmt(opMul(a, b)),
            fmt(opDiv(a, b)), fmt(opMod(a, b)), fmt(opNeg(a)),
            fmt(opPlus(a)), fmt(opNot(a)), fmt(opAnd(a, b)),
            fmt(opOr(a, b)), fmt(opXor(a, b)), fmt(opShl(a, b)),
            fmt(opSar(a, b)), fmt(opShr(a, b)), fmt(opLt(a, b)),
            fmt(opLte(a, b)), fmt(opGt(a, b)), fmt(opGte(a, b)),
            fmt(opEq(a, b)), fmt(opNeq(a, b)), fmt(opSeq(a, b)),
            fmt(opSneq(a, b)), fmt(opLnot(a)),
        ].join(",")).join("|");
        "#,
    );
}

#[test]
fn differential_short_circuit_ternary_and_control_flow() {
    assert_jit_matches_interpreter(
        "control_flow",
        r#"
        function classify(n) {
            let acc = 0;
            for (let i = 0; i < n; i++) {
                if (i % 7 === 0) continue;
                if (i > 25) break;
                acc += (i % 2 === 0) ? i : -i;
            }
            const a = (acc > 0 && n > 5) ? acc : (acc || 99);
            const b = (n === 0 ? null : n) ?? 777;
            return a * 1000 + b;
        }
        function doWhileTest(limit) {
            let x = 0;
            let i = 0;
            do {
                x += ++i;
            } while (i < limit);
            return x;
        }
        [0, 1, 5, 10, 30].map(n => classify(n) + ":" + doWhileTest(n)).join("|");
        "#,
    );
}

#[test]
fn differential_valueof_coercion_and_exceptions() {
    assert_jit_matches_interpreter(
        "valueof_coercion",
        r#"
        function addPair(a, b) { return a + b; }
        function subPair(a, b) { return a - b; }
        function ltPair(a, b) { return a < b; }
        function postIncVal(x) {
            let v = x;
            let old = v++;
            return old + ":" + v;
        }
        const good = { valueOf() { return 10; } };
        const strObj = { toString() { return "hi"; } };
        const bad = { valueOf() { throw new RangeError("bad valueOf"); } };

        const res = [
            addPair(good, 5),
            addPair(strObj, "!"),
            subPair(good, 3),
            ltPair(good, 20),
            postIncVal(2147483647),
            postIncVal("9"),
        ];
        for (const fn of [addPair, subPair, ltPair]) {
            try {
                fn(bad, 1);
                res.push("no-throw");
            } catch (e) {
                res.push(e.name + ":" + e.message);
            }
        }
        res.join("|");
        "#,
    );
}

#[test]
fn differential_getters_setters_and_side_effects() {
    assert_jit_matches_interpreter(
        "getters_setters",
        r#"
        function step(o, arr, idx) {
            const v = o.x + arr[idx];
            o.x = v;
            arr[idx] = v + 1;
            return o.x + arr.length;
        }
        let log = [];
        let backing = 3;
        const o = {
            get x() { log.push("g:" + backing); return backing; },
            set x(v) { log.push("s:" + v); backing = v * 2; },
        };
        const arr = [10, 20, 30];
        const r1 = step(o, arr, 1);
        const r2 = step(o, arr, 2);
        [r1, r2, backing, arr.join(","), log.join(",")].join("|");
        "#,
    );
}

#[test]
fn differential_tdz_reference_error_in_jit() {
    assert_jit_matches_interpreter(
        "tdz_reference_error",
        r#"
        function hitTdz(flag) {
            if (flag) {
                return x;
            }
            let x = 42;
            return x;
        }
        const ok = hitTdz(false);
        let err = "none";
        try {
            hitTdz(true);
        } catch (e) {
            err = e.name;
        }
        ok + "|" + err;
        "#,
    );
}

#[test]
fn differential_mutual_recursion_this_and_builtins() {
    assert_jit_matches_interpreter(
        "mutual_recursion_and_builtins",
        r#"
        function isEven(n) {
            if (n === 0) return true;
            return isOdd(n - 1);
        }
        function isOdd(n) {
            if (n === 0) return false;
            return isEven(n - 1);
        }
        function scaled(x) {
            return this.factor * Math.floor(Math.sqrt(x));
        }
        function double(x) { return x * 2; }
        function sum(acc, x) { return acc + x; }

        const mapped = [1, 2, 3, 4, 5].map(double).reduce(sum, 0);
        const s1 = scaled.call({ factor: 3 }, 20);
        const s2 = scaled.apply({ factor: 5 }, [100]);
        [isEven(20), isOdd(20), isEven(19), s1, s2, mapped].join("|");
        "#,
    );
}

#[test]
fn max_bytecode_bytes_and_reinit_config() {
    if !jit::is_supported() {
        return;
    }
    let rt = rong::quickjs::RongJS::runtime();
    let ctx = rt.context();

    // Reject everything by setting max_bytecode_bytes = 1.
    jit::init(
        &ctx,
        JitConfig {
            hot_threshold: 1,
            max_bytecode_bytes: 1,
        },
    )
    .unwrap();

    let v: i32 = ctx
        .eval(Source::from_bytes(
            "function f1(x) { return x + 1; } f1(41);",
        ))
        .unwrap();
    assert_eq!(v, 42);
    assert_eq!(jit::stats(&ctx).unwrap().compiled_functions, 0);
    assert!(jit::stats(&ctx).unwrap().rejected_functions >= 1);

    // Re-init with normal bytecode limit; a newly defined function should now compile.
    jit::init(
        &ctx,
        JitConfig {
            hot_threshold: 1,
            max_bytecode_bytes: 16 * 1024,
        },
    )
    .unwrap();
    let v2: i32 = ctx
        .eval(Source::from_bytes(
            "function f2(x) { return x * 2; } f2(21);",
        ))
        .unwrap();
    assert_eq!(v2, 42);
    assert_eq!(jit::stats(&ctx).unwrap().compiled_functions, 1);
}

#[test]
fn multithreaded_concurrent_jit_runtimes_and_gc_stress() {
    let mut handles = Vec::new();
    for tid in 0..4 {
        handles.push(std::thread::spawn(move || {
            for round in 0..5 {
                let rt = rong::quickjs::RongJS::runtime();
                let ctx = rt.context();
                jit::init(
                    &ctx,
                    JitConfig {
                        hot_threshold: 1,
                        ..JitConfig::default()
                    },
                )
                .unwrap();
                let res: i32 = ctx
                    .eval(Source::from_bytes(
                        r#"
                        function work(seed) {
                            let acc = 0;
                            let s = "p";
                            for (let i = 1; i <= 100; i++) {
                                acc += (i ^ seed);
                                if (i % 25 === 0) s = s + i;
                            }
                            return acc + s.length;
                        }
                        work(17) + work(31);
                        "#,
                    ))
                    .unwrap();
                assert_eq!(res, 10288, "thread {tid} round {round}");
                rt.run_gc();
            }
        }));
    }
    for h in handles {
        h.join().expect("worker thread panicked");
    }
}
