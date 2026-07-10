# How to Implement Iterators

This guide explains how to make your Rust module's objects usable with JavaScript's `for...of` and `for await...of` syntax.

## Table of Contents

- [Background](#background)
- [Sync Iterators](#sync-iterators)
- [Async Iterators](#async-iterators)
  - [Approach 1: Stream-Based (Recommended)](#approach-1-stream-based-recommended)
  - [Approach 2: Manual Protocol](#approach-2-manual-protocol)
  - [Approach 3: Install on Existing Object](#approach-3-install-on-existing-object)
- [Real-World Example: ReadableStream](#real-world-example-readablestream)
- [Testing](#testing)

---

## Background

JavaScript's iteration protocols require an object to have:

**Sync** (`for...of`):
1. **`[Symbol.iterator]()`** — returns `this` (self-referential)
2. **`next()`** — returns `{ done: boolean, value: any }`
3. **`return()`** *(optional)* — called on early termination (`break` in `for...of`)

**Async** (`for await...of`):
1. **`[Symbol.asyncIterator]()`** — returns `this` (self-referential)
2. **`next()`** — returns a Promise resolving to `{ done: boolean, value: any }`
3. **`return()`** *(optional)* — called on early termination (`break` in `for await...of`)

Rong provides built-in support for both via `JSIterator` / `JSAsyncIterator` and extension traits, so you rarely need to implement the protocol by hand.

---

## Sync Iterators

If your data source implements `IntoIterator`, use the extension trait for a one-liner.

### Rust side

```rust
use rong::*;

pub fn init(ctx: &JSContext) -> JSResult<()> {
    let make_iter = JSFunc::new(ctx, move |ctx: JSContext| {
        vec!["hello", "world", "!"].to_js_iter(&ctx)
    })?;
    ctx.global().set("makeIter", make_iter)?;
    Ok(())
}
```

### JavaScript side

```javascript
for (const item of makeIter()) {
    console.log(item); // "hello", "world", "!"
}

// Manual usage — each next() returns a fresh object
const iter = makeIter();
const a = iter.next(); // { done: false, value: "hello" }
const b = iter.next(); // { done: false, value: "world" }
// a and b are independent objects
```

### How it works

The `to_js_iter()` extension method (from `IntoJSIteratorExt`) does three things automatically:

1. Wraps the iterator in `JSIterator`
2. Creates a JS object with `next()` that returns `{ done, value }`
3. Installs `[Symbol.iterator]` and `return()` for cleanup

`return()` is called by the JS engine when a `for...of` loop exits early (via `break`, `throw`, or `return`). It replaces the underlying Rust iterator with an empty one to release resources.

### Install on existing object

```rust
let obj = JSObject::new(ctx);
obj.set("name", "my-collection")?;
vec![1, 2, 3].install_js_iter(ctx, &obj)?;
// obj now has next(), return(), and [Symbol.iterator]
```

---

## Async Iterators

### Approach 1: Stream-Based (Recommended)

If your data source can be expressed as a `futures::Stream`, this is the simplest path.

#### Rust side

```rust
use rong::*;
use futures::stream;

pub fn init(ctx: &JSContext) -> JSResult<()> {
    // Any futures::Stream works — channels, async generators, etc.
    let make_iter = JSFunc::new(ctx, move |ctx: JSContext| {
        let s = stream::iter(vec!["hello", "world", "!"]);
        s.to_js_async_iter(&ctx)   // returns JSResult<JSObject>
    })?;
    ctx.global().set("makeIter", make_iter)?;
    Ok(())
}
```

#### JavaScript side

```javascript
for await (const item of makeIter()) {
    console.log(item); // "hello", "world", "!"
}
```

#### How it works

The `to_js_async_iter()` extension method (from `IntoJSAsyncIteratorExt`) does three things automatically:

1. Wraps the stream in `JSAsyncIterator`
2. Creates a JS object with `next()` that returns Promises
3. Installs `[Symbol.asyncIterator]` and `return()` for cleanup

Errors from the Rust side are propagated as rejected Promises using the runtime's normal JS error values, so they surface naturally in JavaScript `try/catch` blocks with message/stack metadata intact.

#### Using a channel as the stream

The most common real-world pattern uses a `tokio::sync::mpsc` channel:

```rust
use rong::*;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

fn create_counter(ctx: &JSContext) -> JSResult<JSObject> {
    let (tx, rx) = mpsc::channel::<i32>(32);

    // Producer: spawn background work that feeds the channel
    spawn_local(async move {
        for i in 1..=10 {
            if tx.send(i).await.is_err() { break; }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        // tx drops here → stream ends → done: true
    });

    // Consumer: wrap receiver as async iterator
    ReceiverStream::new(rx).to_js_async_iter(ctx)
}
```

```javascript
for await (const n of createCounter()) {
    console.log(n); // 1, 2, 3, ... 10
    if (n >= 5) break; // return() is called, stream is dropped
}
```

---

### Approach 2: Manual Protocol

When you need full control (e.g., custom `next()` logic that reads from a native struct), implement the protocol manually.

```rust
use rong::*;

fn make_manual_async_iter(ctx: &JSContext) -> JSResult<JSObject> {
    let obj = JSObject::new(ctx);
    let count = std::rc::Rc::new(std::cell::RefCell::new(0));

    // next() → Promise<{ done, value }>
    let c = count.clone();
    let next_fn = JSFunc::new(ctx, move |ctx: JSContext| -> JSObject {
        let c = c.clone();
        match ctx.promise() {
            Ok((promise, resolve, reject)) => {
                spawn_local(async move {
                    let mut n = c.borrow_mut();
                    *n += 1;
                    let result = JSObject::new(&ctx);
                    if *n <= 5 {
                        result.set("done", false).ok();
                        result.set("value", *n).ok();
                    } else {
                        result.set("done", true).ok();
                        result.set("value", JSValue::undefined(&ctx)).ok();
                    }
                    let _ = resolve.call::<_, ()>(None, (result,));
                });
                promise.into_object()
            }
            Err(_) => {
                let result = JSObject::new(&ctx);
                result.set("done", true).ok();
                result
            }
        }
    })?;
    obj.set("next", next_fn)?;

    // [Symbol.asyncIterator]() → this
    rong::install_async_iterator_symbol(ctx, &obj)?;

    Ok(obj)
}
```

### Key pattern: Promise creation

```rust
let (promise, resolve, reject) = ctx.promise()?;

spawn_local(async move {
    // ... do async work ...
    let _ = resolve.call::<_, ()>(None, (result,));
});

// Return the promise immediately (it resolves later)
promise.into_object()
```

---

### Approach 3: Install on Existing Object

When you have a `#[js_class]` class and want its instances to be async-iterable, you install the protocol on the class prototype.

This approach is more involved because you need to bridge between JavaScript's `this` binding and Rust's ownership model. The key challenge: the `next()` closure must borrow a native struct from the JS object, perform async work with it, and restore it for the next call — all while ensuring the receiver isn't dropped mid-iteration.

```rust
use rong::*;
use tokio_stream::wrappers::ReceiverStream;

#[js_class]
pub struct DataCursor {
    // Arc<Mutex<Option<...>>> because:
    // - Arc: shared between the JS closure and the native struct
    // - Mutex: safe to access from async context
    // - Option: allows take/put-back pattern for async borrowing
    rx: std::sync::Arc<std::sync::Mutex<Option<tokio::sync::mpsc::Receiver<String>>>>,
}

pub fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_class::<DataCursor>()?;

    let ctor = Class::lookup::<DataCursor>(ctx)?;
    let proto: JSObject = ctor.get("prototype")?;

    let next_fn = JSFunc::new(ctx, move |ctx: JSContext, this: This<JSObject>| async move {
        // Step 1: Borrow the native struct from the JS object
        let rx_slot = match (*this).borrow::<DataCursor>() {
            Ok(cursor) => cursor.rx.clone(),
            Err(_) => {
                return Err(HostError::new(rong::error::E_TYPE, "not a DataCursor")
                    .with_name("TypeError")
                    .into());
            }
        };

        // Step 2: Take the receiver out (so we can .await on it without holding the lock)
        let mut rx = {
            let mut guard = rx_slot.lock().unwrap();
            guard.take()
        };

        let Some(mut rx) = rx.take() else {
            // Receiver already consumed — iteration is done
            let out = JSObject::new(&ctx);
            out.set("done", true).ok();
            return Ok(out);
        };

        // Step 3: Await the next item
        let item = rx.recv().await;

        // Step 4: Put the receiver back for the next call
        if let Ok(mut slot) = rx_slot.lock() && slot.is_none() {
            *slot = Some(rx);
        }

        let out = JSObject::new(&ctx);
        match item {
            Some(value) => {
                out.set("done", false).ok();
                out.set("value", value).ok();
            }
            None => {
                out.set("done", true).ok();
            }
        }
        Ok(out)
    })?;
    proto.set("next", next_fn)?;

    install_async_iterator_symbol(ctx, &proto)?;
    Ok(())
}
```

This is the pattern used by `ReadableStream` — see `modules/rong_stream/src/readable.rs:576`.

---

## Real-World Example: ReadableStream

`ReadableStream` is the canonical example in the codebase. Key points:

| Aspect | Implementation |
|--------|---------------|
| Data source | `mpsc::Receiver<Result<Bytes, String>>` |
| `next()` | Async method on prototype, borrows `ReadableStream` from `this` |
| `[Symbol.asyncIterator]` | Installed via `install_async_iterator_symbol()` |
| `return()` | Not explicitly added (stream drops when receiver is consumed) |
| Per-instance setup | Called in `JSReadableStream::new()` after `Class::instance()` |

See: `modules/rong_stream/src/readable.rs` lines 576–633.

---

## Testing

### Rust test (integration)

```rust
use futures::stream;
use rong_test::*;

#[test]
fn my_async_iter_test() {
    async_run!(async |ctx: JSContext| {
        let data = stream::iter(vec![10, 20, 30]);
        let iter_fn = JSFunc::new(&ctx, move |ctx: JSContext| {
            data.clone().to_js_async_iter(&ctx)
        })?;
        ctx.global().set("myIter", iter_fn)?;

        let result: i32 = ctx.eval_async(Source::from_bytes(r#"
            (async () => {
                let sum = 0;
                for await (const n of myIter()) {
                    sum += n;
                }
                return sum;
            })()
        "#)).await?;
        assert_eq!(result, 60);
        Ok(())
    });
}
```

### JavaScript test (unit)

```javascript
const iter = createMyAsyncIter();
const results = [];
for await (const item of iter) {
    results.push(item);
}
assert.deepEqual(results, [10, 20, 30]);
```

---

## API Reference

| API | Description |
|-----|-------------|
| `iter.to_js_iter(&ctx)` | Convert any `IntoIterator` to a sync-iterable JSObject |
| `iter.install_js_iter(&ctx, &obj)` | Install `next()`/`return()`/`[Symbol.iterator]` on existing object |
| `stream.to_js_async_iter(&ctx)` | Convert any `Stream` to an async-iterable JSObject |
| `stream.install_js_async_iter(&ctx, &obj)` | Install `next()`/`return()`/`[Symbol.asyncIterator]` on existing object |
| `install_iterator_symbol(&ctx, &obj)` | Add only `[Symbol.iterator]` (when `next()` already exists) |
| `install_async_iterator_symbol(&ctx, &obj)` | Add only `[Symbol.asyncIterator]` (when `next()` already exists) |
| `ctx.promise()` | Create `(promise, resolve, reject)` for manual Promise construction |
| `spawn_local(async { ... })` | Spawn async work on the current thread's LocalSet |

## See Also

- [Module Development](./module_development.md) — Functions, classes, and macros
- [Value System](./value_system.md) — Type conversions between Rust and JavaScript
- Source: `core/src/iterator.rs` — `JSIterator`, `JSAsyncIterator`, and extension traits
- Source: `modules/rong_stream/src/readable.rs` — ReadableStream async iteration
