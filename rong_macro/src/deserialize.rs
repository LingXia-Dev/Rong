use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use rong_typegen::{JsDefault, js_field_options};
use syn::{Data, Fields, GenericArgument, PathArguments, Type, TypePath};

/// Check if a type is Option<T> and return the inner type T
fn extract_option_inner_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(TypePath { path, .. }) = ty
        && let Some(segment) = path.segments.last()
        && segment.ident == "Option"
        && let PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(GenericArgument::Type(inner_ty)) = args.args.first()
    {
        return Some(inner_ty);
    }
    None
}

pub(crate) fn impl_deserialize(input: syn::DeriveInput) -> syn::Result<TokenStream2> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "FromJSObject does not support generic structs",
        ));
    }
    let name = &input.ident;

    // Get the fields from the struct
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    &input,
                    "FromJSObject can only be derived for structs with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                &input,
                "FromJSObject can only be derived for structs",
            ));
        }
    };

    // Generate field extractions
    let field_extractions = fields
        .iter()
        .map(|field| -> syn::Result<TokenStream2> {
            let field_name = field.ident.as_ref().unwrap();
            let field_type = &field.ty;
            let options = js_field_options(&field.attrs)?;
            if options.ts_skip {
                return Err(syn::Error::new_spanned(
                    field,
                    "ts_skip is only valid on a derived struct",
                ));
            }
            let js_name = options.js_name.unwrap_or_else(|| field_name.to_string());
            let js_default_value = options.default.map(|value| match value {
                JsDefault::Default => quote! { Default::default() },
                JsDefault::String(value) => quote! { #value.into() },
            });

            let js_name_lit = syn::LitStr::new(&js_name, field_name.span());

            // Presence comes from `get_opt` itself, so a nested conversion that
            // reports a missing field is never mistaken for this field being absent.
            if let Some(inner_type) = extract_option_inner_type(field_type) {
                // Optional field: absent, `undefined` and `null` are `None`.
                Ok(quote! {
                    #field_name: match obj.get_opt::<_, Option<#inner_type>>(#js_name_lit) {
                        Ok(val) => val.flatten(),
                        Err(e) => return Err(e.into_field_error(#js_name_lit)),
                    }
                })
            } else if let Some(js_default_expr) = js_default_value {
                // Field with default value: used when the property is absent or
                // `undefined`, like an omitted `Optional` argument; `null` is a value.
                Ok(quote! {
                    #field_name: match obj.get_opt::<_, rong::JSValue>(#js_name_lit) {
                        Ok(Some(val)) if !val.is_undefined() => {
                            <#field_type as rong::FromJSValue<rong::JSEngineValue>>::from_js_value(ctx, val)
                                .map_err(|e| e.into_field_error(#js_name_lit))?
                        }
                        Ok(_) => #js_default_expr,
                        Err(e) => return Err(e.into_field_error(#js_name_lit)),
                    }
                })
            } else {
                // Required field
                Ok(quote! {
                    #field_name: match obj.get_opt::<_, #field_type>(#js_name_lit) {
                        Ok(Some(val)) => val,
                        Ok(None) => {
                            return Err(rong::HostError::missing_field(#js_name_lit).into());
                        }
                        Err(e) => return Err(e.into_field_error(#js_name_lit)),
                    }
                })
            }
        })
        .collect::<syn::Result<Vec<_>>>()?;

    let expanded = quote! {
        impl rong::FromJSValue<rong::JSEngineValue> for #name {
            fn from_js_value(ctx: &rong::JSContext, value: rong::JSValue) -> rong::JSResult<Self> {
                if !value.is_object() {
                    return Err(rong::HostError::type_mismatch(
                        "an object",
                        value.type_of().to_string(),
                    )
                    .into());
                }
                let obj = rong::JSObject::from_js_value(ctx, value)?;
                Ok(Self {
                    #(#field_extractions,)*
                })
            }
        }

        impl rong::function::JSParameterType for #name {}
    };

    Ok(expanded)
}
