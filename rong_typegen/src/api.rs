//! Parser model shared by runtime `js_api!` registration and type generation.

use std::collections::HashSet;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{
    Attribute, Expr, ExprLit, Ident, Lit, LitStr, Path, Result, Token, braced, parenthesized,
};

/// One crate-local declaration of runtime JS bindings and their TypeScript API.
pub struct JsApiInput {
    pub register: Ident,
    pub ctx: Ident,
    pub namespace: Ident,
    pub target: Expr,
    pub entries: Punctuated<JsApiEntry, Token![;]>,
}

pub enum JsApiEntry {
    Function(FunctionExport),
    Class(ClassExport),
    Const(ConstExport),
    TypeAlias(TypeAliasExport),
}

pub struct ClassExport {
    pub name: LitStr,
    pub class: Path,
}

pub struct FunctionExport {
    pub name: LitStr,
    pub function: Path,
    pub ts_params: Option<LitStr>,
    pub ts_return: Option<LitStr>,
    pub rust_cfg: Option<LitStr>,
}

pub struct ConstExport {
    pub name: LitStr,
    pub ts_type: LitStr,
    pub value: Expr,
}

pub struct TypeAliasExport {
    pub name: Ident,
    pub ts_type: LitStr,
    pub docs: Vec<String>,
}

impl Parse for JsApiInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<Token![fn]>()?;
        let register = input.parse()?;
        let args;
        parenthesized!(args in input);
        let ctx = args.parse()?;
        if !args.is_empty() {
            return Err(args.error("registration function accepts exactly one context name"));
        }

        let content;
        braced!(content in input);
        let namespace_keyword: Ident = content.parse()?;
        if namespace_keyword != "namespace" {
            return Err(syn::Error::new(
                namespace_keyword.span(),
                "expected `namespace TypeScriptInterface = runtime_expression;`",
            ));
        }
        let namespace = content.parse()?;
        content.parse::<Token![=]>()?;
        let target = content.parse()?;
        content.parse::<Token![;]>()?;

        let entries = content.parse_terminated(JsApiEntry::parse, Token![;])?;
        let mut runtime_names = HashSet::new();
        let mut type_names = HashSet::new();
        for entry in &entries {
            match entry {
                JsApiEntry::Function(export) => unique_name(&mut runtime_names, &export.name)?,
                JsApiEntry::Class(export) => unique_name(&mut runtime_names, &export.name)?,
                JsApiEntry::Const(export) => unique_name(&mut runtime_names, &export.name)?,
                JsApiEntry::TypeAlias(export) => {
                    if !type_names.insert(export.name.to_string()) {
                        return Err(syn::Error::new(
                            export.name.span(),
                            format!("duplicate type alias `{}`", export.name),
                        ));
                    }
                }
            }
        }

        Ok(Self {
            register,
            ctx,
            namespace,
            target,
            entries,
        })
    }
}

impl Parse for JsApiEntry {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        if input.peek(Token![fn]) {
            reject_attrs(&attrs, "namespace functions")?;
            input.parse::<Token![fn]>()?;
            let name = parse_name(input)?;
            let (ts_params, ts_return, rust_cfg) = parse_function_options(input)?;
            input.parse::<Token![=]>()?;
            return Ok(Self::Function(FunctionExport {
                name,
                function: input.parse()?,
                ts_params,
                ts_return,
                rust_cfg,
            }));
        }

        let keyword: Ident = input
            .call(Ident::parse_any)
            .map_err(|_| input.error("expected `fn`, `class`, `const`, or `type` JS API entry"))?;
        match keyword.to_string().as_str() {
            "class" => {
                reject_attrs(&attrs, "namespace classes")?;
                let name = parse_name(input)?;
                input.parse::<Token![=]>()?;
                Ok(Self::Class(ClassExport {
                    name,
                    class: input.parse()?,
                }))
            }
            "const" => {
                reject_attrs(&attrs, "namespace constants")?;
                let name = parse_name(input)?;
                input.parse::<Token![:]>()?;
                let ts_type = input.parse()?;
                input.parse::<Token![=]>()?;
                Ok(Self::Const(ConstExport {
                    name,
                    ts_type,
                    value: input.parse()?,
                }))
            }
            "type" => {
                let name = input.parse()?;
                input.parse::<Token![=]>()?;
                Ok(Self::TypeAlias(TypeAliasExport {
                    name,
                    ts_type: input.parse()?,
                    docs: doc_lines(&attrs)?,
                }))
            }
            _ => Err(syn::Error::new(
                keyword.span(),
                "expected `fn`, `class`, `const`, or `type` JS API entry",
            )),
        }
    }
}

