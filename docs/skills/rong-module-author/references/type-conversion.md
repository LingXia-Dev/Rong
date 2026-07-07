# Value system & type conversion

Rong bridges Rust and JS through `FromJSValue` (JS -> Rust) and `IntoJSValue`
(Rust -> JS), plus thin wrapper types. Conversions in function/method signatures
happen automatically.

## Wrapper types

| Type            | Wraps         | Represents                          |
| :---            | :---          | :---                                |
| `JSValue<V>`    | raw `V`       | any JS value (with context)         |
| `JSObject<V>`   | `JSValue`     | JS object                           |
| `JSArray<V>`    | `JSObject`    | JS array                            |
| `JSFunc<V>`     | `JSObject`    | JS function                         |
| `JSDate<V>`     | `JSValue`     | Date                                |
| `JSSymbol<V>`   | `JSValue`     | Symbol                              |
| `JSException<V>`| `JSObject`    | thrown/rejected payload (often Error) |

In module code you usually write the engine-erased aliases (`JSValue`,
`JSObject`, `JSContext`, ...); the generic `V` is the engine value type.

## Rust -> JavaScript

| Rust                       | JavaScript     |
| :---                       | :---           |
| `bool`                     | Boolean        |
| `i8/i16/i32/i64/isize`     | Number         |
| `u8/u16/u32/u64/usize`     | Number         |
| `f64`                      | Number         |
| `&str`, `String`           | String         |
| `()`                       | undefined      |
| `Option<T>`                | `T` or `null`  |
| `Vec<T>`                   | Array          |
| `SystemTime`               | Date           |
| `JSResult<T>`              | `T` or thrown  |
| `RongJSError`              | thrown         |
| `JSValue/JSObject/JSArray/JSFunc/JSDate/JSSymbol/Promise` | passthrough/unwrap |

## JavaScript -> Rust

| JavaScript   | Rust                                       | Validation                     |
| :---         | :---                                       | :---                           |
| Boolean      | `bool`                                     | type check                     |
| Number       | `i32/u32/i64/u64/f64` (and smaller via cast) | type check                   |
| String       | `String`                                   | type check                     |
| any          | `()`                                        | always succeeds                |
| Array        | `Vec<T>` / `JSArray`                        | `is_array()` (+ element convert) |
| Object       | `JSObject`                                  | `is_object()`                  |
| Function     | `JSFunc`                                    | `is_function()`                |
| Date         | `JSDate` / `SystemTime`                     | `is_date()`                    |
| Symbol       | `JSSymbol`                                  | `is_symbol()`                  |
| Promise      | `Promise`                                   | via `JSObject`                 |
| any          | `JSValue`                                   | always succeeds                |
| Error/thrown | `RongJSError`                               | captures the value             |

For public object-shaped API data, use `#[derive(FromJSObj)]` /
`#[derive(IntoJSObj)]` with `#[rename = "jsName"]` (see `classes.md`). Avoid
manual `JSObject` parsing/building for stable API shapes; the macro/derive path
keeps runtime conversion and generated TypeScript tied to the same Rust source.
Use field-level `#[ts_type = "..."]` for intentional TypeScript precision that
cannot be inferred from the Rust conversion type. Use struct-level `#[ts_skip]`
for internal derived parser structs that must not become public TypeScript.

## Explicit conversions

```rust
// Rust -> JS
let js = JSValue::from_rust(&ctx, 42);
let js = true.into_js_value(&ctx);

// JS -> Rust
let n: i32 = js_value.to_rust()?;
let s: String = js_value.clone().try_into::<String>()?; // probing form (returns Result)
```

## Objects and arrays

Use raw `JSObject` / `JSArray` for dynamic dictionaries, arbitrary JS values,
internal glue, or cases where the field set is not part of the public contract.
For named public option/result shapes, prefer derive structs instead.

```rust
let obj = JSObject::new(&ctx);
obj.set("name", "Alice")?;
let name: String = obj.get("name")?;
for (k, v) in obj.entries_as::<String, String>()? { /* ... */ }

let arr = JSArray::new(&ctx)?;
arr.push(1)?;
let first: i32 = arr.get(0)?.unwrap();         // get returns Option (holes -> None)
for item in arr.iter::<JSValue>() { let v = item?; }
```

## Type-checking helpers

`value.is_string()`, `is_array_buffer()`, `is_undefined()`, `is_null()`,
`is_object()`, `is_array()`, `is_function()`, `is_date()`, `is_symbol()`,
`is_exception()`; `value.into_object() -> Option<JSObject>`;
`obj.borrow::<T>()` to recover a native Rust struct from a JS object;
`JSArray::from_object(obj)`, `JSTypedArray::from_object(obj)`.
