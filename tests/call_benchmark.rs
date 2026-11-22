//! Not asserted: logs the cost of the hot embedder shape, a Rust loop that
//! calls a JS function and reads its result.

use rong_test::*;
use std::time::Instant;

const CALLS: usize = 100_000;

fn log_line(line: &str) {
    #[cfg(target_env = "ohos")]
    log::info!("[call_benchmark] {line}");
    #[cfg(not(target_env = "ohos"))]
    println!("[call_benchmark] {line}");
}

#[cfg(target_env = "ohos")]
fn heap(ctx: &JSContext) -> String {
    use rong_arkjs_sys as sys;
    use rong_core::JSContextImpl;
    unsafe {
        let env: sys::JSVM_Env = *ctx.as_ref().as_raw();
        let mut vm: sys::JSVM_VM = std::ptr::null_mut();
        let mut stats: sys::JSVM_HeapStatistics = std::mem::zeroed();
        sys::OH_JSVM_GetVM(env, &mut vm);
        sys::OH_JSVM_GetHeapStatistics(vm, &mut stats);
        format!(
            "used heap {} bytes, global handles {} bytes",
            stats.usedHeapSize, stats.totalGlobalHandlesSize
        )
    }
}

#[cfg(not(target_env = "ohos"))]
fn heap(_ctx: &JSContext) -> String {
    String::from("heap not reported on this engine")
}

#[test]
fn rust_driven_calls_reading_a_result() -> Result<(), String> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let err = |e: RongJSError| e.to_string();
    let func: JSFunc = ctx
        .eval(Source::from_bytes("(i) => ({ i, label: 'item' })"))
        .map_err(err)?;

    rt.run_gc();
    let heap_before = heap(&ctx);
    let start = Instant::now();
    let mut sum = 0i64;
    for i in 0..CALLS {
        let item: JSObject = func.call(None, (i as i32,)).map_err(err)?;
        sum += item.get::<_, i32>("i").map_err(err)? as i64;
    }
    let elapsed = start.elapsed();
    rt.run_gc();
    let heap_after = heap(&ctx);

    let expected = (CALLS as i64 - 1) * CALLS as i64 / 2;
    log_line(&format!(
        "{CALLS} calls in {} ms, {} ns per call; {heap_before} -> {heap_after}",
        elapsed.as_millis(),
        elapsed.as_nanos() / CALLS as u128
    ));
    if sum == expected {
        Ok(())
    } else {
        Err(format!("results summed to {sum}, expected {expected}"))
    }
}
