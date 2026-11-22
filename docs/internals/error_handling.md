# Error Handling Guide

This document is for Rong module developers, covering error handling patterns at the Rust ↔ JS boundary.

Two concepts matter throughout this guide:

- `RongError`: a host-generated Error-like object with `name/message/code/data`
- thrown payload: any preserved JavaScript value (`throw 1`, Promise rejection reason, abort reason, ...)

---

## Quick Reference

### Common Scenarios

| Scenario                 | Rust Code                                                                          |
| :---                     | :---                                                                               |
| Type mismatch            | `HostError::type_mismatch("a string", "number")` (parameters do this for you)      |
| Wrong argument count     | `HostError::invalid_arg_count(2, got)`                                             |
| Out of range             | `HostError::new(E_OUT_OF_RANGE, "index out of bounds").with_name("RangeError")`    |
| IO failure               | `HostError::new(E_IO, err.to_string())`                                            |
| Permission denied        | `HostError::new(E_PERMISSION_DENIED, "access denied")`                             |
| Operation aborted        | `HostError::aborted(None)` or `RongJSError::from_thrown_value(reason)`            |
| Invalid state            | `HostError::new(E_INVALID_STATE, "stream already closed")`                         |
| Preserve JS thrown value | `RongJSError::from_thrown_value(js_value)`                                         |

### Complete Examples

```rust
use rong::error::{E_INVALID_ARG, E_IO, E_TYPE, HostError, JSResult};

// Example 1: Throw TypeError (argument validation)
fn parse_url(url: String) -> JSResult<Url> {
    Url::parse(&url).map_err(|e| {
        HostError::new(E_INVALID_ARG, format!("Invalid URL: {e}"))
            .with_name("TypeError")
            .into()
    })
}

// Example 2: Throw Error with structured data
fn read_file(path: &str) -> JSResult<Vec<u8>> {
    std::fs::read(path).map_err(|e| {
        HostError::new(E_IO, e.to_string())
            .with_data(rong::err_data!({
                path: path,
                os_error: (e.raw_os_error()),  // Option<i32> -> number | null
                tags: (vec!["fs", "read"]),    // Vec<&str> -> string[]
            }))
            .into()
    })
}

// Example 3: Preserve JS thrown value (for callback/promise)
fn call_js_callback(func: JSFunc, args: impl IntoArgs) -> JSResult<JSValue> {
    let result = func.call(None, args)?;
    if result.is_exception() {
        return Err(RongJSError::from_thrown_value(result));
    }
    Ok(result)
}
```

### What JS Receives

```js
try {
    await readFile("/etc/shadow");
} catch (e) {
    if (e instanceof Error) {
        e.name;    // "Error"
        e.message; // "permission denied"
        e.code;    // "E_PERMISSION_DENIED" (stable, for programmatic handling)
        e.data;    // { path: "/etc/shadow" } (optional structured details)
    }
}
```

---

## Three Core Rules

### 1. Rust `Err` = JS throw/reject

This is a hard rule, no exceptions:

```
JSResult<T> = Ok(v)  →  JS normal return / Promise.resolve(v)
JSResult<T> = Err(e) →  JS throw / Promise.reject(payload)
```

If you want certain failures to **not enter catch**, you must use `Ok(...)` to return an explicit result object (see "API Design" section).

`JSResult<T>` is also the bridge type used by function registration and Promise integration, so this rule applies uniformly to sync and async host functions.

### 2. Use `is_exception()` to detect exceptions

```rust
// ✅ Correct
if value.is_exception() { return Err(...); }

// ❌ Wrong - is_error() only means "this is an Error object", not an exception
if value.is_exception() || value.is_error() { ... }
```

In JS, `return new Error("x")` is a normal return value; only `throw` enters the exception channel.

### 3. Host failures use HostError, JS thrown values use from_thrown_value

| Error Source      | Rust Representation                 | Use Case                                   |
| :---              | :---                                | :---                                       |
| Rust-side failure | `HostError::new(code, msg)`         | Argument validation, IO, permissions, etc. |
| JS-side throw     | `RongJSError::from_thrown_value(v)` | Exceptions from eval/call/promise          |

---

## Error Code Reference

Location: [`rong::error::E_*`](../core/src/error.rs)

| Code                      | Meaning                   | Typical `.with_name()`        |
| :---                      | :---                      | :---                          |
| `E_TYPE`                  | Type mismatch             | `TypeError`                   |
| `E_INVALID_ARG`           | Invalid argument          | `TypeError`                   |
| `E_OUT_OF_RANGE`          | Index/value out of bounds | `RangeError`                  |
| `E_MISSING_PROPERTY`      | Property not found        | `ReferenceError`              |
| `E_IO`                    | IO error                  | `Error`                       |
| `E_PERMISSION_DENIED`     | Permission denied         | `Error`                       |
| `E_NOT_FOUND`             | Resource not found        | `Error`                       |
| `E_ALREADY_EXISTS`        | Resource already exists   | `Error`                       |
| `E_ABORT`                 | Operation aborted         | `AbortError`                  |
| `E_INVALID_STATE`         | Invalid state             | `Error` / `InvalidStateError` |
| `E_INVALID_DATA`          | Data format error         | `Error`                       |
| `E_NOT_SUPPORTED`         | Feature not supported     | `Error`                       |
| `E_STREAM`                | Stream error              | `Error`                       |
| `E_COMPILE`               | Compilation error         | `SyntaxError`                 |
| `E_INTERNAL`              | Internal error            | `Error`                       |
| `E_ILLEGAL_CONSTRUCTOR`   | Illegal constructor       | `TypeError`                   |
| `E_JS_THROWN`             | Wrapped JS thrown value   | `Error` + original payload in `cause` / `data.thrown` |

