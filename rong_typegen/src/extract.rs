//! Extract descriptors from parsed Rust source.
//!
//! The generator parses a crate's `.rs` files with `syn` and feeds each item
//! here. [`extract_impl`] turns a `#[js_class] impl` into a class descriptor;
//! [`extract_struct`] turns a `#[derive(FromJSObject)]` / `IntoJSObject` struct into
//! an interface. Everything is pure AST → descriptor, so it is unit-testable
//! and shared by any crate's generation (rong's modules or `lingxia-logic`).

use crate::map::{array_of, is_injected, map_return, rust_type_to_ts};
use crate::model::{
    ClassDef, Field, FnSig, FunctionDef, InterfaceDef, Item, Member, MemberKind, NumericEnumDef,
    NumericEnumVariant, Param, TypeAliasDef,
};
use crate::{
    JsFieldOptions, JsMethodOptions, js_class_options, js_field_options, js_method_options,
    u32_integer_expr, validate_js_method_signature,
};
use std::collections::{HashMap, HashSet};
use syn::{
    Attribute, Expr, FnArg, ImplItem, ImplItemFn, ItemEnum, ItemFn, ItemImpl, ItemStruct, Lit,
    Meta, Pat, ReturnType, Signature, Type,
};

/// Extract a namespace function signature from a registered Rust function.
pub fn extract_function(
    input: &ItemFn,
    name: String,
    ts_params: Option<String>,
    ts_return: Option<String>,
) -> FunctionDef {
    let is_async = input.sig.asyncness.is_some();
    let ret = overridden_return(ts_return.as_deref(), is_async)
        .unwrap_or_else(|| return_ts(&input.sig, is_async, None));
    FunctionDef {
        name,
        docs: doc_lines(&input.attrs),
        sig: FnSig {
            params: params(&input.sig),
            ret,
            raw_params: ts_params,
        },
    }
}

