# Value System and Type Conversion Guide

This document explains the core value types, traits, and conversion APIs in the Rong JavaScript engine binding layer.

## Table of Contents

- [Overview](#overview)
- [Quick Start](#quick-start)
- [Type Hierarchy](#type-hierarchy)
- [Core Traits](#core-traits)
  - [JSValueImpl](#jsvalueimpl)
  - [FromJSValue](#fromjsvaluev)
  - [IntoJSValue](#intojsvaluev)
  - [JSTypeOf](#jstypeof)
  - [JSObjectOps](#jsobjectops)
- [Value Types](#value-types)
  - [JSValue](#jsvaluev)
  - [JSObject](#jsobjectv)
  - [JSArray](#jsarrayv)
  - [JSFunc](#jsfuncv)
  - [JSDate](#jsdatev)
- [Type Conversion](#type-conversion)
  - [Rust → JavaScript (IntoJSValue)](#rust--javascript-intojsvalue)
  - [JavaScript → Rust (FromJSValue)](#javascript--rust-fromjsvalue)
  - [Complete Type Mapping Reference](#complete-type-mapping-reference)
- [API Reference](#api-reference)
- [Common Patterns](#common-patterns)
- [Common Pitfalls](#common-pitfalls)

---

## Overview

The value system provides a type-safe bridge between Rust and JavaScript. It abstracts over different JS engines (QuickJS, ArkTS, etc.) through a trait-based design.

**Key Design Principles:**

1. **Engine Abstraction**: The generic parameter `V` represents engine-specific value types
2. **Type Safety**: `JSValue<V>` bundles raw values with context to prevent misuse
3. **Zero-Cost Wrappers**: Specialized types (`JSObject`, `JSArray`, etc.) are thin wrappers
4. **Bidirectional Conversion**: `FromJSValue` and `IntoJSValue` traits handle all conversions

---

## Quick Start

Most common conversions you'll need:

```rust
// Rust → JavaScript
let js_num = JSValue::from_rust(&ctx, 42);
let js_str = JSValue::from_rust(&ctx, "hello");
let js_bool = true.into_js_value(&ctx);

// JavaScript → Rust
let num: i32 = js_value.to_rust()?;
let s: String = js_value.to_rust()?;
let b: bool = js_value.to_rust()?;

// Working with objects
let obj = JSObject::new(&ctx);
obj.set("name", "Alice")?;
let name: String = obj.get("name")?;

// Working with arrays
let arr = JSArray::new(&ctx)?;
arr.push(1)?;
arr.push(2)?;
let first: i32 = arr.get(0)?.unwrap();
```

For detailed information, see [Type Conversion](#type-conversion) and [API Reference](#api-reference).

---

## Type Hierarchy

```
                    ┌─────────────────┐
                    │  V (raw value)  │  Engine-specific (e.g., QJSValue)
                    │  JSValueImpl    │
                    └────────┬────────┘
                             │
                             │ wrapped by
                             ▼
                    ┌─────────────────┐
                    │   JSValue<V>    │  Safe wrapper with context
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┬──────────────┐
              │              │              │              │
              ▼              ▼              ▼              ▼
       ┌────────────┐ ┌────────────┐ ┌────────────┐ ┌────────────┐
       │ JSObject<V>│ │  JSFunc<V> │ │ JSSymbol<V>│ │  JSDate<V> │
       └──────┬─────┘ └────────────┘ └────────────┘ └────────────┘
              │
      ┌───────┴───────┐
      │               │
      ▼               ▼
┌────────────┐ ┌──────────────┐
│ JSArray<V> │ │JSException<V>│
└────────────┘ └──────────────┘
```

### Relationship Summary

| Type             | Wraps         | Represents                                  |
| :---             | :---          | :---                                        |
| `V`              | -             | Raw engine value (implements `JSValueImpl`) |
| `JSValue<V>`     | `V`           | Any JS value with context                   |
| `JSObject<V>`    | `JSValue<V>`  | JS object                                   |
| `JSArray<V>`     | `JSObject<V>` | JS array                                    |
| `JSFunc<V>`      | `JSObject<V>` | JS function                                 |
| `JSDate<V>`      | `JSValue<V>`  | JS Date object                              |
| `JSSymbol<V>`    | `JSValue<V>`  | JS Symbol                                   |
| `JSException<V>` | `JSObject<V>` | Exception-channel object (thrown/rejected payload; often an `Error`) |

---

## Core Traits

### `JSValueImpl`

The foundational trait that all engine-specific value types must implement.

```rust
pub trait JSValueImpl: Clone + PartialEq + Hash {
    type RawValue: Copy;                              // FFI-level value
    type Context: JSContextImpl<Value = Self>;        // Associated context type

    fn from_borrowed_raw(ctx: RawContext, value: RawValue) -> Self;
    fn from_owned_raw(ctx: RawContext, value: RawValue) -> Self;
    fn into_raw_value(self) -> RawValue;

    fn create_null(ctx: &Self::Context) -> Self;
    fn create_undefined(ctx: &Self::Context) -> Self;
    fn create_symbol(ctx: &Self::Context, description: &str) -> Self;
    fn create_date(ctx: &Self::Context, epoch_ms: f64) -> Self;
    // ...
}
```

### `FromJSValue<V>`

Converts JavaScript values to Rust types.

```rust
pub trait FromJSValue<V>: Sized
where
    V: JSValueImpl,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self>;
}
```

### `IntoJSValue<V>`

Converts Rust types to JavaScript values.

```rust
pub trait IntoJSValue<V>
where
    V: JSValueImpl,
{
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V>;
}
```

### `JSTypeOf`

Provides runtime type checking for JS values.

```rust
pub trait JSTypeOf {
    fn type_of(&self) -> JSValueType;
    fn is_object(&self) -> bool;
    fn is_array(&self) -> bool;
    fn is_function(&self) -> bool;
    fn is_string(&self) -> bool;
    fn is_number(&self) -> bool;
    fn is_boolean(&self) -> bool;
    fn is_null(&self) -> bool;
    fn is_undefined(&self) -> bool;
    fn is_exception(&self) -> bool;
    fn is_promise(&self) -> bool;
    fn is_date(&self) -> bool;
    fn is_symbol(&self) -> bool;
    // ...
}
```

### `JSObjectOps`

Operations available on JavaScript objects.

```rust
pub trait JSObjectOps: JSValueConversion + JSTypeOf {
    fn new_object(ctx: &Self::Context) -> Self;
    fn get_property(&self, key: Self) -> Result<Option<Self>, Self>;
    fn set_property(&self, key: Self, value: Self) -> Result<(), Self>;
    fn del_property(&self, key: Self) -> Result<bool, Self>;
    fn has_property(&self, key: Self) -> Result<bool, Self>;
    fn get_own_property_names(&self) -> Result<Vec<Self>, Self>;
    // ...
}
```

### `JSValueConversion`

A helper trait that bundles all primitive type conversion bounds.

```rust
pub trait JSValueConversion:
    JSValueImpl
    + for<'a> From<(&'a Self::Context, bool)>
    + for<'a> From<(&'a Self::Context, i32)>
    + for<'a> From<(&'a Self::Context, f64)>
    + for<'a> From<(&'a Self::Context, &'a str)>
    + TryInto<bool, Error = RongJSError>
    + TryInto<i32, Error = RongJSError>
    + TryInto<f64, Error = RongJSError>
    + TryInto<String, Error = RongJSError>
    // ... more types
{
}
```

### `JSCompatible`

Marker trait for Rust primitive types that have direct JS equivalents.

```rust
pub trait JSCompatible: Sized {}

// Implemented for: i32, u32, i64, u64, f64, bool
```

---

## Value Types

### `JSValue<V>`

The primary safe wrapper for JavaScript values.

```rust
pub struct JSValue<V: JSValueImpl> {
    inner: V,
}
```

**Key Methods:**

| Method                 | Description                                   |
| :---                   | :---                                          |
| `to_rust::<T>(self)`   | Convert to Rust type `T` (uses `FromJSValue`) |
| `from_rust(&ctx, val)` | Create from Rust value (uses `IntoJSValue`)   |
| `from_raw(ctx, value)` | Create from raw engine value                  |
| `into_value(self)`     | Extract raw engine value                      |
| `as_value(&self)`      | Borrow raw engine value                       |
| `context(&self)`       | Get associated context                        |
| `undefined(ctx)`       | Create JS `undefined`                         |
| `null(ctx)`            | Create JS `null`                              |
| `into_object(self)`    | Convert to `JSObject` if applicable           |

### `JSObject<V>`

Represents JavaScript objects. Wraps `JSValue<V>` and implements `Deref<Target = JSValue<V>>`.

```rust
pub struct JSObject<V: JSValueImpl>(JSValue<V>);
```

**Key Methods:**

| Method                         | Description              |
| :---                           | :---                     |
| `new(ctx)`                     | Create empty object      |
| `from_json_string(ctx, json)`  | Parse JSON into object   |
| `get<K, T>(key)`               | Get property as type `T` |
| `set<K, T>(key, value)`        | Set property             |
| `del(key)`                     | Delete property          |
| `has(key)`                     | Check property existence |
| `keys()`                       | Get all property keys    |
| `values()`                     | Get all property values  |
| `entries()`                    | Get key-value pairs      |
| `into_js_value(self)`          | Convert to `JSValue<V>`  |
| `as_js_value(&self)`           | Borrow as `JSValue<V>`   |

### `JSArray<V>`

Represents JavaScript arrays. Wraps `JSObject<V>`.

```rust
pub struct JSArray<V: JSValueImpl>(JSObject<V>);
```

**Key Methods:**

| Method                 | Description                    |
| :---                   | :---                           |
| `new(ctx)`             | Create empty array             |
| `len()`                | Get array length               |
| `get<T>(index)`        | Get element at index           |
| `set<T>(index, value)` | Set element at index           |
| `push<T>(value)`       | Append element                 |
| `pop<T>()`             | Remove and return last element |
| `iter<T>()`            | Iterate over elements          |

### `JSFunc<V>`

Represents JavaScript functions.

**Key Methods:**

| Method                         | Description                       |
| :---                           | :---                              |
| `call<R, A>(this, args)`       | Call function synchronously       |
| `call_async<R, A>(this, args)` | Call and await if returns Promise |

### `JSDate<V>`

Represents JavaScript Date objects.

**Key Methods:**

| Method               | Description                        |
| :---                 | :---                               |
| `new(ctx, epoch_ms)` | Create from epoch milliseconds     |
| `now(ctx)`           | Create with current time           |
| `get_time()`         | Get epoch milliseconds             |
| `to_system_time()`   | Convert to `std::time::SystemTime` |

---

## Type Conversion

Rong provides seamless type conversion between Rust and JavaScript types through the `FromJSValue` and `IntoJSValue` traits. The conversion system is designed to be:

- **Type-safe**: All conversions are checked at compile time or return `JSResult` for runtime validation
- **Extensible**: Custom types can implement the conversion traits
- **Ergonomic**: Common conversions work automatically with minimal boilerplate

### Conversion Architecture

The conversion system has two layers:

1. **Low-level**: `TryInto<T>` / `From<(&Ctx, T)>` — Engine-specific, operates on raw `V`
2. **High-level**: `FromJSValue<V>` / `IntoJSValue<V>` — Application-level, operates on `JSValue<V>`

```
┌─────────────────────────────────────────────────────────────────────┐
│                          Rust Types                                 │
├─────────────────────────────────────────────────────────────────────┤
│  Primitives     │  Collections  │  Wrapper Types  │  Special Types  │
│  ───────────    │  ───────────  │  ────────────   │  ────────────   │
│  bool           │  Vec<T>       │  JSValue<V>     │  SystemTime     │
│  i8..i64        │               │  JSObject<V>    │  RongJSError    │
│  u8..u64        │               │  JSArray<V>     │  JSResult<T>    │
│  f64            │               │  JSFunc<V>      │                 │
│  String, &str   │               │  JSDate<V>      │                 │
│  ()             │               │  JSSymbol<V>    │                 │
│  Option<T>      │               │  Promise<V>     │                 │
└────────┬────────┴───────┬───────┴────────┬────────┴────────┬────────┘
         │                │                │                 │
         │   IntoJSValue  │                │                 │
         ▼                ▼                ▼                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                          JSValue<V>                                 │
├─────────────────────────────────────────────────────────────────────┤
│        .into_value() ───────────────────────────▶ V (raw value)     │
│        JSValue::from_raw(ctx, v) ◀─────────────── V (raw value)     │
└─────────────────────────────────────────────────────────────────────┘
         │                │                │                 │
         │   FromJSValue  │                │                 │
         ▼                ▼                ▼                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                       JavaScript Types                              │
├─────────────────────────────────────────────────────────────────────┤
│  Boolean │ Number │ String │ undefined │ null │ Array │ Object │... │
└─────────────────────────────────────────────────────────────────────┘
```

### Low-Level Traits: `TryInto` and `From`

At the engine level, raw values (`V`) implement standard Rust traits for primitive type conversions. These are the foundation that `FromJSValue` and `IntoJSValue` build upon.

| Trait                   | Direction | Description                         |
| :---                    | :---      | :---                                |
| `TryInto<T> for V`      | JS → Rust | Extract primitive from raw JS value |
| `From<(&Ctx, T)> for V` | Rust → JS | Create raw JS value from primitive  |

**Supported primitive types:** `bool`, `i32`, `u32`, `i64`, `u64`, `f64`, `String`

### How High-Level Traits Use Low-Level Traits

The `FromJSValue` and `IntoJSValue` traits delegate to these low-level implementations:

```rust
// FromJSValue uses TryInto internally
impl<V, T> FromJSValue<V> for T
where
    V: TryInto<T, Error = RongJSError>,  // ← requires low-level trait
    T: JSCompatible,
{
    fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        value.into_value().try_into()
        //    ^^^^^^^^^^^^  ^^^^^^^^^
        //    unwrap to V   use TryInto<T>
    }
}

// IntoJSValue uses From internally
impl<V, T> IntoJSValue<V> for T
where
    V: for<'a> From<(&'a V::Context, T)>,  // ← requires low-level trait
    T: JSCompatible,
{
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
        JSValue::from_raw(ctx, V::from((ctx.as_ref(), self)))
        //                     ^^^^^^^^^^^^^^^^^^^^^^^^^^
        //                     use From<(&Ctx, T)>
    }
}
```

### Trait Layers Summary

| Layer      | Direction   | Trait              | Input           | Output          |
| :---       | :---      | :---              | :---         | :---          |
| Low-level  | JS → Rust   | `TryInto<T>`       | `V` (raw)       | `Result<T>`     |
| Low-level  | Rust → JS   | `From<(&Ctx, T)>`  | `(&Ctx, T)`     | `V` (raw)       |
| High-level | JS → Rust   | `FromJSValue<V>`   | `JSValue<V>`    | `JSResult<T>`   |
| High-level | Rust → JS   | `IntoJSValue<V>`   | `T`             | `JSValue<V>`    |

> **Note**: The low-level `TryInto<T> for V` operates on raw engine value `V`.
> The `JSValue::try_into::<T>()` method is a high-level convenience API that uses `FromJSValue` internally.

### Rust → JavaScript (IntoJSValue)

The `IntoJSValue` trait converts Rust values into JavaScript values.

```rust
pub trait IntoJSValue<V: JSValueImpl> {
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V>;
}
```

#### Primitive Types

| Rust Type | JavaScript Type | Implementation              |
| :---      | :---            | :---                        |
| `bool`    | Boolean         | `JSCompatible` blanket impl |
| `i32`     | Number          | `JSCompatible` blanket impl |
| `u32`     | Number          | `JSCompatible` blanket impl |
| `i64`     | Number          | `JSCompatible` blanket impl |
| `u64`     | Number          | `JSCompatible` blanket impl |
| `f64`     | Number          | `JSCompatible` blanket impl |

```rust
// Implementation: blanket impl for JSCompatible types
impl<V, T> IntoJSValue<V> for T
where
    V: JSValueImpl,
    V: for<'a> From<(&'a V::Context, T)>,
    T: JSCompatible,  // marker trait for: i32, u32, i64, u64, f64, bool
{
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
        JSValue::from_raw(ctx, V::from((ctx.as_ref(), self)))
    }
}
```

#### Extended Integer Types

Smaller integer types are converted via intermediate types:

| Rust Type | Intermediate | JavaScript Type |
| :---      | :---         | :---            |
| `i8`      | → `i32`      | Number          |
| `i16`     | → `i32`      | Number          |
| `u8`      | → `u32`      | Number          |
| `u16`     | → `u32`      | Number          |
| `isize`   | → `i64`      | Number          |
| `usize`   | → `u64`      | Number          |

```rust
// Generated by impl_js_converter_for_int! macro
impl<V> IntoJSValue<V> for i8 {
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
        JSValue::from_raw(ctx, V::from((ctx.as_ref(), self as i32)))
    }
}
```

#### String Types

| Rust Type | JavaScript Type |
| :---      | :---            |
| `&str`    | String          |
| `String`  | String          |

```rust
impl<V> IntoJSValue<V> for &str {
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
        let raw = V::from((ctx.as_ref(), self));
        JSValue::from_raw(ctx, raw)
    }
}

impl<V> IntoJSValue<V> for String {
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
        let raw = V::from((ctx.as_ref(), self.as_str()));
        JSValue::from_raw(ctx, raw)
    }
}
```

#### Special Types

| Rust Type     | JavaScript Type  | Notes                            |
| :---          | :---             | :---                             |
| `()`          | `undefined`      | Unit type becomes undefined      |
| `Option<T>`   | `T` or `null`    | `None` → `null`, `Some(v)` → `v` |
| `Vec<T>`      | `Array`          | Each element converted           |
| `SystemTime`  | `Date`           | Converted via epoch milliseconds |
| `JSResult<T>` | `T` or Exception | `Ok(v)` → `v`, `Err(e)` → throws |
| `RongJSError` | Exception        | Throws JS exception              |

```rust
// Option<T>: None becomes null
impl<V, T> IntoJSValue<V> for Option<T>
where
    T: IntoJSValue<V>,
{
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
        match self {
            Some(value) => value.into_js_value(ctx),
            None => JSValue::from_raw(ctx, V::create_null(ctx.as_ref())),
        }
    }
}

// Vec<T>: creates JS Array with converted elements
impl<V, T> IntoJSValue<V> for Vec<T>
where
    V: JSObjectOps + JSArrayOps,
    T: IntoJSValue<V>,
{
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
        let array = JSArray::new(ctx).unwrap();
        for item in self {
            array.push(item).expect("Failed to set value in array");
        }
        array.into_js_value(ctx)
    }
}

// SystemTime: converts to JS Date via epoch milliseconds
impl<V: JSValueImpl> IntoJSValue<V> for SystemTime {
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
        let epoch_ms = self
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as f64;
        JSValue::from_raw(ctx, V::create_date(ctx.as_ref(), epoch_ms))
    }
}

// JSResult<T>: Ok converts value, Err throws exception
impl<V, T> IntoJSValue<V> for JSResult<T>
where
    T: IntoJSValue<V>,
{
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
        match self {
            Ok(value) => value.into_js_value(ctx),
            Err(err) => err.into_js_value(ctx), // throws exception
        }
    }
}
```

#### Wrapper Types (Passthrough)

These types already wrap JS values and simply unwrap:

| Rust Type     | JavaScript Type |
| :---          | :---            |
| `JSValue<V>`  | (any)           |
| `JSObject<V>` | Object          |
| `JSArray<V>`  | Array           |
| `JSFunc<V>`   | Function        |
| `JSDate<V>`   | Date            |
| `JSSymbol<V>` | Symbol          |
| `Promise<V>`  | Promise         |

```rust
// All wrapper types follow this pattern
impl<V: JSValueImpl> IntoJSValue<V> for JSObject<V> {
    fn into_js_value(self, _ctx: &JSContext<V::Context>) -> JSValue<V> {
        self.0  // unwrap inner JSValue
    }
}
```

### JavaScript → Rust (FromJSValue)

The `FromJSValue` trait converts JavaScript values into Rust types. All conversions are fallible and return `JSResult<T>`.

```rust
pub trait FromJSValue<V: JSValueImpl>: Sized {
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self>;
}
```

#### Primitive Types

| JavaScript Type | Rust Type                         | Error on Mismatch |
| :---            | :---                              | :---              |
| Boolean         | `bool`                            | TypeError         |
| Number          | `i32`, `u32`, `i64`, `u64`, `f64` | TypeError         |
| String          | `String`                          | TypeError         |
| any             | `()`                              | Never fails       |

```rust
// Blanket impl for JSCompatible types using TryInto
impl<V, T> FromJSValue<V> for T
where
    V: TryInto<T, Error = RongJSError>,
    T: JSCompatible,
{
    fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        value.into_value().try_into()  // uses engine's TryInto impl
    }
}

// String has explicit implementation
impl<V> FromJSValue<V> for String
where
    V: TryInto<String, Error = RongJSError>,
{
    fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        value.into_value().try_into()
    }
}

// Unit type always succeeds
impl<V: JSValueImpl> FromJSValue<V> for () {
    fn from_js_value(_ctx: &JSContext<V::Context>, _value: JSValue<V>) -> JSResult<Self> {
        Ok(())
    }
}
```

#### Extended Integer Types

| JavaScript Number | Rust Type | Conversion Path     |
| :---              | :---      | :---                |
| Number            | `i8`      | via `i32` then cast |
| Number            | `i16`     | via `i32` then cast |
| Number            | `u8`      | via `u32` then cast |
| Number            | `u16`     | via `u32` then cast |
| Number            | `isize`   | via `i64` then cast |
| Number            | `usize`   | via `u64` then cast |

```rust
// Generated by impl_js_converter_for_int! macro
impl<V> FromJSValue<V> for i8 {
    fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        let intermediate: i32 = value.into_value().try_into()?;
        Ok(intermediate as i8)
    }
}
```

#### Collection Types

| JavaScript Type | Rust Type | Requirements        |
| :---            | :---      | :---                |
| Array           | `Vec<T>`  | `T: FromJSValue<V>` |

```rust
impl<V, T> FromJSValue<V> for Vec<T>
where
    V: JSTypeOf + JSObjectOps + JSArrayOps,
    T: FromJSValue<V>,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        if value.is_array() {
            let array = JSArray::from_js_value(ctx, value)?;
            array
                .iter::<T>()?
                .enumerate()
                .map(|(i, item)| item.map_err(|e| e.at_index(i)))
                .collect::<JSResult<Vec<_>>>()
        } else {
            Err(HostError::type_mismatch("an array", value.type_of().to_string()).into())
        }
    }
}
```

#### Wrapper Types

Each wrapper type validates the JS type before wrapping. A refusal is a type
mismatch like a primitive's (`HostError::type_mismatch`), so as a parameter it
reads `argument 2 must be a function, got object` (`E_INVALID_ARG`) and as a
field it names the path (`` `items[1]` must be a string, got number ``):

| JavaScript Type | Rust Type     | Validation      |
| :---            | :---          | :---            |
| Object          | `JSObject<V>` | `is_object()`   |
| Array           | `JSArray<V>`  | `is_array()`    |
| Function        | `JSFunc<V>`   | `is_function()` |
| Date            | `JSDate<V>`   | `is_date()`     |
| Symbol          | `JSSymbol<V>` | `is_symbol()`   |
| Promise         | `Promise<V>`  | (via JSObject)  |
| any             | `JSValue<V>`  | Never fails     |

```rust
// JSObject: validates is_object()
impl<V: JSTypeOf> FromJSValue<V> for JSObject<V> {
    fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        if value.is_object() {
            Ok(value.into())
        } else {
            Err(HostError::type_mismatch("an object", value.type_of().to_string()).into())
        }
    }
}

// JSArray: validates is_array()
impl<V: JSTypeOf> FromJSValue<V> for JSArray<V> {
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        if value.is_array() {
            JSObject::from_js_value(ctx, value).map(Self)
        } else {
            Err(HostError::type_mismatch("an array", value.type_of().to_string()).into())
        }
    }
}

// JSDate: validates is_date()
impl<V: JSTypeOf> FromJSValue<V> for JSDate<V> {
    fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        if !value.is_date() {
            return Err(HostError::type_mismatch("a Date", value.type_of().to_string()).into());
        }
        Ok(JSDate { inner: value })
    }
}

// JSValue: passthrough, never fails
impl<V: JSValueImpl> FromJSValue<V> for JSValue<V> {
    fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        Ok(value)
    }
}
```

#### Special Types

| JavaScript Type    | Rust Type     | Notes                    |
| :---               | :---          | :---                     |
| Date               | `SystemTime`  | Calls `getTime()` method |
| Error/thrown value | `RongJSError` | Captures exception info  |

```rust
// SystemTime: extracts epoch ms from Date
impl<V> FromJSValue<V> for SystemTime
where
    V: JSTypeOf + JSValueConversion + JSObjectOps,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        let js_date = JSDate::from_js_value(ctx, value)?;
        js_date.to_system_time()  // calls getTime() internally
    }
}

// RongJSError: captures thrown value
impl<V: JSObjectOps> FromJSValue<V> for RongJSError {
    fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        Ok(RongJSError::from_thrown_value(value))
    }
}
```

### Complete Type Mapping Reference

#### Rust → JavaScript

| Rust Type      | JavaScript    | Conversion Method          |
| :---           | :---          | :---                       |
| `bool`         | Boolean       | `JSCompatible`             |
| `i8`, `i16`    | Number        | via `i32`                  |
| `i32`          | Number        | `JSCompatible`             |
| `i64`, `isize` | Number        | `JSCompatible` / via `i64` |
| `u8`, `u16`    | Number        | via `u32`                  |
| `u32`          | Number        | `JSCompatible`             |
| `u64`, `usize` | Number        | `JSCompatible` / via `u64` |
| `f64`          | Number        | `JSCompatible`             |
| `&str`         | String        | Direct                     |
| `String`       | String        | Direct                     |
| `()`           | undefined     | `V::create_undefined()`    |
| `Option<T>`    | T / null      | Recursive                  |
| `Vec<T>`       | Array         | Create + push each         |
| `SystemTime`   | Date          | `V::create_date(epoch_ms)` |
| `JSResult<T>`  | T / Exception | Ok/Err handling            |
| `RongJSError`  | Exception     | `throw_js_exception()`     |
| `JSValue<V>`   | (passthrough) | Identity                   |
| `JSObject<V>`  | Object        | Unwrap                     |
| `JSArray<V>`   | Array         | Unwrap                     |
| `JSFunc<V>`    | Function      | Unwrap                     |
| `JSDate<V>`    | Date          | Unwrap                     |
| `JSSymbol<V>`  | Symbol        | Unwrap                     |
| `Promise<V>`   | Promise       | Unwrap                     |

#### JavaScript → Rust

| JavaScript   | Rust Type                                  | Validation                     |
| :---         | :---                                       | :---                           |
| Boolean      | `bool`                                     | Type check                     |
| Number       | `i32`, `u32`, `i64`, `u64`, `f64`          | Type check                     |
| Number       | `i8`, `i16`, `u8`, `u16`, `isize`, `usize` | Type check + cast              |
| String       | `String`                                   | Type check                     |
| any          | `()`                                       | Always succeeds                |
| Array        | `Vec<T>`                                   | `is_array()` + element convert |
| Array        | `JSArray<V>`                               | `is_array()`                   |
| Object       | `JSObject<V>`                              | `is_object()`                  |
| Function     | `JSFunc<V>`                                | `is_function()`                |
| Date         | `JSDate<V>`                                | `is_date()`                    |
| Date         | `SystemTime`                               | `is_date()` + `getTime()`      |
| Symbol       | `JSSymbol<V>`                              | `is_symbol()`                  |
| Promise      | `Promise<V>`                               | via `JSObject`                 |
| any          | `JSValue<V>`                               | Always succeeds                |
| Error/thrown | `RongJSError`                              | Captures value                 |

---

## API Reference

### Creating Values

```rust
// Primitives
let num = 42_i32.into_js_value(&ctx);
let str = "hello".into_js_value(&ctx);
let flag = true.into_js_value(&ctx);

// Special values
let undef = JSValue::undefined(&ctx);
let null = JSValue::null(&ctx);

// Objects
let obj = JSObject::new(&ctx);
let obj = JSObject::from_json_string(&ctx, r#"{"key": "value"}"#)?;

// Arrays
let arr = JSArray::new(&ctx)?;
```

### Converting Values

```rust
// JS → Rust: using to_rust (most common pattern)
let value: JSValue<V> = /* ... */;

// Note: to_rust() consumes the JSValue, so clone() if you need to keep the original
// With type annotation
let s: String = value.clone().to_rust()?;

// With turbofish syntax
let s = value.clone().to_rust::<String>()?;
let n = value.clone().to_rust::<i32>()?;
let b = value.clone().to_rust::<bool>()?;

// Pattern matching (common in conditional logic)
if let Ok(s) = value.clone().to_rust::<String>() {
    println!("Got string: {}", s);
}

// If you don't need the value afterward, no clone needed
let msg: String = value.to_rust().unwrap_or_default();

// Using FromJSValue directly (less common)
let num = i32::from_js_value(&ctx, value)?;

// Rust → JS
let js_value = JSValue::from_rust(&ctx, 42);
let js_value = JSValue::from_rust(&ctx, "hello");
let js_value = true.into_js_value(&ctx);
```

### Working with Objects

```rust
let obj = JSObject::new(&ctx);

// Set properties
obj.set("name", "Alice")?;
obj.set("age", 30)?;

// Get properties
let name: String = obj.get("name")?;
let age: i32 = obj.get("age")?;

// Check and delete
if obj.has_property("temp")? {
    obj.delete("temp")?;
}

// Iterate
for key in obj.keys_as::<String>()? {
    println!("{}", key);
}
```

### Working with Arrays

```rust
let arr = JSArray::new(&ctx)?;

// Modify
arr.push(1)?;
arr.push(2)?;
arr.push(3)?;
arr.set(0, 100)?;

// Access
let first: i32 = arr.get(0)?.unwrap();
let len = arr.len();

// Iterate
for item in arr.iter::<i32>() {
    println!("{}", item?);
}

// Convert to Vec
let vec: Vec<i32> = Vec::from_js_value(&ctx, arr.into_js_value(&ctx))?;
```

### Type Checking

```rust
let value: JSValue<V> = /* ... */;

if value.is_string() {
    let s: String = value.to_rust()?;
}

match value.type_of() {
    JSValueType::Number => { /* ... */ }
    JSValueType::String => { /* ... */ }
    JSValueType::Object => { /* ... */ }
    JSValueType::Array => { /* ... */ }
    _ => { /* ... */ }
}
```

---

## Common Patterns

### Pattern 1: Wrapping Raw Values

When receiving raw values from engine APIs, wrap them with `JSValue::from_raw`:

```rust
let raw: V = engine_api_call();
let value = JSValue::from_raw(&ctx, raw);
```

### Pattern 2: Unwrapping for Engine APIs

When passing values to engine APIs that expect raw types:

```rust
let value: JSValue<V> = /* ... */;
let raw: V = value.into_value();
engine_api_call(raw);
```

### Pattern 3: Generic Function with Conversion

```rust
fn process_value<V, T>(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<T>
where
    V: JSValueImpl,
    T: FromJSValue<V>,
{
    T::from_js_value(ctx, value)
}
```

### Pattern 4: Returning Values to JavaScript

```rust
fn rust_function<V>(ctx: &JSContext<V::Context>) -> JSValue<V>
where
    V: JSValueImpl + for<'a> From<(&'a V::Context, i32)>,
{
    42.into_js_value(ctx)
}
```

### Pattern 5: Optional Properties

```rust
// Getting optional property
let maybe_value: Option<String> = obj.get("optional_key").ok();

// Setting optional value (None becomes null)
let opt: Option<i32> = Some(42);
obj.set("key", opt)?;
```

---

## Common Pitfalls

### 1. `to_rust()` Consumes the Value

The `to_rust()` method takes ownership of `JSValue`. If you need the value multiple times, clone it first:

```rust
// Wrong: value is consumed after first to_rust()
let s: String = value.to_rust()?;
let n: i32 = value.to_rust()?;  // Error: value already moved

// Correct: clone before consuming
let s: String = value.clone().to_rust()?;
let n: i32 = value.to_rust()?;  // OK, uses original
```

### 2. Integer Overflow is Silent

Smaller integer types (`i8`, `u8`, etc.) are converted via intermediate types and then cast. Overflow is not checked:

```rust
// JS number 300 → i8 will truncate
let big_number = JSValue::from_rust(&ctx, 300_i32);
let small: i8 = big_number.to_rust()?;  // Result: 44, not 300!
```

If you need overflow checking, convert to the intermediate type first and validate.

### 3. `null` vs `undefined`

These are distinct in JavaScript and map differently:

| Rust                | JavaScript  |
| :---                | :---        |
| `()`                | `undefined` |
| `Option::None`      | `null`      |
| `Option::Some(val)` | `val`       |

```rust
// To return undefined
fn returns_undefined<V>() -> JSValue<V> {
    ().into_js_value(&ctx)
}

// To return null
fn returns_null<V>() -> JSValue<V> {
    None::<i32>.into_js_value(&ctx)
}
```

### 4. Type Mismatch Errors

Conversion fails at runtime if the JavaScript type doesn't match:

```rust
let js_string = JSValue::from_rust(&ctx, "hello");
let num: Result<i32, _> = js_string.to_rust();  // Err(TypeError)
```

Use type checking before conversion if the type is uncertain:

```rust
if value.is_number() {
    let n: i32 = value.to_rust()?;
}
```

---

## See Also

- [Error Handling](./error_handling.md) - Error types and exception handling
