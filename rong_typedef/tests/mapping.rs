//! Rust→TS mapping coverage.

use rong_typedef::map::{is_injected, map_return, rust_type_to_ts};

fn ts(src: &str) -> String {
    let ty: syn::Type = syn::parse_str(src).unwrap();
    rust_type_to_ts(&ty).text
}

fn optional(src: &str) -> bool {
    let ty: syn::Type = syn::parse_str(src).unwrap();
    rust_type_to_ts(&ty).optional
}

fn ret(src: &str, is_async: bool) -> String {
    let ty: syn::Type = syn::parse_str(src).unwrap();
    map_return(&ty, is_async)
}

#[test]
fn primitives() {
    assert_eq!(ts("String"), "string");
    assert_eq!(ts("&str"), "string");
    assert_eq!(ts("bool"), "boolean");
    assert_eq!(ts("u32"), "number");
    assert_eq!(ts("i64"), "number");
    assert_eq!(ts("f64"), "number");
    assert_eq!(ts("PathBuf"), "string");
    assert_eq!(ts("()"), "void");
}

#[test]
fn dynamic_boundary_types() {
    assert_eq!(ts("JSValue"), "any");
    assert_eq!(ts("JSObject"), "object");
    assert_eq!(ts("JSArray"), "any[]");
    assert_eq!(ts("JSFunc"), "(...args: any[]) => any");
    assert_eq!(ts("JSArrayBuffer"), "ArrayBuffer");
    assert_eq!(ts("JSBytes"), "Uint8Array");
}

#[test]
fn containers() {
    assert_eq!(ts("Vec<String>"), "string[]");
    assert_eq!(ts("Vec<u8>"), "number[]");
    assert_eq!(ts("Option<String>"), "string | null");
    assert_eq!(ts("HashMap<String, f64>"), "Record<string, number>");
    assert_eq!(ts("BTreeMap<String, JSValue>"), "Record<string, any>");
    assert_eq!(ts("&[u8]"), "number[]");
}

#[test]
fn transparent_wrappers_unwrap() {
    assert_eq!(ts("Box<String>"), "string");
    assert_eq!(ts("Rc<RefCell<u32>>"), "number");
    assert_eq!(ts("Arc<Vec<String>>"), "string[]");
    // A typed class-instance reference unwraps to the class type.
    assert_eq!(ts("JSClassRef<WritableStream>"), "WritableStream");
}

#[test]
fn optional_wrapper_marks_param_optional() {
    assert_eq!(ts("Optional<String>"), "string");
    assert!(optional("Optional<String>"));
    assert!(!optional("String"));
    // Nested: optionality is preserved through the wrapper.
    assert_eq!(ts("Optional<Vec<String>>"), "string[]");
    assert!(optional("Optional<Vec<String>>"));
}

#[test]
fn union_element_is_parenthesized_in_arrays() {
    assert_eq!(ts("Vec<Option<String>>"), "(string | null)[]");
}

#[test]
fn custom_types_pass_through_by_name() {
    assert_eq!(ts("RunResult"), "RunResult");
    assert_eq!(ts("Vec<DirEntry>"), "DirEntry[]");
    assert_eq!(ts("Option<StorageInfo>"), "StorageInfo | null");
}

#[test]
fn custom_generic_types_keep_their_args() {
    assert_eq!(ts("Paginated<Item>"), "Paginated<Item>");
    assert_eq!(ts("Wrapper<String, u32>"), "Wrapper<string, number>");
}

#[test]
fn return_unwraps_jsresult_and_wraps_async() {
    assert_eq!(ret("JSResult<()>", false), "void");
    assert_eq!(ret("JSResult<String>", false), "string");
    assert_eq!(ret("JSResult<JSArray>", false), "any[]");
    assert_eq!(ret("String", false), "string");
    // async wraps the (unwrapped) result in a Promise.
    assert_eq!(ret("JSResult<String>", true), "Promise<string>");
    assert_eq!(ret("JSResult<()>", true), "Promise<void>");
}

#[test]
fn injected_params_are_recognized() {
    let ctx: syn::Type = syn::parse_str("JSContext").unwrap();
    let this: syn::Type = syn::parse_str("This<SQLite>").unwrap();
    let real: syn::Type = syn::parse_str("String").unwrap();
    assert!(is_injected(&ctx));
    assert!(is_injected(&this));
    assert!(!is_injected(&real));
}
