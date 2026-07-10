# Classes: exposing Rust structs to JavaScript

Classes model stateful objects (e.g. `new Point2D(10, 20)`).

## Definition

```rust
use rong::*;
use rong::{js_class, js_method};

#[js_class]
#[derive(Debug)]
struct Point { x: i32, y: i32 }

#[js_class(rename = "Point2D")]   // rename optional; defaults to the struct name
impl Point {
    #[js_method(constructor)]
    fn new(x: i32, y: i32) -> Self { Self { x, y } }
}
```

- `#[js_class]` makes the struct available to the macro system.
- `#[js_class(rename = "JsName")]` exposes the impl block; `rename` sets the JS class name.
- `#[js_method(...)]` marks individual methods to expose.

## Registration: visible vs hidden

```rust
ctx.register_class::<Point>()?;         // exposes globalThis.Point2D (new Point2D(...))
ctx.register_hidden_class::<Point>()?;  // registers metadata but NO global constructor
```

Hidden registration is for Rust-owned interop types. After it, you can still
`Class::lookup::<Point>(&ctx)?` / `Class::prototype::<Point>(&ctx)?`, and create
instances from Rust:

```rust
ctx.register_hidden_class::<Point>()?;
let class = Class::lookup::<Point>(ctx)?;
let instance = class.instance(Point { x: 1, y: 2 });
```

## Constructors

Mark with `#[js_method(constructor)]`. Must return `Self` (or `JSResult<Self>`
for fallible construction).

```rust
#[js_method(constructor)]
fn new(x: i32, y: i32) -> JSResult<Self> {
    if x < 0 || y < 0 {
        return Err(HostError::new(rong::error::E_INVALID_ARG, "must be non-negative")
            .with_name("TypeError").into());
    }
    Ok(Self { x, y })
}
```

## Instance, static, and mutable methods

```rust
#[js_class]
impl Point {
    #[js_method]                      // &self -> instance method
    fn distance(&self) -> f64 { ((self.x.pow(2) + self.y.pow(2)) as f64).sqrt() }

    #[js_method(rename = "add")]
    fn add(&self, other: Point) -> Self { Self { x: self.x + other.x, y: self.y + other.y } }

    #[js_method(rename = "moveBy")]   // &mut self -> mutating method
    fn move_by(&mut self, dx: i32, dy: i32) { self.x += dx; self.y += dy; }

    #[js_method]                      // no self -> static method (Point2D.origin())
    fn origin() -> Self { Self { x: 0, y: 0 } }
}
```

## Getters and setters

```rust
#[js_method(getter, enumerable)]
fn x(&self) -> i32 { self.x }

#[js_method(setter, rename = "x")]   // setter must `rename` to match the getter; needs &mut self
fn set_x(&mut self, x: i32) { self.x = x; }
```

- `enumerable` makes the property show up in `Object.keys()` / `for...in`.

## Attribute reference

| Attribute       | Meaning                              | Example                              |
| :---            | :---                                 | :---                                 |
| `constructor`   | Class constructor                    | `#[js_method(constructor)]`          |
| `private`       | Private TS constructor (with `constructor`) | `#[js_method(constructor, private)]` |
| `rename = "x"`  | JS method/property name              | `#[js_method(rename = "moveBy")]`    |
| `getter`        | Property getter                      | `#[js_method(getter)]`               |
| `setter`        | Property setter                      | `#[js_method(setter, rename = "x")]` |
| `enumerable`    | Property enumerable                  | `#[js_method(getter, enumerable)]`   |

`#[js_class(rename = "JsName")]` sets the class name on the impl block.

## Object-shaped inputs and outputs

Derive `FromJSObject` to accept a JS object as a Rust struct, and `IntoJSObject` to
return one. Use `#[js_name = "jsName"]` to map names; `Option<T>` fields are
optional (omitted when `None` on output).

Default to this for public API shapes. Use raw `JSObject` only for dynamic
dictionaries or pass-through values. Use `#[ts_type = "..."]` for field-level TS
precision, and `#[ts_skip]` for internal derived parser structs that should not
be exported.

```rust
use rong::{FromJSObject, IntoJSObject};
use rong::function::Optional;

#[derive(FromJSObject, Default)]
pub struct StorageOptions {
    #[js_name = "maxSize"] max_size: Option<u32>,
    compression: Option<bool>,
}

#[derive(IntoJSObject)]
pub struct StorageInfo {
    #[js_name = "currentSize"] current_size: u32,
    #[js_name = "keyCount"]    key_count: u32,
    #[ts_type = "number | bigint"]
    #[js_name = "lastRowid"]   last_rowid: i64,
}

#[js_class]
impl Storage {
    #[js_method(constructor)]
    fn new(path: String, options: Optional<StorageOptions>) -> JSResult<Self> {
        let opts = options.0.unwrap_or_default();
        // opts.max_size, opts.compression ...
        todo!()
    }

    #[js_method]
    fn info(&self) -> JSResult<StorageInfo> { todo!() }
}
```

## Numeric constant objects

Use `#[js_numeric_enum]` for numeric constants exposed as a JS object, such as
`Rong.SeekMode.Start === 0`. Typegen emits a const object plus a literal-union
type; do not write a TS enum prelude for this shape.

```rust
#[js_numeric_enum]
enum SeekMode {
    Start = 0,
    Current = 1,
    End = 2,
}

pub(crate) fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.host_namespace().set("SeekMode", SeekMode::js_object(ctx)?)?;
    Ok(())
}
```

## Union-type parameters (`string | Request | URL`, etc.)

Two patterns:

**1. Accept `JSValue` and probe by type** - check the most specific types first
(native structs via `borrow`), then arrays, then plain objects, then strings.

```rust
#[js_method(constructor)]
fn new(input: JSValue, init: Optional<RequestInit>) -> JSResult<Self> {
    if let Ok(url_str) = input.clone().try_into::<String>() { /* string */ }
    if let Some(obj) = input.into_object() {
        if let Ok(req) = obj.borrow::<Request>() { return Ok(req.clone()); }     // existing Request
        if let Ok(url) = obj.borrow::<URL>() { /* URL object */ }
    }
    Err(HostError::new(rong::error::E_TYPE, "string, Request, or URL")
        .with_name("TypeError").into())
}
```

**2. A reusable enum implementing `FromJSValue`** - when the same union appears
in multiple APIs.

```rust
pub enum EventKey { String(String), Symbol(JSSymbol) }

impl FromJSValue<JSEngineValue> for EventKey {
    fn from_js_value(ctx: &JSContext, value: JSValue) -> JSResult<Self> {
        if let Ok(s) = String::from_js_value(ctx, value.clone()) { return Ok(EventKey::String(s)); }
        if let Ok(sym) = JSSymbol::from_js_value(ctx, value) { return Ok(EventKey::Symbol(sym)); }
        Err(HostError::new(rong::error::E_INVALID_ARG, "string or symbol")
            .with_name("TypeError").into())
    }
}
```

Type-checking helpers: `is_string()`, `is_array_buffer()`, `is_undefined()`/`is_null()`,
`into_object() -> Option<JSObject>`, `obj.borrow::<T>()`, `JSArray::from_object(obj)`,
`JSTypedArray::from_object(obj)`, `value.try_into::<T>()`.

Real references in the repo: `modules/rong_http/src/request.rs`,
`modules/rong_http/src/header.rs`, `modules/rong_event/src/event_emitter.rs`.