/// Extract a class descriptor from an `impl` block, if it carries `#[js_class]`.
pub fn extract_impl(input: &ItemImpl) -> syn::Result<Option<Item>> {
    let Some(class_options) = js_class_options(&input.attrs)? else {
        return Ok(None);
    };
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "#[js_class] does not support generic impl blocks",
        ));
    }
    let Some(rust_name) = type_name(&input.self_ty) else {
        return Ok(None);
    };
    let name = class_options.rename.unwrap_or_else(|| rust_name.clone());
    if name.is_empty() {
        return Ok(None); // e.g. a `#[js_class]` on an unnamed/complex self type
    }

    let mut constructor = None;
    let mut constructor_docs = Vec::new();
    let mut private_constructor = false;
    let mut gc_mark_seen = false;
    let mut members = Vec::new();

    // Parse each method's options once.
    let parsed: Vec<(&ImplItemFn, JsMethodOptions)> = fns(input)
        .map(|method| {
            Ok((
                method,
                js_method_options(&method.attrs)?.expect("filtered method"),
            ))
        })
        .collect::<syn::Result<_>>()?;
    let mut js_members = HashMap::<(bool, String), (bool, bool, bool)>::new();
    for (method, options) in &parsed {
        if options.constructor || options.gc_mark {
            continue;
        }
        let key = (method.sig.receiver().is_none(), js_name(method, options));
        let seen = js_members.entry(key.clone()).or_default();
        let duplicate = if options.getter {
            let duplicate = seen.0 || seen.2;
            seen.0 = true;
            duplicate
        } else if options.setter {
            let duplicate = seen.1 || seen.2;
            seen.1 = true;
            duplicate
        } else {
            let duplicate = seen.0 || seen.1 || seen.2;
            seen.2 = true;
            duplicate
        };
        if duplicate {
            return Err(syn::Error::new_spanned(
                method,
                format!("duplicate JavaScript class member `{}`", key.1),
            ));
        }
    }
    // A getter that also has a setter is a read/write property, not readonly.
    let setter_names: HashSet<(bool, String)> = parsed
        .iter()
        .filter(|(_, o)| o.setter)
        .map(|(m, o)| (m.sig.receiver().is_none(), js_name(m, o)))
        .collect();
    let getter_names: HashSet<(bool, String)> = parsed
        .iter()
        .filter(|(_, o)| o.getter)
        .map(|(m, o)| (m.sig.receiver().is_none(), js_name(m, o)))
        .collect();

    for (m, opts) in &parsed {
        let m = *m;
        validate_js_method_signature(m, opts)?;
        if opts.gc_mark {
            if gc_mark_seen {
                return Err(syn::Error::new_spanned(
                    m,
                    "a js_class cannot declare more than one gc_mark method",
                ));
            }
            gc_mark_seen = true;
            continue;
        }
        let is_async = m.sig.asyncness.is_some();
        let member_name = js_name(m, opts);

        if opts.constructor {
            if constructor.is_some() || private_constructor {
                return Err(syn::Error::new_spanned(
                    m,
                    "a js_class cannot declare more than one constructor",
                ));
            }
            constructor_docs = doc_lines(&m.attrs);
            if opts.private {
                private_constructor = true;
            } else {
                constructor = Some(FnSig {
                    params: params(&m.sig),
                    ret: String::new(),
                    raw_params: opts.ts_params.clone(),
                });
            }
            continue;
        }
        let member = if opts.getter {
            let is_static = m.sig.receiver().is_none();
            let kind = match (
                is_static,
                setter_names.contains(&(is_static, member_name.clone())),
            ) {
                (false, false) => MemberKind::Getter,
                (false, true) => MemberKind::Property,
                (true, false) => MemberKind::StaticGetter,
                (true, true) => MemberKind::StaticProperty,
            };
            Member {
                kind,
                name: member_name,
                docs: doc_lines(&m.attrs),
                sig: make_sig(m, opts, is_async, false, &name),
            }
        } else if opts.setter {
            let is_static = m.sig.receiver().is_none();
            if getter_names.contains(&(is_static, member_name.clone())) {
                continue; // represented by the matching getter's property
            }
            Member {
                kind: if is_static {
                    MemberKind::StaticSetter
                } else {
                    MemberKind::Setter
                },
                name: member_name,
                docs: doc_lines(&m.attrs),
                sig: make_sig(m, opts, is_async, true, &name),
            }
        } else {
            let kind = if m.sig.receiver().is_some() {
                MemberKind::Method
            } else {
                MemberKind::StaticMethod
            };
            Member {
                kind,
                name: member_name,
                docs: doc_lines(&m.attrs),
                sig: make_sig(m, opts, is_async, true, &name),
            }
        };
        members.push(member);
    }

    Ok(Some(Item::Class(ClassDef {
        rust_name,
        name,
        docs: doc_lines(&input.attrs),
        constructor,
        constructor_docs,
        private_constructor,
        members,
    })))
}

/// True if this impl carries `#[js_method]` fns but no `#[js_class]`, so its
/// methods would be silently dropped. Callers can warn.
pub fn has_orphan_js_methods(input: &ItemImpl) -> bool {
    !input
        .attrs
        .iter()
        .any(|attr| path_last_is(attr.path(), "js_class"))
        && fns(input).next().is_some()
}

/// Extract an interface from a struct that derives `FromJSObject` or `IntoJSObject`.
pub fn extract_struct(input: &ItemStruct) -> syn::Result<Option<Item>> {
    let struct_options = js_field_options(&input.attrs)?;
    if !derives_js_obj(&input.attrs) || struct_options.ts_skip {
        return Ok(None);
    }
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "FromJSObject and IntoJSObject do not support generic structs",
        ));
    }
    let syn::Fields::Named(named) = &input.fields else {
        return Ok(None);
    };

    let fields = named
        .named
        .iter()
        .map(|f| -> syn::Result<Field> {
            let options = js_field_options(&f.attrs)?;
            if options.ts_skip {
                return Err(syn::Error::new_spanned(
                    f,
                    "ts_skip is only valid on a derived struct",
                ));
            }
            let rust_name = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
            let (ts_type, optional) = field_ts(&options, &f.ty);
            Ok(Field {
                name: options.js_name.unwrap_or(rust_name),
                ts_type,
                optional,
                readonly: false,
                docs: doc_lines(&f.attrs),
            })
        })
        .collect::<syn::Result<_>>()?;

    Ok(Some(Item::Interface(InterfaceDef {
        name: input.ident.to_string(),
        docs: doc_lines(&input.attrs),
        fields,
    })))
}

