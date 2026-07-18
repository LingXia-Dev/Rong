# Worker pool selection and lifecycle

Use Rong's host-side worker pools when JavaScript tasks must run on dedicated
runtime threads. These are distinct from the JavaScript `Worker` class provided
by the `rong_worker` module.

## Choose the placement model

| Requirement | Model |
|---|---|
| Current-thread, single-runtime embedding | Raw `RongJS::runtime()` |
| Independent stateless tasks on any idle worker | `shared()` |
| Stable key affinity with retained per-key state | `pinned::<K, S>()` |

The builder requires an explicit model:

```rust
let shared = Rong::<RongJS>::builder()
    .shared()
    .workers(4)
    .task_queue_capacity(100)
    .message_queue_capacity(512)
    .build()?;
```

Do not depend on prior task state in shared mode. Each task may run on a
different worker.

## Call or spawn shared work

Use `call` when only the result matters:

```rust
let value: i32 = shared
    .call(|runtime, _receiver| async move {
        runtime.context().eval(Source::from_bytes("6 * 7"))
    })
    .await?;
```

Use `spawn` when the caller needs a `TaskHandle`, task messaging, or separate
join timing. `TaskHandle::send` writes to that task's `MessageReceiver`; `join`
returns the typed result.

## Use pinned state deliberately

Pinned tasks receive the routing key and the state returned by the previous
successful task for that key:

```rust
let pinned = Rong::<RongJS>::builder()
    .pinned::<String, usize>()
    .workers(2)
    .build()?;

let next = pinned
    .call("tenant-a".to_string(), |_runtime, _key, state, _receiver| async move {
        let next = state.unwrap_or_default() + 1;
        (Ok(next), Some(next))
    })
    .await?;
```

An aborted or timed-out pinned task may not produce replacement state. Design
state updates so cancellation cannot expose a partially committed host state.

## Bound task execution

Use `call_with_timeout` for a deadline covering dispatch, queueing, and
execution. Use `TaskHandle::join_with_timeout` when a task was spawned earlier.
Read [interruption.md](interruption.md) before implementing custom interrupt
policy.

Queued timeouts cancel only the queued task. Running timeouts abort the Rust
future and request JavaScript interruption; the worker releases that scoped
request before accepting another task.

## Avoid sync bridge misuse

`call_blocking` and `call_blocking_with_timeout` are for synchronous hosts with
no active Tokio runtime. They reject calls made inside Rong worker threads or an
active Tokio runtime. Use async `call` methods in those contexts.

## Shut down explicitly

- `join()` waits for currently queued/in-flight work to drain.
- Pool `Worker::terminate()` is a graceful lifecycle signal; it does not imply
  hard JavaScript interruption.
- `Rong::shutdown()` or `PinnedRong::shutdown()` signals workers and joins their
  OS threads.
- Dropping the final pool owner invokes shutdown, but explicit shutdown makes
  host lifecycle and error reporting clearer.

Do not reuse a worker after a persistent manual interrupt until the request has
been cleared. Prefer task-scoped timeout APIs when the worker must remain
reusable.
