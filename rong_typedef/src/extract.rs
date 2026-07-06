//! Extract descriptors from parsed Rust source.
//!
//! The generator parses a crate's `.rs` files with `syn` and feeds each item
//! here. [`extract_impl`] turns a `#[js_class] impl` into a class descriptor;
//! [`extract_struct`] turns a `#[derive(FromJSObj)]` / `IntoJSObj` struct into
//! an interface. Everything is pure AST → descriptor, so it is unit-testable
//! and shared by any crate's generation (rong's modules or `lingxia-logic`).

use crate::map::{array_of, is_injected, map_return, rust_type_to_ts};
use crate::model::{ClassDef, Field, FnSig, InterfaceDef, Item, Member, MemberKind, Param};
use std::collections::HashSet;
use syn::{
    Attribute, Expr, FnArg, ImplItem, ImplItemFn, ItemImpl, ItemStruct, Lit, Meta, Pat, ReturnType,
    Type,
};

/// Extract a class descriptor from an `impl` block, if it carries `#[js_class]`.
pub fn extract_impl(input: &ItemImpl) -> Option<Item> {
    let class_rename = js_class_rename(&input.attrs)?;
    let name = class_rename.or_else(|| type_name(&input.self_ty))?;
    if name.is_empty() {
        return None; // e.g. a `#[js_class]` on an unnamed/complex self type
    }

    let mut constructor = None;
    let mut private_constructor = false;
    let mut members = Vec::new();

    // Parse each method's options once.
    let parsed: Vec<(&ImplItemFn, Opts)> = fns(input).map(|m| (m, parse_opts(m))).collect();
    // A getter that also has a setter is a read/write property, not readonly.
    let setter_names: HashSet<String> = parsed
        .iter()
        .filter(|(_, o)| o.setter)
        .map(|(m, o)| js_name(m, o))
        .collect();

    for (m, opts) in &parsed {
        let m = *m;
        if opts.gc_mark {
            continue;
        }
        let is_async = m.sig.asyncness.is_some();
        let member_name = js_name(m, opts);

        if opts.constructor {
            if constructor_rejects(m) {
                private_constructor = true;
            } else {
                constructor = Some(FnSig {
                    params: params(m),
                    ret: String::new(),
                    raw_params: opts.ts_args.clone(),
                });
            }
            continue;
        }
        if opts.setter {
            continue; // folded into its getter's property
        }

        let member = if opts.getter {
            let kind = if setter_names.contains(&member_name) {
                MemberKind::Property
            } else {
                MemberKind::Getter
            };
            Member {
                kind,
                name: member_name,
                docs: doc_lines(&m.attrs),
                sig: make_sig(m, opts, is_async, false, &name),
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

    Some(Item::Class(ClassDef {
        name,
        docs: doc_lines(&input.attrs),
        constructor,
        private_constructor,
        members,
    }))
}

/// Whether a constructor body rejects direct construction by calling
/// `illegal_constructor(...)` — in a bare expr, a `let`, a `return`/`?`, or
/// nested in a block/`if`.
fn constructor_rejects(m: &ImplItemFn) -> bool {
    stmts_call_illegal(&m.block.stmts)
}

fn stmts_call_illegal(stmts: &[syn::Stmt]) -> bool {
    stmts.iter().any(|s| match s {
        syn::Stmt::Expr(e, _) => expr_calls_illegal(e),
        syn::Stmt::Local(l) => l.init.as_ref().is_some_and(|i| expr_calls_illegal(&i.expr)),
        _ => false,
    })
}

fn expr_calls_illegal(e: &Expr) -> bool {
    match e {
        Expr::Call(c) => {
            matches!(&*c.func, Expr::Path(p) if path_last_is(&p.path, "illegal_constructor"))
        }
        Expr::Return(r) => r.expr.as_deref().is_some_and(expr_calls_illegal),
        Expr::Try(t) => expr_calls_illegal(&t.expr),
        Expr::Block(b) => stmts_call_illegal(&b.block.stmts),
        Expr::If(i) => {
            stmts_call_illegal(&i.then_branch.stmts)
                || i.else_branch
                    .as_ref()
                    .is_some_and(|(_, e)| expr_calls_illegal(e))
        }
        _ => false,
    }
}

/// True if this impl carries `#[js_method]` fns but no `#[js_class]`, so its
/// methods would be silently dropped. Callers can warn.
pub fn has_orphan_js_methods(input: &ItemImpl) -> bool {
    js_class_rename(&input.attrs).is_none() && fns(input).next().is_some()
}

/// Extract an interface from a struct that derives `FromJSObj` or `IntoJSObj`.
pub fn extract_struct(input: &ItemStruct) -> Option<Item> {
    if !derives_js_obj(&input.attrs) {
        return None;
    }
    let syn::Fields::Named(named) = &input.fields else {
        return None;
    };

    let fields = named
        .named
        .iter()
        .map(|f| {
            let rust_name = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
            let (ts_type, optional) = field_ts(&f.ty);
            Field {
                name: field_js_name(&f.attrs, &rust_name),
                ts_type,
                optional,
                readonly: false,
                docs: doc_lines(&f.attrs),
            }
        })
        .collect();

    Some(Item::Interface(InterfaceDef {
        name: input.ident.to_string(),
        docs: doc_lines(&input.attrs),
        fields,
    }))
}

// ---- class helpers ----

fn fns(input: &ItemImpl) -> impl Iterator<Item = &ImplItemFn> {
    input.items.iter().filter_map(|it| match it {
        ImplItem::Fn(f) if has_ident_attr(&f.attrs, "js_method") => Some(f),
        _ => None,
    })
}

#[derive(Default)]
struct Opts {
    rename: Option<String>,
    getter: bool,
    setter: bool,
    gc_mark: bool,
    constructor: bool,
    ts_return: Option<String>,
    ts_args: Option<String>,
}

fn parse_opts(m: &ImplItemFn) -> Opts {
    let mut opts = Opts::default();
    for attr in &m.attrs {
        if !path_last_is(attr.path(), "js_method") {
            continue;
        }
        let Meta::List(list) = &attr.meta else {
            continue;
        };
        let Ok(nested) = list
            .parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
        else {
            continue;
        };
        for meta in nested {
            match meta {
                Meta::Path(p) if p.is_ident("getter") => opts.getter = true,
                Meta::Path(p) if p.is_ident("setter") => opts.setter = true,
                Meta::Path(p) if p.is_ident("gc_mark") => opts.gc_mark = true,
                Meta::Path(p) if p.is_ident("constructor") => opts.constructor = true,
                Meta::NameValue(nv) => {
                    if let Expr::Lit(e) = &nv.value
                        && let Lit::Str(s) = &e.lit
                    {
                        if nv.path.is_ident("rename") {
                            opts.rename = Some(s.value());
                        } else if nv.path.is_ident("ts_return") {
                            opts.ts_return = Some(s.value());
                        } else if nv.path.is_ident("ts_args") {
                            opts.ts_args = Some(s.value());
                        }
                    }
                }
                _ => {}
            }
        }
    }
    opts
}

fn js_name(m: &ImplItemFn, opts: &Opts) -> String {
    opts.rename
        .clone()
        .unwrap_or_else(|| m.sig.ident.to_string())
}

/// Build a signature, applying `ts_return`/`ts_args` overrides when present.
fn make_sig(
    m: &ImplItemFn,
    opts: &Opts,
    is_async: bool,
    include_params: bool,
    self_name: &str,
) -> FnSig {
    let params = if include_params { params(m) } else { vec![] };
    // A `ts_return` hatch gives the *inner* type; async wrapping still applies,
    // so an async method with `ts_return = "T"` renders `Promise<T>`. Don't
    // double-wrap if a hatch already spelled out `Promise<…>`.
    let ret = match &opts.ts_return {
        Some(t) if is_async && !t.trim_start().starts_with("Promise<") => {
            format!("Promise<{t}>")
        }
        Some(t) => t.clone(),
        None => return_ts(m, is_async, self_name),
    };
    FnSig {
        params,
        ret,
        raw_params: opts.ts_args.clone(),
    }
}

/// JS-visible parameters: drop the receiver and runtime-injected params.
fn params(m: &ImplItemFn) -> Vec<Param> {
    m.sig
        .inputs
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

fn return_ts(m: &ImplItemFn, is_async: bool, self_name: &str) -> String {
    let ret = match &m.sig.output {
        ReturnType::Default => {
            if is_async {
                "Promise<void>".to_string()
            } else {
                "void".to_string()
            }
        }
        ReturnType::Type(_, ty) => map_return(ty, is_async),
    };
    // `Self` (builder methods returning `Self`) names the class in TS. Replace
    // only whole-word `Self` tokens, so a user type like `SelfTest` is intact.
    replace_ident(&ret, "Self", self_name)
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

/// The rename in `#[js_class(rename = "…")]`, wrapped so a present `#[js_class]`
/// returns `Some` (with `None` inside when no rename), absent returns `None`.
fn js_class_rename(attrs: &[Attribute]) -> Option<Option<String>> {
    let attr = attrs.iter().find(|a| path_last_is(a.path(), "js_class"))?;
    Some(match &attr.meta {
        Meta::List(list) => list
            .parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
            .ok()
            .and_then(|nested| {
                nested.into_iter().find_map(|m| match m {
                    Meta::NameValue(nv) if nv.path.is_ident("rename") => str_lit(&nv.value),
                    _ => None,
                })
            }),
        _ => None,
    })
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
                    .any(|p| path_last_is(p, "FromJSObj") || path_last_is(p, "IntoJSObj"))
            })
            .unwrap_or(false)
    })
}

/// A struct field's TS type and optionality: `Option<T>`/`Optional<T>` render as
/// an optional field of `T` (not `T | null`).
fn field_ts(ty: &Type) -> (String, bool) {
    if (last_ident_is(ty, "Option") || last_ident_is(ty, "Optional"))
        && let Some(inner) = generic_arg0(ty)
    {
        return (rust_type_to_ts(&inner).text, true);
    }
    (rust_type_to_ts(ty).text, false)
}

/// Field name honoring `#[rename = "…"]` (the derives' field attribute).
fn field_js_name(attrs: &[Attribute], rust_name: &str) -> String {
    for a in attrs {
        if a.path().is_ident("rename")
            && let Meta::NameValue(nv) = &a.meta
            && let Some(v) = str_lit(&nv.value)
        {
            return v;
        }
    }
    rust_name.to_string()
}

// ---- shared helpers ----

fn has_ident_attr(attrs: &[Attribute], ident: &str) -> bool {
    attrs.iter().any(|a| path_last_is(a.path(), ident))
}

/// Match an attribute/derive by its final path segment, so a qualified form
/// like `#[rong::js_class]` or `#[derive(rong::FromJSObj)]` still matches.
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
        match extract_impl(&input).unwrap() {
            Item::Class(c) => c,
            _ => panic!("expected class"),
        }
    }

    #[test]
    fn only_js_class_impls_are_extracted() {
        // No #[js_class] -> not extracted.
        let plain: syn::ItemImpl = syn::parse_quote! { impl Foo { fn bar(&self) {} } };
        assert!(extract_impl(&plain).is_none());
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
    fn illegal_constructor_becomes_private() {
        let c = class(syn::parse_quote! {
            #[js_class]
            impl Statement {
                #[js_method(constructor)]
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
        });
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

                #[js_method(ts_return = "RunResult", ts_args = "sql: string, params?: SQLiteParams")]
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
            #[derive(FromJSObj, Default)]
            struct SpawnOptions {
                cmd: String,
                #[rename = "maxBuffer"]
                max_buffer: Option<u32>,
                args: Vec<String>,
            }
        };
        let Item::Interface(i) = extract_struct(&s).unwrap() else {
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
    }

    #[test]
    fn struct_without_derive_is_ignored() {
        let s: syn::ItemStruct = syn::parse_quote! { struct Plain { x: i32 } };
        assert!(extract_struct(&s).is_none());
    }
}
