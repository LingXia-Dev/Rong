//! Benchmark comparing QuickJS interpreter (JIT OFF) vs Cranelift baseline JIT (JIT ON).
//!
//! Run in release mode for realistic numbers:
//! ```sh
//! cargo run --release -p rong_quickjs_jit --example bench
//! ```

use rong::quickjs::jit::{self, JitConfig};
use rong::{JSEngine, Source};
use std::time::{Duration, Instant};

struct BenchCase {
    name: &'static str,
    setup: &'static str,
    call: &'static str,
}

const CASES: &[BenchCase] = &[
    BenchCase {
        name: "int_loop (5M iters)",
        setup: r#"
            function int_loop(n) {
                let acc = 0;
                for (let i = 1; i <= n; i++) {
                    acc += i;
                }
                return acc;
            }
        "#,
        call: "int_loop(5000000)",
    },
    BenchCase {
        name: "xorshift32 (5M iters)",
        setup: r#"
            function xorshift(n) {
                let x = 2463534242 | 0;
                for (let i = 0; i < n; i++) {
                    x ^= x << 13;
                    x ^= x >>> 17;
                    x ^= x << 5;
                }
                return x >>> 0;
            }
        "#,
        call: "xorshift(5000000)",
    },
    BenchCase {
        name: "f64_dot (5M iters)",
        setup: r#"
            function f64_dot(n) {
                let a = 1.0000001;
                let b = 0.9999999;
                let acc = 0.0;
                for (let i = 0; i < n; i++) {
                    acc = acc + a * b;
                }
                return Math.round(acc);
            }
        "#,
        call: "f64_dot(5000000)",
    },
    BenchCase {
        name: "fib(30) recursive",
        setup: r#"
            function fib(n) {
                if (n <= 1) return n;
                return fib(n - 1) + fib(n - 2);
            }
        "#,
        call: "fib(30)",
    },
    BenchCase {
        name: "gcd_loop (2M iters)",
        setup: r#"
            function gcd(a, b) {
                while (b !== 0) {
                    let t = a % b;
                    a = b;
                    b = t;
                }
                return a;
            }
            function gcd_loop(n) {
                let s = 0;
                for (let i = 1; i <= n; i++) {
                    s += gcd(i + 12345, i * 3 + 7);
                }
                return s;
            }
        "#,
        call: "gcd_loop(2000000)",
    },
    BenchCase {
        name: "array_sum (100k x 20)",
        setup: r#"
            const arr = new Array(100000);
            for (let i = 0; i < arr.length; i++) arr[i] = i & 255;
            function array_sum(a) {
                let s = 0;
                const len = a.length;
                for (let i = 0; i < len; i++) {
                    s += a[i];
                }
                return s;
            }
            function run_array_sum(rounds) {
                let total = 0;
                for (let r = 0; r < rounds; r++) {
                    total += array_sum(arr);
                }
                return total;
            }
        "#,
        call: "run_array_sum(20)",
    },
];

fn run_case(
    case: &BenchCase,
    enable_jit: bool,
    runs: usize,
) -> (Duration, f64, Option<jit::JitStats>) {
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
        .expect("jit::init failed");
    }

    ctx.eval::<()>(Source::from_bytes(case.setup))
        .expect("setup failed");

    // Warmup (ensures JIT compilation completes before timing).
    let warmup: f64 = ctx
        .eval(Source::from_bytes(case.call))
        .expect("warmup failed");

    let mut best = Duration::MAX;
    let mut last = warmup;
    for _ in 0..runs {
        let start = Instant::now();
        last = ctx.eval(Source::from_bytes(case.call)).expect("run failed");
        let elapsed = start.elapsed();
        if elapsed < best {
            best = elapsed;
        }
    }

    let stats = jit::stats(&ctx);
    (best, last, stats)
}

fn main() {
    println!("QuickJS JIT Benchmark (best of 3 runs, after warmup)");
    println!("Target JIT supported: {}\n", jit::is_supported());
    println!(
        "{:<24} {:>12} {:>12} {:>10} {:>10}",
        "Workload", "JIT OFF", "JIT ON", "Speedup", "Compiled"
    );
    println!("{}", "-".repeat(72));

    for case in CASES {
        let (off_dur, off_val, _) = run_case(case, false, 3);
        let (on_dur, on_val, stats) = run_case(case, true, 3);

        assert_eq!(
            off_val, on_val,
            "Mismatch in {}: off={off_val}, on={on_val}",
            case.name
        );

        let off_ms = off_dur.as_secs_f64() * 1000.0;
        let on_ms = on_dur.as_secs_f64() * 1000.0;
        let speedup = off_ms / on_ms;
        let compiled = stats.map(|s| s.compiled_functions).unwrap_or(0);

        println!(
            "{:<24} {:>9.2} ms {:>9.2} ms {:>9.2}x {:>10}",
            case.name, off_ms, on_ms, speedup, compiled
        );
    }
}
