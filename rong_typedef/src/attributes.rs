//! Shared parsing for binding attributes consumed by both proc macros and
//! source-based type generation.

use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Attribute, Expr, Lit, LitStr, Meta, Result, Token};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct JsClassOptions {
    pub rename: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct JsMethodOptions {
    pub rename: Option<String>,
    pub getter: bool,
    pub setter: bool,
    pub enumerable: bool,
    pub gc_mark: bool,
    pub constructor: bool,
    pub private: bool,
    pub ts_return: Option<String>,
    pub ts_params: Option<String>,
}

#[derive(Clone)]
pub enum JsDefault {
    Default,
    String(LitStr),
}

#[derive(Default, Clone)]
pub struct JsFieldOptions {
    pub js_name: Option<String>,
    pub default: Option<JsDefault>,
    pub ts_type: Option<String>,
    pub ts_skip: bool,
}

impl Parse for JsClassOptions {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let metas = Punctuated::<Meta, Token![,]>::parse_terminated(input)?;
        let mut options = Self::default();
        for meta in metas {
            match meta {
                Meta::NameValue(value) if value.path.is_ident("rename") => {
                    set_string(&mut options.rename, &value.value, "rename")?;
                }
                other => return Err(syn::Error::new_spanned(other, "unknown js_class option")),
            }
        }
        Ok(options)
    }
}

pub fn parse_js_class_args(tokens: proc_macro2::TokenStream) -> Result<JsClassOptions> {
    if tokens.is_empty() {
        Ok(JsClassOptions::default())
    } else {
        syn::parse2(tokens)
    }
}

pub fn js_class_options(attrs: &[Attribute]) -> Result<Option<JsClassOptions>> {
    let mut result = None;
    for attr in attrs
        .iter()
        .filter(|attr| path_last_is(attr.path(), "js_class"))
    {
        if result.is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "duplicate js_class attribute",
            ));
        }
        let options = match &attr.meta {
            Meta::Path(_) => JsClassOptions::default(),
            Meta::List(list) => parse_js_class_args(list.tokens.clone())?,
            other => return Err(syn::Error::new_spanned(other, "invalid js_class attribute")),
        };
        result = Some(options);
    }
    Ok(result)
}

pub fn js_method_options(attrs: &[Attribute]) -> Result<Option<JsMethodOptions>> {
    let mut result = None;
    for attr in attrs
        .iter()
        .filter(|attr| path_last_is(attr.path(), "js_method"))
    {
        if result.is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "duplicate js_method attribute",
            ));
        }
        let mut options = JsMethodOptions::default();
        let metas = match &attr.meta {
            Meta::Path(_) => Punctuated::new(),
            Meta::List(list) => {
                list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?
            }
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "invalid js_method attribute",
                ));
            }
        };
        for meta in metas {
            match meta {
                Meta::Path(path) if path.is_ident("getter") => {
                    set_flag(&mut options.getter, &path, "getter")?
                }
                Meta::Path(path) if path.is_ident("setter") => {
                    set_flag(&mut options.setter, &path, "setter")?
                }
                Meta::Path(path) if path.is_ident("enumerable") => {
                    set_flag(&mut options.enumerable, &path, "enumerable")?
                }
                Meta::Path(path) if path.is_ident("gc_mark") => {
                    set_flag(&mut options.gc_mark, &path, "gc_mark")?
                }
                Meta::Path(path) if path.is_ident("constructor") => {
                    set_flag(&mut options.constructor, &path, "constructor")?
                }
                Meta::Path(path) if path.is_ident("private") => {
                    set_flag(&mut options.private, &path, "private")?
                }
                Meta::NameValue(value) if value.path.is_ident("rename") => {
                    set_string(&mut options.rename, &value.value, "rename")?
                }
                Meta::NameValue(value) if value.path.is_ident("ts_return") => {
                    set_string(&mut options.ts_return, &value.value, "ts_return")?
                }
                Meta::NameValue(value) if value.path.is_ident("ts_params") => {
                    set_string(&mut options.ts_params, &value.value, "ts_params")?
                }
                other => return Err(syn::Error::new_spanned(other, "unknown js_method option")),
            }
        }
        if options.getter && options.setter {
            return Err(syn::Error::new_spanned(
                attr,
                "a js_method cannot be both getter and setter",
            ));
        }
        if options.private && !options.constructor {
            return Err(syn::Error::new_spanned(
                attr,
                "private is only valid with constructor",
            ));
        }
        if options.private && options.ts_params.is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "ts_params is not valid on a private constructor",
            ));
        }
        if options.constructor
            && (options.getter
                || options.setter
                || options.gc_mark
                || options.enumerable
                || options.rename.is_some())
        {
            return Err(syn::Error::new_spanned(
                attr,
                "constructor cannot be combined with getter, setter, gc_mark, enumerable, or rename",
            ));
        }
        if options.constructor && options.ts_return.is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "ts_return is not valid on a constructor",
            ));
        }
        if (options.getter || options.setter) && options.ts_params.is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "ts_params is not valid on a getter or setter",
            ));
        }
        if options.setter && options.ts_return.is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "ts_return is not valid on a setter",
            ));
        }
        if options.gc_mark
            && (options.rename.is_some()
                || options.enumerable
                || options.ts_return.is_some()
                || options.ts_params.is_some())
        {
            return Err(syn::Error::new_spanned(
                attr,
                "gc_mark cannot be combined with rename, enumerable, ts_return, or ts_params",
            ));
        }
        result = Some(options);
    }
    Ok(result)
}

