use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields};

pub(crate) fn impl_enum_conversions(input: &DeriveInput) -> Result<TokenStream, Error> {
    if !input.generics.params.is_empty() {
        return Err(Error::new_spanned(
            &input.generics,
            "#[js_union] does not support generic enums",
        ));
    }
    let name = &input.ident;

    match &input.data {
        Data::Enum(data) => {
            let mut from_js_variants = Vec::new();
            let mut into_js_variants = Vec::new();

            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                match &variant.fields {
                    Fields::Unnamed(fields) => {
                        if fields.unnamed.len() == 1 {
                            let ty = &fields.unnamed.first().unwrap().ty;
                            from_js_variants.push(quote! {
                                if let Ok(val) = #ty::from_js_value(ctx, value.clone()) {
                                    return Ok(Self::#variant_name(val));
                                }
                            });

                            into_js_variants.push(quote! {
                                Self::#variant_name(val) => <#ty as rong::IntoJSValue<rong::JSEngineValue>>::into_js_value(val, ctx)
                            });
                        } else {
                            return Err(Error::new(
                                variant_name.span(),
                                "Multiple fields in enum variants are not yet supported",
                            ));
                        }
                    }
                    Fields::Unit => {
                        // Unit variants are not supported
                        return Err(Error::new(
                            variant_name.span(),
                            "Unit variants in enums are not supported",
                        ));
                    }
                    Fields::Named(_) => {
                        return Err(Error::new(
                            variant_name.span(),
                            "Named fields in enums are not supported",
                        ));
                    }
                }
            }

            let variant_names = data.variants.iter().map(|v| &v.ident);

            let expanded = quote! {
                impl rong::FromJSValue<rong::JSEngineValue> for #name {
                    fn from_js_value(ctx: &rong::JSContext, value: rong::JSValue) -> rong::JSResult<Self> {
                        #(#from_js_variants)*
                        Err(rong::HostError::new(
                            rong::error::E_INVALID_ARG,
                            format!(
                                "Invalid value for enum {}. Expected one of: {}",
                                stringify!(#name),
                                [#(stringify!(#variant_names)),*].join(", ")
                            )
                        ).with_name("TypeError").into())
                    }
                }

                impl rong::IntoJSValue<rong::JSEngineValue> for #name {
                    fn into_js_value(self, ctx: &rong::JSContext) -> rong::JSValue {
                        match self {
                            #(#into_js_variants,)*
                        }
                    }
                }

                impl rong::function::JSParameterType for #name {}
            };

            let input_tokens = quote! { #input };
            Ok(quote! {
                #input_tokens
                #expanded
            })
        }
        _ => Err(Error::new(
            Span::call_site(),
            "This implementation is only for enums",
        )),
    }
}