fn parse_function_options(
    input: ParseStream<'_>,
) -> Result<(Option<LitStr>, Option<LitStr>, Option<LitStr>)> {
    let mut ts_params = None;
    let mut ts_return = None;
    let mut rust_cfg = None;
    if input.peek(syn::token::Paren) {
        let options;
        parenthesized!(options in input);
        while !options.is_empty() {
            let key: Ident = options.parse()?;
            options.parse::<Token![=]>()?;
            let value: LitStr = options.parse()?;
            match key.to_string().as_str() {
                "ts_params" => set_once(&mut ts_params, value, "ts_params")?,
                "ts_return" => set_once(&mut ts_return, value, "ts_return")?,
                "cfg" => set_once(&mut rust_cfg, value, "cfg")?,
                _ => return Err(syn::Error::new(key.span(), "unknown function option")),
            }
            if options.is_empty() {
                break;
            }
            options.parse::<Token![,]>()?;
        }
    }
    Ok((ts_params, ts_return, rust_cfg))
}

fn unique_name(names: &mut HashSet<String>, name: &LitStr) -> Result<()> {
    if !names.insert(name.value()) {
        return Err(syn::Error::new(
            name.span(),
            format!("duplicate namespace entry `{}`", name.value()),
        ));
    }
    Ok(())
}

fn reject_attrs(attrs: &[Attribute], subject: &str) -> Result<()> {
    if let Some(attr) = attrs.first() {
        return Err(syn::Error::new_spanned(
            attr,
            format!("attributes are not supported on {subject}; document the referenced Rust item"),
        ));
    }
    Ok(())
}

fn doc_lines(attrs: &[Attribute]) -> Result<Vec<String>> {
    attrs
        .iter()
        .map(|attr| match &attr.meta {
            syn::Meta::NameValue(value) if value.path.is_ident("doc") => match &value.value {
                Expr::Lit(ExprLit {
                    lit: Lit::Str(value),
                    ..
                }) => Ok(value.value().trim().to_string()),
                _ => Err(syn::Error::new_spanned(
                    attr,
                    "doc attribute must be a string",
                )),
            },
            _ => Err(syn::Error::new_spanned(
                attr,
                "only doc comments are supported on type aliases",
            )),
        })
        .collect()
}

fn set_once(target: &mut Option<LitStr>, value: LitStr, name: &str) -> Result<()> {
    if target.is_some() {
        return Err(syn::Error::new(
            value.span(),
            format!("duplicate {name} option"),
        ));
    }
    *target = Some(value);
    Ok(())
}

fn parse_name(input: ParseStream<'_>) -> Result<LitStr> {
    if input.peek(LitStr) {
        input.parse()
    } else {
        let ident: Ident = input.parse()?;
        Ok(LitStr::new(&ident.to_string(), ident.span()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_runtime_bindings_and_type_aliases() {
        let input: JsApiInput = syn::parse_quote! {
            fn register_fs(ctx) {
                namespace RongNamespace = ctx.host_namespace();
                /// Data accepted by write.
                type WriteData = "string | ArrayBuffer";
                fn file = rong_file::file;
                fn readDir(ts_return = "AsyncIterableIterator<DirEntry>") = dir::readdir;
                class FileHandle = file::FileHandle;
                const SeekMode: "typeof SeekMode" = file::SeekMode::js_object(ctx)?;
            }
        };
        assert_eq!(input.namespace, "RongNamespace");
        assert_eq!(input.entries.len(), 5);
        let JsApiEntry::TypeAlias(alias) = &input.entries[0] else {
            panic!("type alias")
        };
        assert_eq!(alias.docs, ["Data accepted by write."]);
    }
}