pub fn js_field_options(attrs: &[Attribute]) -> Result<JsFieldOptions> {
    let mut options = JsFieldOptions::default();
    for attr in attrs {
        if attr.path().is_ident("js_name") {
            let Meta::NameValue(value) = &attr.meta else {
                return Err(syn::Error::new_spanned(
                    attr,
                    "js_name requires a string value",
                ));
            };
            set_string(&mut options.js_name, &value.value, "js_name")?;
        } else if attr.path().is_ident("ts_type") {
            let Meta::NameValue(value) = &attr.meta else {
                return Err(syn::Error::new_spanned(
                    attr,
                    "ts_type requires a string value",
                ));
            };
            set_string(&mut options.ts_type, &value.value, "ts_type")?;
        } else if attr.path().is_ident("ts_skip") {
            if !matches!(attr.meta, Meta::Path(_)) {
                return Err(syn::Error::new_spanned(
                    attr,
                    "ts_skip does not accept a value",
                ));
            }
            set_flag(&mut options.ts_skip, attr, "ts_skip")?;
        } else if attr.path().is_ident("js_default") {
            if options.default.is_some() {
                return Err(syn::Error::new_spanned(
                    attr,
                    "duplicate js_default attribute",
                ));
            }
            options.default = Some(match &attr.meta {
                Meta::Path(_) => JsDefault::Default,
                Meta::NameValue(value) => match &value.value {
                    Expr::Lit(expr) => match &expr.lit {
                        Lit::Str(value) => JsDefault::String(value.clone()),
                        _ => {
                            return Err(syn::Error::new_spanned(
                                attr,
                                "js_default requires a string value",
                            ));
                        }
                    },
                    _ => {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "js_default requires a string value",
                        ));
                    }
                },
                _ => {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "invalid js_default attribute",
                    ));
                }
            });
        }
    }
    Ok(options)
}

pub fn path_last_is(path: &syn::Path, name: &str) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == name)
}

/// Parse the unsigned integer expression accepted by `#[js_numeric_enum]`.
pub fn u32_integer_expr(expr: &Expr) -> Result<u32> {
    let Expr::Lit(expr) = expr else {
        return Err(syn::Error::new_spanned(
            expr,
            "js_numeric_enum values must be integer literals",
        ));
    };
    let Lit::Int(value) = &expr.lit else {
        return Err(syn::Error::new_spanned(
            expr,
            "js_numeric_enum values must be integer literals",
        ));
    };
    value.base10_parse()
}

fn set_flag(flag: &mut bool, span: impl quote::ToTokens, name: &str) -> Result<()> {
    if *flag {
        return Err(syn::Error::new_spanned(
            span,
            format!("duplicate {name} option"),
        ));
    }
    *flag = true;
    Ok(())
}

fn set_string(slot: &mut Option<String>, expr: &Expr, name: &str) -> Result<()> {
    if slot.is_some() {
        return Err(syn::Error::new_spanned(
            expr,
            format!("duplicate {name} option"),
        ));
    }
    let Expr::Lit(expr) = expr else {
        return Err(syn::Error::new_spanned(
            expr,
            format!("{name} requires a string literal"),
        ));
    };
    let Lit::Str(value) = &expr.lit else {
        return Err(syn::Error::new_spanned(
            expr,
            format!("{name} requires a string literal"),
        ));
    };
    *slot = Some(value.value());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_options_are_shared_and_fail_closed() {
        let method: syn::ImplItemFn = syn::parse_quote! {
            #[js_method(constructor, ts_params = "value?: string")]
            fn new() {}
        };
        let options = js_method_options(&method.attrs).unwrap().unwrap();
        assert!(options.constructor);
        assert_eq!(options.ts_params.as_deref(), Some("value?: string"));

        let typo: syn::ImplItemFn = syn::parse_quote! {
            #[js_method(ts_retrun = "string")]
            fn value() {}
        };
        assert!(js_method_options(&typo.attrs).is_err());
    }

    #[test]
    fn field_defaults_are_explicit_metadata() {
        let item: syn::ItemStruct = syn::parse_quote! {
            struct Options {
                #[js_name = "enabledValue"]
                #[js_default]
                enabled: bool,
            }
        };
        let syn::Fields::Named(fields) = item.fields else {
            panic!("named fields")
        };
        let field = fields.named.first().unwrap();
        let options = js_field_options(&field.attrs).unwrap();
        assert_eq!(options.js_name.as_deref(), Some("enabledValue"));
        assert!(matches!(options.default, Some(JsDefault::Default)));
    }
}
