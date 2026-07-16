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
            let field_name_str = field_name.to_string();

            // Check if field type is Option<T>
            if let Some(inner_type) = extract_option_inner_type(field_type) {
                // Optional field
                Ok(quote! {
                    #field_name: match obj.get::<_, Option<#inner_type>>(#js_name_lit) {
                        Ok(val) => val,
                        Err(e) if e.is_property_not_found() => None,
                        Err(e) => return Err(rong::HostError::new(
                            rong::error::E_INVALID_ARG,
                            format!("Failed to convert field '{}': {}", #field_name_str, e)
                        ).with_name("TypeError").into()),
                    }
                })
            } else if let Some(js_default_expr) = js_default_value {
                // Field with default value
                Ok(quote! {
                    #field_name: match obj.get(#js_name_lit) {
                        Ok(val) => val,
                        Err(e) if e.is_property_not_found() => #js_default_expr,
                        Err(e) => return Err(rong::HostError::new(
                            rong::error::E_INVALID_ARG,
                            format!("Failed to convert field '{}': {}", #field_name_str, e)
                        ).with_name("TypeError").into()),
                    }
                })
            } else {
                // Required field
                Ok(quote! {
                    #field_name: obj.get(#js_name_lit).map_err(|e| {
                        if e.is_property_not_found() {
                            rong::HostError::new(
                                rong::error::E_MISSING_PROPERTY,
                                format!("Required field '{}' is missing", #field_name_str)
                            ).with_name("TypeError")
                        } else {
                            rong::HostError::new(
                                rong::error::E_INVALID_ARG,
                                format!("Failed to convert field '{}': {}", #field_name_str, e)
                            ).with_name("TypeError")
                        }
                    })?
                })
            }
        })
        .collect::<syn::Result<Vec<_>>>()?;

    let expanded = quote! {
        impl rong::FromJSValue<rong::JSEngineValue> for #name {
            fn from_js_value(ctx: &rong::JSContext, value: rong::JSValue) -> rong::JSResult<Self> {
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
