use rong_test::*;

// Primary engine configurations where `JSRuntime::memory_usage` is backed by a
// native heap-statistics API:
// - QuickJS (all targets)
// - HarmonyOS ArkJS (`target_env = "ohos"`)
// - JavaScriptCore source/JSCOnly builds (`jscore-source`, or `jscore` on
//   non-Apple targets)
#[cfg(any(
    feature = "primary-quickjs",
    all(feature = "quickjs", not(feature = "jscore"), not(feature = "arkjs")),
    all(
        target_env = "ohos",
        any(
            feature = "primary-arkjs",
            all(feature = "arkjs", not(feature = "jscore"))
        )
    ),
    all(
        any(
            feature = "primary-jscore",
            all(
                feature = "jscore",
                not(feature = "primary-quickjs"),
                not(feature = "primary-arkjs")
            )
        ),
        any(
            feature = "jscore-source",
            not(any(target_os = "macos", target_os = "ios"))
        )
    )
))]
#[test]
fn configured_engine_reports_memory_usage() {
    let runtime = RongJS::runtime();
    let bare = runtime
        .memory_usage()
        .expect("configured engine backend must report memory statistics");
    let bare_allocated = bare
        .allocated_bytes
        .expect("configured engine backend must report allocated_bytes");
    assert!(
        bare_allocated > 0,
        "expected positive allocated_bytes on new runtime: {bare:?}"
    );

    let ctx = runtime.context();
    ctx.eval::<()>(Source::from_bytes(
        "globalThis.__rongInit = Array.from({ length: 256 }, (_, i) => ({ i }));",
    ))
    .expect("context evaluation must succeed");
    let with_ctx = runtime
        .memory_usage()
        .expect("configured engine backend must report memory statistics with context");
    let with_ctx_allocated = with_ctx
        .allocated_bytes
        .expect("configured engine backend must report allocated_bytes with context");
    assert!(
        with_ctx.used_bytes > bare.used_bytes || with_ctx_allocated >= bare_allocated,
        "context memory usage must remain valid: bare={bare:?}, with_ctx={with_ctx:?}"
    );
}

// Apple's system JavaScriptCore.framework exposes no public heap-statistics C
// API, so default system-framework builds return `None` rather than linking
// private SPIs.
#[cfg(all(
    any(target_os = "macos", target_os = "ios"),
    any(
        feature = "primary-jscore",
        all(
            feature = "jscore",
            not(feature = "primary-quickjs"),
            not(feature = "primary-arkjs")
        )
    ),
    not(feature = "jscore-source")
))]
#[test]
fn apple_system_jscore_reports_none_for_memory_usage() {
    let runtime = RongJS::runtime();
    assert_eq!(runtime.memory_usage(), None);
}

#[test]
fn memory_usage_grows_with_live_allocations() {
    let runtime = RongJS::runtime();
    let ctx = runtime.context();
    let Some(before) = runtime.memory_usage() else {
        return;
    };

    ctx.eval::<()>(Source::from_bytes(
        r#"
        const items = [];
        for (let i = 0; i < 4000; i++) {
            items.push({ index: i, label: "entry-" + i + "-" + "x".repeat(64) });
        }
        const buf = new ArrayBuffer(1024 * 1024);
        new Uint8Array(buf).fill(0x5a);
        globalThis.__rongMemHold = { items, buf };
        "#,
    ))
    .expect("allocation script must succeed");

    let after = runtime
        .memory_usage()
        .expect("engine that reported before allocation must report after");

    assert!(
        after.used_bytes >= before.used_bytes + 256 * 1024,
        "expected used_bytes to grow substantially after live heap allocations: before={before:?}, after={after:?}"
    );

    if let (Some(before_objs), Some(after_objs)) = (before.object_count, after.object_count) {
        assert!(
            after_objs >= before_objs + 4000,
            "expected object_count to grow by at least 4000: before={before:?}, after={after:?}"
        );
    }
}

// QuickJS and HarmonyOS ArkJS perform synchronous full collection in `run_gc()`.
#[cfg(any(
    feature = "primary-quickjs",
    all(feature = "quickjs", not(feature = "jscore"), not(feature = "arkjs")),
    all(
        target_env = "ohos",
        any(
            feature = "primary-arkjs",
            all(feature = "arkjs", not(feature = "jscore"))
        )
    )
))]
#[test]
fn memory_usage_decreases_after_gc_reclaims_unreachable_heap() {
    let runtime = RongJS::runtime();
    let ctx = runtime.context();

    ctx.eval::<()>(Source::from_bytes(
        r#"
        globalThis.__rongMemTransient = [];
        for (let i = 0; i < 8000; i++) {
            globalThis.__rongMemTransient.push({ index: i, payload: "x".repeat(128) });
        }
        "#,
    ))
    .expect("transient allocation must succeed");
    let peak = runtime
        .memory_usage()
        .expect("memory_usage must be available");

    ctx.eval::<()>(Source::from_bytes("delete globalThis.__rongMemTransient;"))
        .expect("releasing global reference must succeed");
    runtime.run_gc();

    let after_gc = runtime
        .memory_usage()
        .expect("memory_usage must remain available after GC");
    assert!(
        after_gc.used_bytes < peak.used_bytes,
        "expected run_gc() to reclaim unreachable objects: peak={peak:?}, after_gc={after_gc:?}"
    );
}