**Module-specific codes**: Modules can define their own codes with prefixes like `FS_*`, `NET_*`, `HTTP_*`.

---

## API Design: Throwing vs No-throw

### Throwing API (Default)

Failures enter `catch`, suitable for network/IO/permissions/cancellation:

```rust
// Rust
pub async fn fetch(url: String) -> JSResult<Response> { ... }
```

```js
// JS
try {
    const resp = await fetch(url);
} catch (e) {
    // Network failure, cancellation, permission error...
}
```

### No-throw API (Explicit Result)

Failures don't enter `catch`, suitable for frequent expected failures:

```rust
// Rust - note: returns Ok(ResultObject) instead of Err(...)
pub fn try_parse_json(ctx: &JSContext, text: String) -> JSResult<JSValue> {
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(v) => Ok(rong::js_value!(ctx, { ok: true, value: (v) })),
        Err(e) => Ok(rong::js_value!(ctx, { ok: false, error: (e.to_string()) })),
    }
}
```

```js
// JS
const result = tryParseJson(text);
if (result.ok) {
    console.log(result.value);
} else {
    console.log("parse failed:", result.error);
}
```

**Naming conventions**: `tryXxx`, `xxxOrNull`, `parseXxxSafe`

---

## Abort Semantics

`AbortController.abort(reason)` must preserve `reason` as-is:

```rust
// ✅ Preserve original reason
if let Some(reason) = signal.reason() {
    return Err(RongJSError::from_thrown_value(reason));
}

// ✅ Use default AbortError when no reason
return Err(HostError::aborted(None).into());
```

```js
// JS expectation
controller.abort("custom reason");
// catch should receive "custom reason" (===), not a wrapped Error
```

---

## HostError API Reference

```rust
// Basic construction
HostError::new(code, message)

// Set JS Error name (TypeError, RangeError, AbortError, ...)
.with_name("TypeError")

// A stable refinement of `code`, and free-form developer detail; both become
// own properties of the JS error and are absent when unset
.with_reason("host_upgrade_required")
.with_detail("needs host 2.1")

// Attach structured data
.with_data(rong::err_data!({ key: "value" }))
.with_data(rong::err_data!({ maybe_code: (Some(5)) }))

// Convenience constructors
HostError::type_mismatch(expected, got)      // Wrong type: "must be {expected}, got {got}"
HostError::range_mismatch(expected, got)     // Integer out of range
HostError::missing_field(field)              // Required field absent
HostError::invalid_arg_count(expected, got)  // Wrong argument count
HostError::aborted(None)                      // Default AbortError

// Preserve original JS abort reason as-is
RongJSError::from_thrown_value(reason)

// Convert to RongJSError
host_error.into()  // converts HostError into RongJSError
```

### Strict parameters

A `String`, `bool`, `f64` or integer parameter accepts only that JS type, and
the refusal names the argument: `argument 1 must be a string, got undefined`
(`E_INVALID_ARG`). Take `Coerced<String>` (or `Coerced<i32>`, …) where a Web
API calls for `DOMString`/`long` conversion, and `JSValue::to_string_coerced()`
to format a value. A custom `FromJSValue` that converts parts of a value can
locate a refusal with `err.at_field("name")` or `err.at_index(i)`.

### Embedder error classes

`ctx.register_error_class("AppError", ctor)` makes host errors named
`AppError` instances of `ctor` on every engine (`"Error"` covers every plain
`HostError::new`). The class sets `name`; Rong sets `code`, `reason`, `detail`
and `data`.

---

## Common Mistakes

### ❌ Using `is_error()` to detect exceptions

```rust
// Wrong: `return new Error("x")` will be treated as exception
if v.is_error() { return Err(...); }
```

### ❌ Losing the JS thrown value

```rust
// Wrong: original thrown value lost, JS can't access cause
Err(HostError::new(E_ERROR, "call failed").into())

// Correct: preserve original value
Err(RongJSError::from_thrown_value(js_exception))
```

### ❌ Mixing throw and no-throw in one function

```rust
// Wrong: sometimes throws, sometimes returns null
pub fn get_item(key: String) -> JSResult<Option<String>> {
    if key.is_empty() {
        return Err(...);  // throws
    }
    Ok(storage.get(&key))  // returns None if not found, no throw
}
```

Pick one style: either always throw, or provide a `tryGetItem` variant.
