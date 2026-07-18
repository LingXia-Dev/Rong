# Execution timeouts and interruption

Use a task timeout for application-level execution limits. It owns the entire
lifecycle and leaves the worker reusable:

```rust
use rong::{JSResult, Rong, RongJS, Source};
use std::time::Duration;

# async fn run() -> JSResult<()> {
let rong = Rong::<RongJS>::builder().shared().workers(2).build()?;
let value = rong
    .call_with_timeout(
        Duration::from_secs(1),
        |runtime, _receiver| async move {
            runtime.context().eval::<i32>(Source::from_bytes("21 * 2"))
        },
    )
    .await?;
assert_eq!(value, 42);
# Ok(())
# }
```

`call_with_timeout` covers worker selection, queueing, and execution. If its
deadline expires:

- a task that has not started is cancelled without interrupting the task ahead
  of it;
- a running Rust future is aborted and its JavaScript runtime is interrupted;
- the task-owned interruption request is released at the worker execution
  boundary, before another task starts; and
- the caller receives `E_TIMEOUT`, detectable with
  `RongJSError::is_timeout()`.

The same workflow is available through `Worker::call_with_timeout`,
`PinnedWorker::call_with_timeout`, `PinnedRong::call_with_timeout`, and
`TaskHandle::join_with_timeout`. Blocking callers can use the corresponding
`call_blocking_with_timeout` methods.

## Engine capability

`InterruptHandle::mode()` and `Worker::interrupt_mode()` return a typed
`InterruptMode`:

- `Unbound`: no runtime has been attached to a newly constructed handle yet;
- `Cooperative`: queued and yielding tasks can be cancelled, and new
  evaluations are rejected, but non-yielding synchronous JavaScript cannot be
  stopped; and
- `Preemptive`: running JavaScript can be stopped even inside code such as
  `while (true) {}`.

QuickJS and JSC source/JSCOnly builds are preemptive. Apple's system JSC
framework is preemptive with `jscore-interrupt`; without that feature it is
cooperative. ArkJS is cooperative because the exposed JSVM API has no runtime
termination primitive.

On a cooperative engine, a timeout still returns `E_TIMEOUT` and aborts a
yielding Rust future. If the worker is inside non-yielding synchronous
JavaScript, however, that worker remains busy until the engine call returns.

## Low-level control

Use `InterruptHandle` when another thread or a host supervisor must control the
runtime directly:

```rust
let interrupt = worker.interrupt_handle();
interrupt.interrupt();

// The interrupted invocation is not resumed. This only permits future work.
interrupt.clear();
```

`interrupt()` is persistent and level-triggered: while it remains set, running
JavaScript is preempted where supported and every new evaluation fails with
`E_INTERRUPTED`. `clear()` never resumes an aborted script.

For independently owned requests, use a scoped guard:

```rust
let request = interrupt.interrupt_scoped();
// Other persistent or scoped requests remain independent.
drop(request);
```

Dropping one `InterruptGuard` cannot clear another guard or a persistent
request. This makes layered supervisors safe to compose. Synchronous eval and
compile APIs normalize native engine termination to `E_INTERRUPTED`, detectable
with `RongJSError::is_interrupted()`.

## Shutdown is separate

`Worker::terminate()` requests graceful worker shutdown and aborts the current
Rust task at its next yield. It intentionally does not imply hard JavaScript
interruption. Use a task timeout for bounded work, or explicitly interrupt
before termination when a supervisor must break non-yielding JavaScript.
