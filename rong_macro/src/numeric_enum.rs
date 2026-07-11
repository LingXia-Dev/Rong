use proc_macro2::{Span, TokenStream};
use quote::quote;
use rong_typegen::u32_integer_expr;
use syn::{Data, DeriveInput, Error, Fields};

pub(crate) fn impl_numeric_enum(input: &DeriveInput) -> Result<TokenStream, Error> {
    let name = &input.ident;

    if !input.generics.params.is_empty() {
        return Err(Error::new_spanned(
            &input.generics,
            "js_numeric_enum does not support generic enums",
        ));
    }

    let Data::Enum(data) = &input.data else {
        return Err(Error::new(
            Span::call_site(),
            "js_numeric_enum can only be used on enums",
        ));
    };

    let mut from_arms = Vec::new();
    let mut into_arms = Vec::new();
    let mut object_sets = Vec::new();
    let mut expected = Vec::new();

    for variant in &data.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(Error::new(
                variant.ident.span(),
                "js_numeric_enum only supports unit variants",
            ));
        }

        let Some((_, expr)) = &variant.discriminant else {
            return Err(Error::new(
                variant.ident.span(),
                "js_numeric_enum variants must have explicit integer values",
            ));
        };

        let value = u32_integer_expr(expr)?;

        let variant_name = &variant.ident;
        let js_name = variant_name.to_string();
        expected.push(format!("{js_name} ({value})"));

        from_arms.push(quote! {
            #value => Ok(Self::#variant_name),
        });
        into_arms.push(quote! {
            Self::#variant_name => <u32 as rong::IntoJSValue<rong::JSEngineValue>>::into_js_value(#value, ctx),
        });
        object_sets.push(quote! {
            obj.set(#js_name, #value)?;
        });
    }

    let expected = expected.join(", ");
    let input_tokens = quote! { #input };

    Ok(quote! {
        #input_tokens

        impl rong::FromJSValue<rong::JSEngineValue> for #name {
            fn from_js_value(ctx: &rong::JSContext, value: rong::JSValue) -> rong::JSResult<Self> {
                let value = <u32 as rong::FromJSValue<rong::JSEngineValue>>::from_js_value(ctx, value)?;
                match value {
                    #(#from_arms)*
                    other => Err(rong::HostError::new(
                        rong::error::E_INVALID_ARG,
                        format!(
                            "Invalid value for enum {}: {}. Expected one of: {}",
                            stringify!(#name),
                            other,
                            #expected,
                        )
                    ).with_name("TypeError").into()),
                }
            }
        }

        impl rong::IntoJSValue<rong::JSEngineValue> for #name {
            fn into_js_value(self, ctx: &rong::JSContext) -> rong::JSValue {
                match self {
                    #(#into_arms)*
                }
            }
        }

        impl rong::function::JSParameterType for #name {}

        impl #name {
            pub fn js_object(ctx: &rong::JSContext) -> rong::JSResult<rong::JSObject> {
                let obj = rong::JSObject::new(ctx);
                #(#object_sets)*
                Ok(obj)
            }
        }
    })
}
