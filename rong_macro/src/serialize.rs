use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use rong_typegen::js_field_options;
use syn::{Data, Fields};

pub(crate) fn impl_serialize(input: syn::DeriveInput) -> syn::Result<TokenStream2> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "IntoJSObject does not support generic structs",
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
                    "IntoJSObject can only be derived for structs with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                &input,
                "IntoJSObject can only be derived for structs",
            ));
        }
    };

    // Generate field assignments
    let field_assignments = fields
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

            let js_name_lit = syn::LitStr::new(&js_name, field_name.span());

            // Check if field type is Option<T>
            let is_option = if let syn::Type::Path(type_path) = field_type {
                type_path
                    .path
                    .segments
                    .last()
                    .map(|seg| seg.ident == "Option")
                    .unwrap_or(false)
            } else {
                false
            };

            if is_option {
                // For Option<T>, only set the property if Some(value)
                Ok(quote! {
                    if let Some(ref value) = self.#field_name {
                        obj.set(#js_name_lit, value.clone())?;
                    }
                })
            } else {
                // For non-optional fields, always set the property
                Ok(quote! {
                    obj.set(#js_name_lit, self.#field_name.clone())?;
                })
            }
        })
        .collect::<syn::Result<Vec<_>>>()?;

    let expanded = quote! {
        impl rong::IntoJSValue<rong::JSEngineValue> for #name {
            fn into_js_value(self, ctx: &rong::JSContext) -> rong::JSValue {
                let obj = rong::JSObject::new(ctx);

                // Set each field on the object
                let result: rong::JSResult<()> = (|| {
                    #(#field_assignments)*
                    Ok(())
                })();

                // Preserve property-conversion failures on the JS exception channel.
                match result {
                    Ok(()) => obj.into_js_value(),
                    Err(error) => rong::IntoJSValue::into_js_value(error, ctx),
                }
            }
        }

        impl rong::function::JSParameterType for #name {}
    };

    Ok(expanded)
}