/// Extract a numeric constant enum from `#[js_numeric_enum]`.
pub fn extract_numeric_enum(input: &ItemEnum) -> syn::Result<Option<Item>> {
    if !has_ident_attr(&input.attrs, "js_numeric_enum") {
        return Ok(None);
    }

    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "js_numeric_enum does not support generic enums",
        ));
    }

    let variants = input
        .variants
        .iter()
        .map(|v| -> syn::Result<NumericEnumVariant> {
            if !matches!(v.fields, syn::Fields::Unit) {
                return Err(syn::Error::new_spanned(
                    &v.fields,
                    "js_numeric_enum only supports unit variants",
                ));
            }
            let (_, expr) = v.discriminant.as_ref().ok_or_else(|| {
                syn::Error::new_spanned(
                    v,
                    "js_numeric_enum variants must have explicit integer values",
                )
            })?;
            Ok(NumericEnumVariant {
                name: v.ident.to_string(),
                value: u32_integer_expr(expr)?,
                docs: doc_lines(&v.attrs),
            })
        })
        .collect::<syn::Result<_>>()?;

    Ok(Some(Item::NumericEnum(NumericEnumDef {
        name: input.ident.to_string(),
        docs: doc_lines(&input.attrs),
        variants,
    })))
}

/// Extract an untagged `#[js_union]` enum as a TypeScript union alias.
pub fn extract_union(input: &ItemEnum) -> syn::Result<Option<Item>> {
    if !has_ident_attr(&input.attrs, "js_union") {
        return Ok(None);
    }
    if has_ident_attr(&input.attrs, "js_numeric_enum") {
        return Err(syn::Error::new_spanned(
            input,
            "an enum cannot be both #[js_union] and #[js_numeric_enum]",
        ));
    }
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "#[js_union] does not support generic enums",
        ));
    }

    let variants = input
        .variants
        .iter()
        .map(|variant| match &variant.fields {
            syn::Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                Ok(rust_type_to_ts(&fields.unnamed[0].ty).text)
            }
            _ => Err(syn::Error::new_spanned(
                &variant.fields,
                "#[js_union] variants must contain exactly one unnamed field",
            )),
        })
        .collect::<syn::Result<Vec<_>>>()?;

    Ok(Some(Item::TypeAlias(TypeAliasDef {
        name: input.ident.to_string(),
        ts_type: variants.join(" | "),
        docs: doc_lines(&input.attrs),
    })))
}

// ---- class helpers ----

fn fns(input: &ItemImpl) -> impl Iterator<Item = &ImplItemFn> {
    input.items.iter().filter_map(|it| match it {
        ImplItem::Fn(f) if has_ident_attr(&f.attrs, "js_method") => Some(f),
        _ => None,
    })
}

fn js_name(m: &ImplItemFn, opts: &JsMethodOptions) -> String {
    opts.rename
        .clone()
        .unwrap_or_else(|| m.sig.ident.to_string())
}

/// Build a signature, applying `ts_return`/`ts_params` overrides when present.
fn make_sig(
    m: &ImplItemFn,
    opts: &JsMethodOptions,
    is_async: bool,
    include_params: bool,
    self_name: &str,
) -> FnSig {
    let params = if include_params {
        params(&m.sig)
    } else {
        vec![]
    };
    // A `ts_return` hatch gives the *inner* type; async wrapping still applies,
    // so an async method with `ts_return = "T"` renders `Promise<T>`. Don't
    // double-wrap if a hatch already spelled out `Promise<…>`.
    let ret = overridden_return(opts.ts_return.as_deref(), is_async)
        .unwrap_or_else(|| return_ts(&m.sig, is_async, Some(self_name)));
    FnSig {
        params,
        ret,
        raw_params: opts.ts_params.clone(),
    }
}

