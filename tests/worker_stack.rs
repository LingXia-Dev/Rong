use rong::{JSEngine, JSObject, JSResult, JSRuntime, Rong, RongJS, Source};
use std::hint::black_box;
use std::time::Duration;

/// How far below the calling task the call into JavaScript is made: past
/// QuickJS's 1 MiB default limit, within a worker's stack. In debug builds a
/// host future is polled some hundreds of KiB below its JS caller.
const DEPTH: usize = 1536 * 1024;

const TASK_TIMEOUT: Duration = Duration::from_secs(10);

/// Runs `f` once this thread's stack is `DEPTH` below `top`. Measured by
/// address rather than by frame count, so the depth holds in debug and
/// release builds alike.
#[inline(never)]
fn below<R>(top: usize, f: &mut dyn FnMut() -> R) -> R {
    let pad = black_box([0u8; 16 * 1024]);
    let here = &pad as *const _ as usize;
    let r = if top.saturating_sub(here) >= DEPTH {
        f()
    } else {
        below(top, f)
    };
    black_box(&pad);
    r
}

/// Serializes an object through `JSON.stringify` from `DEPTH` below here,
/// the call a host function makes on a payload it was handed.
fn stringify_deep(runtime: &JSRuntime) -> JSResult<String> {
    let ctx = runtime.context();
    let object: JSObject = ctx.eval(Source::from_bytes(b"({ a: 1 })"))?;
    let top = 0u8;
    below(&top as *const u8 as usize, &mut || object.to_json_string())
}

#[tokio::test]
async fn a_shared_worker_calls_js_deeper_than_the_engine_default() -> JSResult<()> {
    let rong = Rong::<RongJS>::builder().shared().workers(1).build()?;
    let json = rong
        .call(|runtime, _receiver| async move { stringify_deep(&runtime) })
        .await?;
    assert_eq!(json, r#"{"a":1}"#);
    Ok(())
}

#[test]
fn a_pinned_worker_calls_js_deeper_than_the_engine_default() {
    let rong = Rong::<RongJS>::builder()
        .pinned::<u8, ()>()
        .workers(1)
        .build()
        .unwrap();
    let json = rong
        .call_blocking_with_timeout(
            TASK_TIMEOUT,
            0,
            |runtime: JSRuntime, _key, state, _receiver| async move {
                (stringify_deep(&runtime), state)
            },
        )
        .unwrap();
    assert_eq!(json, r#"{"a":1}"#);
}

/// Any thread with a large stack gets the room, not only rong's workers:
/// the limit comes from the thread the runtime is created on.
#[test]
fn a_runtime_on_a_large_thread_calls_js_deeper_than_the_engine_default() {
    let json = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| stringify_deep(&RongJS::runtime()))
        .unwrap()
        .join()
        .unwrap()
        .unwrap();
    assert_eq!(json, r#"{"a":1}"#);
}

/// The larger budget still ends runaway recursion with a catchable error
/// instead of overflowing the thread.
#[tokio::test]
async fn runaway_recursion_on_a_worker_still_throws() -> JSResult<()> {
    let rong = Rong::<RongJS>::builder().shared().workers(1).build()?;
    let caught: bool = rong
        .call(|runtime, _receiver| async move {
            runtime.context().eval(Source::from_bytes(
                b"(() => { function f() { return 1 + f() } \
                  try { f(); return false } catch (e) { return e instanceof RangeError } })()",
            ))
        })
        .await?;
    assert!(caught);
    Ok(())
}