#[test]
fn shared_and_pinned_worker_pools_report_memory_usage() {
    let shared = Rong::<RongJS>::builder()
        .shared()
        .workers(2)
        .build()
        .expect("shared pool must start");
    let pinned = Rong::<RongJS>::builder()
        .pinned::<u64, usize>()
        .workers(2)
        .build()
        .expect("pinned pool must start");

    let expected_some = RongJS::runtime().memory_usage().is_some();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime must start");

    rt.block_on(async {
        let shared_stats = shared
            .memory_usage()
            .await
            .expect("shared pool memory_usage must succeed");
        assert_eq!(shared_stats.len(), 2);
        for entry in &shared_stats {
            assert_eq!(entry.is_some(), expected_some);
            if let Some(usage) = entry {
                assert!(usage.allocated_bytes.unwrap_or(0) > 0);
            }
        }

        let worker0_shared = shared.worker(0).expect("shared worker 0 must exist");
        let idle_usage = worker0_shared
            .memory_usage()
            .await
            .expect("shared worker idle memory_usage must succeed");
        worker0_shared
            .call(|runtime, _rx| async move {
                let ctx = runtime.context();
                ctx.eval::<i32>(Source::from_bytes("20 + 22"))
            })
            .await
            .expect("shared worker eval must succeed");
        if let (Some(idle), Some(active)) = (
            idle_usage,
            worker0_shared
                .memory_usage()
                .await
                .expect("shared worker memory_usage must succeed"),
        ) {
            assert!(active.allocated_bytes.unwrap_or(0) >= idle.allocated_bytes.unwrap_or(0));
        }

        // Seed state on a pinned key and verify sampling memory usage neither
        // requires a key nor disturbs existing per-key state.
        let count = pinned
            .call(42, |runtime, _key, state, _rx| async move {
                let ctx = runtime.context();
                match ctx.eval::<usize>(Source::from_bytes("3")) {
                    Ok(add) => {
                        let next = state.unwrap_or(0) + add;
                        (Ok::<usize, RongJSError>(next), Some(next))
                    }
                    Err(err) => (Err(err), state),
                }
            })
            .await
            .expect("pinned call must succeed");
        assert_eq!(count, 3);

        let pinned_stats = pinned
            .memory_usage()
            .await
            .expect("pinned pool memory_usage must succeed");
        assert_eq!(pinned_stats.len(), 2);
        for entry in &pinned_stats {
            assert_eq!(entry.is_some(), expected_some);
            if let Some(usage) = entry {
                assert!(usage.allocated_bytes.unwrap_or(0) > 0);
            }
        }

        let key_worker = pinned.worker_for_key(&42);
        if let Some(key_worker_stats) = key_worker
            .memory_usage()
            .await
            .expect("pinned worker memory_usage must succeed")
        {
            assert!(key_worker_stats.allocated_bytes.unwrap_or(0) > 0);
        }

        let count_after = pinned
            .call(42, |_runtime, _key, state, _rx| async move {
                let current = state.unwrap_or(0);
                (Ok(current), Some(current))
            })
            .await
            .expect("pinned state must remain intact after memory_usage");
        assert_eq!(count_after, 3);
    });
}

#[test]
fn memory_usage_works_reentrantly_inside_host_function_and_while_interrupted() {
    let runtime = RongJS::runtime();
    let ctx = runtime.context();
    let expected_some = runtime.memory_usage().is_some();

    ctx.global()
        .set(
            "__sampleAllocated",
            JSFunc::new(&ctx, |ctx: JSContext| -> JSResult<f64> {
                Ok(ctx
                    .runtime()
                    .memory_usage()
                    .and_then(|u| u.allocated_bytes)
                    .map(|b| b as f64)
                    .unwrap_or(-1.0))
            })
            .expect("host function must build"),
        )
        .expect("global function must install");

    let sampled: f64 = ctx
        .eval(Source::from_bytes("__sampleAllocated()"))
        .expect("re-entrant memory_usage call from JS must succeed");
    if expected_some {
        assert!(
            sampled > 0.0,
            "expected positive allocated_bytes inside host callback, got {sampled}"
        );
    } else {
        assert_eq!(sampled, -1.0);
    }

    // Even while an interruption request is active (rejecting new JS evals),
    // host-side `runtime.memory_usage()` must still succeed cleanly.
    let interrupt = runtime.interrupt_handle();
    interrupt.interrupt();
    assert!(ctx.eval::<i32>(Source::from_bytes("1 + 1")).is_err());
    assert_eq!(runtime.memory_usage().is_some(), expected_some);
    interrupt.clear();
}