/// JS-visible parameters: drop the receiver and runtime-injected params.
fn params(sig: &Signature) -> Vec<Param> {
    sig.inputs
        .iter()
        .enumerate()
        .filter_map(|(i, arg)| match arg {
            FnArg::Receiver(_) => None,
            FnArg::Typed(pt) if is_injected(&pt.ty) => None,
            // `Rest<T>` is a variadic parameter: `...name: T[]`.
            FnArg::Typed(pt) if last_ident_is(&pt.ty, "Rest") => {
                let inner = generic_arg0(&pt.ty)
                    .map(|t| rust_type_to_ts(&t).text)
                    .unwrap_or_else(|| "any".to_string());
                Some(Param {
                    name: pat_name(&pt.pat, i),
                    ts_type: array_of(&inner),
                    optional: false,
                    rest: true,
                })
            }
            FnArg::Typed(pt) => {
                let mapped = rust_type_to_ts(&pt.ty);
                Some(Param {
                    name: pat_name(&pt.pat, i),
                    ts_type: mapped.text,
                    optional: mapped.optional,
                    rest: false,
                })
            }
        })
        .collect()
}

/// Parameter name, or `argN` (positional) for non-identifier patterns like
/// `_` or destructures, so two such params never collide.
fn pat_name(pat: &Pat, index: usize) -> String {
    match pat {
        Pat::Ident(i) => i.ident.to_string(),
        _ => format!("arg{index}"),
    }
}

fn return_ts(sig: &Signature, is_async: bool, self_name: Option<&str>) -> String {
    let ret = match &sig.output {
        ReturnType::Default => {
            if is_async {
                "Promise<void>".to_string()
            } else {
                "void".to_string()
            }
        }
        ReturnType::Type(_, ty) => map_return(ty, is_async),
    };
    match self_name {
        Some(self_name) => replace_ident(&ret, "Self", self_name),
        None => ret,
    }
}

fn overridden_return(ts_return: Option<&str>, is_async: bool) -> Option<String> {
    match ts_return {
        Some(value) if is_async && !value.trim_start().starts_with("Promise<") => {
            Some(format!("Promise<{value}>"))
        }
        Some(value) => Some(value.to_string()),
        None => None,
    }
}

/// Replace whole-identifier occurrences of `ident` in `text` with `to`. Unlike
/// `str::replace`, it will not touch `ident` embedded in a longer identifier.
fn replace_ident(text: &str, ident: &str, to: &str) -> String {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find(ident) {
        let before_ok = rest[..pos].chars().next_back().is_none_or(|c| !is_word(c));
        let after = &rest[pos + ident.len()..];
        let after_ok = after.chars().next().is_none_or(|c| !is_word(c));
        out.push_str(&rest[..pos]);
        if before_ok && after_ok {
            out.push_str(to);
        } else {
            out.push_str(ident);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

fn type_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

// ---- struct helpers ----

fn derives_js_obj(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("derive")
            && a.parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
            )
            .map(|paths| {
                paths
                    .iter()
                    .any(|p| path_last_is(p, "FromJSObject") || path_last_is(p, "IntoJSObject"))
            })
            .unwrap_or(false)
    })
}

/// A struct field's TS type and optionality: `Option<T>`/`Optional<T>` render as
/// an optional field of `T` (not `T | null`).
fn field_ts(options: &JsFieldOptions, ty: &Type) -> (String, bool) {
    let explicit = options.ts_type.clone();
    if last_ident_is(ty, "Option")
        && let Some(inner) = generic_arg0(ty)
    {
        return (
            explicit.unwrap_or_else(|| rust_type_to_ts(&inner).text),
            true,
        );
    }
    (
        explicit.unwrap_or_else(|| rust_type_to_ts(ty).text),
        options.default.is_some(),
    )
}

// ---- shared helpers ----

fn has_ident_attr(attrs: &[Attribute], ident: &str) -> bool {
    attrs.iter().any(|a| path_last_is(a.path(), ident))
}

