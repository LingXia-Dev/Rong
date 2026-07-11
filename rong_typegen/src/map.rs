//! Rust type → TypeScript type mapping.
//!
//! This is the single place that decides how a Rust type at the JS boundary
//! renders in `.d.ts`. It is deliberately conservative: types that cross the
//! boundary dynamically (`JSValue`, `JSObject`, `JSArray`, `JSFunc`) map to
//! broad TS types, and a caller supplies a precise type via a `ts_type` escape
//! hatch when it matters.

use syn::{GenericArgument, PathArguments, Type};

/// A mapped TS type plus whether the Rust type marked the value optional
/// (rong's `Optional<T>` wrapper, used for optional parameters).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsType {
    pub text: String,
    pub optional: bool,
}

impl TsType {
    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            optional: false,
        }
    }
}

/// Types injected by the runtime rather than passed from JS. Extractors drop
/// parameters of these types when building a signature.
pub fn is_injected(ty: &Type) -> bool {
    matches!(
        last_ident(ty).as_deref(),
        Some("JSContext" | "This" | "ThisMut")
    )
}

/// Map a Rust type used at the JS boundary to a TS type.
pub fn rust_type_to_ts(ty: &Type) -> TsType {
    match ty {
        Type::Reference(r) => rust_type_to_ts(&r.elem),
        Type::Paren(p) => rust_type_to_ts(&p.elem),
        Type::Group(g) => rust_type_to_ts(&g.elem),
        Type::Tuple(t) if t.elems.is_empty() => TsType::plain("void"),
        Type::Tuple(t) => {
            let inner: Vec<String> = t.elems.iter().map(|e| rust_type_to_ts(e).text).collect();
            TsType::plain(format!("[{}]", inner.join(", ")))
        }
        Type::Slice(s) => TsType::plain(array_of(&rust_type_to_ts(&s.elem).text)),
        Type::Path(_) => map_path(ty),
        _ => TsType::plain("any"),
    }
}

/// Map a return type: unwrap `JSResult<T>`, then wrap in `Promise<…>` if async.
pub fn map_return(ty: &Type, is_async: bool) -> String {
    let inner = match last_ident(ty).as_deref() {
        Some("JSResult") => generic_arg(ty, 0).map(|t| rust_type_to_ts(&t)),
        _ => Some(rust_type_to_ts(ty)),
    }
    .map(|t| t.text)
    .unwrap_or_else(|| "void".to_string());

    if is_async {
        format!("Promise<{inner}>")
    } else {
        inner
    }
}

fn map_path(ty: &Type) -> TsType {
    let Some(ident) = last_ident(ty) else {
        return TsType::plain("any");
    };

    match ident.as_str() {
        "String" | "str" | "char" | "PathBuf" | "Path" | "OsString" | "OsStr" => {
            TsType::plain("string")
        }
        "bool" => TsType::plain("boolean"),
        "i8" | "i16" | "i32" | "u8" | "u16" | "u32" | "f32" | "f64" | "usize" | "isize" | "i64"
        | "u64" | "i128" | "u128" | "NonZeroU32" | "NonZeroU64" => TsType::plain("number"),

        // Dynamic-boundary types — broad by design; refine with a ts_type hatch.
        "JSValue" | "CoreJSValue" => TsType::plain("any"),
        "JSObject" | "CoreJSObject" => TsType::plain("object"),
        "JSArray" | "CoreJSArray" => TsType::plain("any[]"),
        "JSFunc" | "CoreJSFunc" => TsType::plain("(...args: any[]) => any"),
        "JSArrayBuffer" | "CoreJSArrayBuffer" => TsType::plain("ArrayBuffer"),
        "JSBytes" | "JSTypedArray" | "AnyJSTypedArray" | "Uint8Clamped" => {
            TsType::plain("Uint8Array")
        }
        "JSDate" => TsType::plain("Date"),

        // A typed reference to a JS class instance — unwrap to the class type.
        "JSClassRef" => generic_arg(ty, 0)
            .map(|t| rust_type_to_ts(&t))
            .unwrap_or_else(|| TsType::plain("any")),

        // Transparent wrappers — unwrap.
        "JSResult" | "Box" | "Rc" | "Arc" | "RefCell" | "Cell" | "Ref" => generic_arg(ty, 0)
            .map(|t| rust_type_to_ts(&t))
            .unwrap_or_else(|| TsType::plain("any")),

        // rong's optional-parameter wrapper.
        "Optional" => {
            let inner = generic_arg(ty, 0)
                .map(|t| rust_type_to_ts(&t))
                .unwrap_or_else(|| TsType::plain("any"));
            TsType {
                text: inner.text,
                optional: true,
            }
        }

        // Nullable value.
        "Option" => {
            let inner = generic_arg(ty, 0)
                .map(|t| rust_type_to_ts(&t))
                .unwrap_or_else(|| TsType::plain("any"));
            TsType::plain(format!("{} | null", inner.text))
        }

        "Vec" | "VecDeque" | "HashSet" | "BTreeSet" => {
            let inner = generic_arg(ty, 0)
                .map(|t| rust_type_to_ts(&t))
                .unwrap_or_else(|| TsType::plain("any"));
            TsType::plain(array_of(&inner.text))
        }

        "HashMap" | "BTreeMap" => {
            let value = generic_arg(ty, 1)
                .map(|t| rust_type_to_ts(&t))
                .unwrap_or_else(|| TsType::plain("any"));
            TsType::plain(format!("Record<string, {}>", value.text))
        }

        // A custom, user-defined type: assume a TS interface/type of the same
        // name is (or will be) emitted alongside it. Keep generic arguments
        // (`Paginated<Item>`) rather than silently dropping them.
        other => match generic_args_ts(ty) {
            Some(args) => TsType::plain(format!("{other}<{args}>")),
            None => TsType::plain(other.to_string()),
        },
    }
}

/// `T[]`, parenthesizing the element when it is a union so precedence is
/// preserved (`(string | null)[]`, not `string | null[]`).
pub(crate) fn array_of(elem: &str) -> String {
    if elem.contains('|') || elem.contains("=>") {
        format!("({elem})[]")
    } else {
        format!("{elem}[]")
    }
}

/// Comma-joined TS mapping of all generic type arguments, or `None` if the type
/// has none.
fn generic_args_ts(ty: &Type) -> Option<String> {
    let Type::Path(p) = ty else { return None };
    let PathArguments::AngleBracketed(args) = &p.path.segments.last()?.arguments else {
        return None;
    };
    let mapped: Vec<String> = args
        .args
        .iter()
        .filter_map(|a| match a {
            GenericArgument::Type(t) => Some(rust_type_to_ts(t).text),
            _ => None,
        })
        .collect();
    if mapped.is_empty() {
        None
    } else {
        Some(mapped.join(", "))
    }
}

/// The identifier of a path type's final segment.
fn last_ident(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        Type::Reference(r) => last_ident(&r.elem),
        Type::Paren(p) => last_ident(&p.elem),
        Type::Group(g) => last_ident(&g.elem),
        _ => None,
    }
}

/// The `n`th generic type argument of a path type, if present.
fn generic_arg(ty: &Type, n: usize) -> Option<Type> {
    let Type::Path(p) = ty else { return None };
    let seg = p.path.segments.last()?;
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    args.args
        .iter()
        .filter_map(|a| match a {
            GenericArgument::Type(t) => Some(t.clone()),
            _ => None,
        })
        .nth(n)
}