/// Match an attribute/derive by its final path segment, so a qualified form
/// like `#[rong::js_class]` or `#[derive(rong::FromJSObject)]` still matches.
fn path_last_is(path: &syn::Path, name: &str) -> bool {
    path.segments.last().is_some_and(|s| s.ident == name)
}

fn str_lit(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(e) => match &e.lit {
            Lit::Str(s) => Some(s.value()),
            _ => None,
        },
        _ => None,
    }
}

fn last_ident_is(ty: &Type, name: &str) -> bool {
    matches!(ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == name))
}

fn generic_arg0(ty: &Type) -> Option<Type> {
    let Type::Path(p) = ty else { return None };
    let seg = p.path.segments.last()?;
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    args.args.iter().find_map(|a| match a {
        syn::GenericArgument::Type(t) => Some(t.clone()),
        _ => None,
    })
}

/// Collect `///` doc lines (`#[doc = "..."]`) as trimmed strings.
fn doc_lines(attrs: &[Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .filter_map(|a| match &a.meta {
            Meta::NameValue(nv) => str_lit(&nv.value).map(|s| s.trim().to_string()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(input: syn::ItemImpl) -> ClassDef {
        match extract_impl(&input).unwrap().unwrap() {
            Item::Class(c) => c,
            _ => panic!("expected class"),
        }
    }

    #[test]
    fn only_js_class_impls_are_extracted() {
        // No #[js_class] -> not extracted.
        let plain: syn::ItemImpl = syn::parse_quote! { impl Foo { fn bar(&self) {} } };
        assert!(extract_impl(&plain).unwrap().is_none());
    }

    #[test]
    fn extracts_methods_getters_and_constructor() {
        let c = class(syn::parse_quote! {
            /// A database.
            #[js_class]
            impl Db {
                #[js_method(constructor)]
                fn new(path: Optional<String>) -> JSResult<Self> { unimplemented!() }

                /// Run SQL.
                #[js_method]
                fn exec(&self, ctx: JSContext, sql: String) -> JSResult<()> { unimplemented!() }

                #[js_method(getter, rename = "inTransaction")]
                fn in_transaction(&self) -> JSResult<bool> { unimplemented!() }

                #[js_method(gc_mark)]
                fn gc(&self, _f: F) {}
            }
        });

        assert_eq!(c.name, "Db");
        assert_eq!(c.docs, vec!["A database.".to_string()]);

        let ctor = c.constructor.unwrap();
        assert_eq!(ctor.params.len(), 1);
        assert!(ctor.params[0].optional);
        assert_eq!(ctor.params[0].ts_type, "string");

        assert_eq!(c.members.len(), 2); // gc_mark skipped

        let exec = &c.members[0];
        assert_eq!(exec.kind, MemberKind::Method);
        assert_eq!(exec.sig.params.len(), 1); // ctx: JSContext dropped
        assert_eq!(exec.sig.params[0].name, "sql");
        assert_eq!(exec.sig.ret, "void");
        assert_eq!(exec.docs, vec!["Run SQL.".to_string()]);

        let getter = &c.members[1];
        assert_eq!(getter.kind, MemberKind::Getter);
        assert_eq!(getter.name, "inTransaction");
        assert_eq!(getter.sig.ret, "boolean");
    }

    #[test]
    fn self_returns_become_the_class_name() {
        let c = class(syn::parse_quote! {
            #[js_class]
            impl Blob {
                #[js_method]
                fn slice(&self, start: Optional<u32>) -> JSResult<Self> { unimplemented!() }
                #[js_method]
                fn error() -> Self { unimplemented!() }
            }
        });
        assert_eq!(c.members[0].sig.ret, "Blob");
        assert_eq!(c.members[1].sig.ret, "Blob");
        assert_eq!(c.members[1].kind, MemberKind::StaticMethod);
    }

    #[test]
    fn preserves_static_and_setter_only_property_kinds() {
        let c = class(syn::parse_quote! {
            #[js_class]
            impl Config {
                #[js_method(getter, rename = "current")]
                fn current() -> Self { unimplemented!() }

                #[js_method(setter, rename = "current")]
                fn set_current(value: Self) { unimplemented!() }

                #[js_method(setter, rename = "token")]
                fn set_token(&mut self, value: String) { unimplemented!() }
            }
        });
        assert_eq!(c.members[0].kind, MemberKind::StaticProperty);
        assert_eq!(c.members[1].kind, MemberKind::Setter);
        assert_eq!(c.members[1].sig.params[0].ts_type, "string");
    }

    #[test]
    fn explicit_private_constructor_becomes_private() {
        let c = class(syn::parse_quote! {
            #[js_class]
            impl Statement {
                #[js_method(constructor, private)]
                fn new() -> JSResult<Self> {
                    rong::illegal_constructor("use db.prepare(sql)")
                }
                #[js_method]
                fn finalize(&self) -> JSResult<()> { unimplemented!() }
            }
        });
        assert!(c.private_constructor);
        assert!(c.constructor.is_none());
    }

    #[test]
    fn ts_return_on_async_method_is_promise_wrapped() {
        let c = class(syn::parse_quote! {
            #[js_class]
            impl S3File {
                // sync + hatch: verbatim.
                #[js_method(ts_return = "S3File")]
                fn slice(&self) -> JSResult<JSObject> { unimplemented!() }
                // async + hatch: wrapped in Promise.
                #[js_method(ts_return = "S3StatResult")]
                async fn stat(&self) -> JSResult<JSObject> { unimplemented!() }
            }
        });
        assert_eq!(c.members[0].sig.ret, "S3File");
        assert_eq!(c.members[1].sig.ret, "Promise<S3StatResult>");
    }

    #[test]
    fn self_replacement_is_whole_word_only() {
        // A return type that merely contains "Self" as a substring is untouched.
        let c = class(syn::parse_quote! {
            #[js_class]
            impl Blob {
                #[js_method]
                fn a(&self) -> JSResult<Self> { unimplemented!() }
                #[js_method]
                fn b(&self) -> JSResult<SelfTest> { unimplemented!() }
            }
        });
        assert_eq!(c.members[0].sig.ret, "Blob");
        assert_eq!(c.members[1].sig.ret, "SelfTest"); // NOT "BlobTest"
    }

    #[test]
    fn qualified_js_class_is_recognized() {
        let c = extract_impl(&syn::parse_quote! {
            #[rong::js_class]
            impl Foo {
                #[rong::js_method]
                fn go(&self) -> JSResult<()> { unimplemented!() }
            }
        })
        .unwrap();
        assert!(matches!(c, Some(Item::Class(_))));
    }

    #[test]
    fn rest_param_is_variadic() {
        let c = class(syn::parse_quote! {
            #[js_class]
            impl Console {
                #[js_method]
                fn log(&self, ctx: JSContext, args: Rest<JSValue>) -> JSResult<()> { unimplemented!() }
            }
        });
        let log = &c.members[0];
        assert_eq!(log.sig.params.len(), 1);
        assert!(log.sig.params[0].rest);
        assert_eq!(log.sig.params[0].ts_type, "any[]");
        assert!(!log.sig.params[0].optional);
    }

    #[test]
    fn class_rename_and_async_and_hatches() {
        let c = class(syn::parse_quote! {
            #[js_class(rename = "Sql")]
            impl Db {
                #[js_method]
                async fn load(&self) -> JSResult<String> { unimplemented!() }

                #[js_method(ts_return = "RunResult", ts_params = "sql: string, params?: SQLiteParams")]
                fn run(&self, sql: String, params: Optional<JSArray>) -> JSResult<JSObject> { unimplemented!() }
            }
        });
        assert_eq!(c.name, "Sql");
        assert_eq!(c.members[0].sig.ret, "Promise<string>");
        let run = &c.members[1];
        assert_eq!(run.sig.ret, "RunResult");
        assert_eq!(
            run.sig.raw_params.as_deref(),
            Some("sql: string, params?: SQLiteParams")
        );
    }

    #[test]
    fn interface_from_derive_maps_fields() {
        let s: syn::ItemStruct = syn::parse_quote! {
            /// Options.
            #[derive(FromJSObject, Default)]
            struct SpawnOptions {
                cmd: String,
                #[js_name = "maxBuffer"]
                max_buffer: Option<u32>,
                args: Vec<String>,
                #[ts_type = "number | bigint"]
                rowid: i64,
            }
        };
        let Item::Interface(i) = extract_struct(&s).unwrap().unwrap() else {
            panic!("interface")
        };
        assert_eq!(i.name, "SpawnOptions");
        assert_eq!(i.docs, vec!["Options.".to_string()]);
        assert_eq!(i.fields[0].ts_type, "string");
        assert!(!i.fields[0].optional);
        assert_eq!(i.fields[1].name, "maxBuffer");
        assert_eq!(i.fields[1].ts_type, "number");
        assert!(i.fields[1].optional);
        assert_eq!(i.fields[2].ts_type, "string[]");
        assert_eq!(i.fields[3].ts_type, "number | bigint");
        assert!(!i.fields[3].optional);
    }

    #[test]
    fn interface_field_ts_type_preserves_optionality() {
        let s: syn::ItemStruct = syn::parse_quote! {
            #[derive(FromJSObject, Default)]
            struct Options {
                #[ts_type = "\"GET\" | \"PUT\""]
                method: Option<String>,
            }
        };
        let Item::Interface(i) = extract_struct(&s).unwrap().unwrap() else {
            panic!("interface")
        };
        assert_eq!(i.fields[0].ts_type, "\"GET\" | \"PUT\"");
        assert!(i.fields[0].optional);
    }

    #[test]
    fn js_default_fields_are_optional() {
        let s: syn::ItemStruct = syn::parse_quote! {
            #[derive(FromJSObject, Default)]
            struct Options {
                #[js_default]
                enabled: bool,
            }
        };
        let Item::Interface(interface) = extract_struct(&s).unwrap().unwrap() else {
            panic!("interface")
        };
        assert!(interface.fields[0].optional);
        assert_eq!(interface.fields[0].ts_type, "boolean");
    }

    #[test]
    fn ts_skip_omits_internal_derive_structs() {
        let s: syn::ItemStruct = syn::parse_quote! {
            #[derive(FromJSObject, Default)]
            #[ts_skip]
            struct InternalOptions {
                x: String,
            }
        };
        assert!(extract_struct(&s).unwrap().is_none());
    }

    #[test]
    fn numeric_enum_from_js_numeric_enum_attr() {
        let e: syn::ItemEnum = syn::parse_quote! {
            /// Seek origin.
            #[js_numeric_enum]
            enum SeekMode {
                /// Seek from start.
                Start = 0,
                Current = 1,
                End = 2,
            }
        };
        let Item::NumericEnum(e) = extract_numeric_enum(&e).unwrap().unwrap() else {
            panic!("numeric enum")
        };
        assert_eq!(e.name, "SeekMode");
        assert_eq!(e.docs, vec!["Seek origin.".to_string()]);
        assert_eq!(e.variants.len(), 3);
        assert_eq!(e.variants[0].name, "Start");
        assert_eq!(e.variants[0].value, 0);
        assert_eq!(e.variants[0].docs, vec!["Seek from start.".to_string()]);
    }

    #[test]
    fn js_union_becomes_a_type_alias() {
        let e: syn::ItemEnum = syn::parse_quote! {
            /// A string or number.
            #[js_union]
            enum StringOrNumber {
                String(String),
                Number(f64),
            }
        };
        let Item::TypeAlias(alias) = extract_union(&e).unwrap().unwrap() else {
            panic!("type alias")
        };
        assert_eq!(alias.name, "StringOrNumber");
        assert_eq!(alias.ts_type, "string | number");
        assert_eq!(alias.docs, ["A string or number."]);
    }

    #[test]
    fn struct_without_derive_is_ignored() {
        let s: syn::ItemStruct = syn::parse_quote! { struct Plain { x: i32 } };
        assert!(extract_struct(&s).unwrap().is_none());
    }
}
