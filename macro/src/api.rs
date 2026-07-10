use proc_macro::TokenStream;
use quote::quote;
use rong_typedef::{JsApiEntry, JsApiInput};
use syn::parse_macro_input;

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as JsApiInput);
    let register = input.register;
    let ctx = input.ctx;
    let target = input.target;

    let mut registrations = Vec::new();
    for entry in input.entries {
        let registration = match entry {
            JsApiEntry::Function(function) => {
                let name = function.name;
                let rust_function = function.function;
                let cfg = match function.rust_cfg {
                    Some(value) => match syn::parse_str::<syn::Meta>(&value.value()) {
                        Ok(cfg) => Some(cfg),
                        Err(error) => return error.to_compile_error().into(),
                    },
                    None => None,
                };
                let registration = quote! {
                    let function = rong::JSFunc::new(__rong_ctx, #rust_function)?.name(#name)?;
                    __rong_namespace.set(#name, function)?;
                };
                match cfg {
                    Some(cfg) => quote! {
                        #[cfg(#cfg)]
                        {
                            #registration
                        }
                    },
                    None => registration,
                }
            }
            JsApiEntry::Class(class) => {
                let name = class.name;
                let class = class.class;
                quote! {
                    let constructor = rong::Class::lookup::<#class>(__rong_ctx)?.clone();
                    __rong_namespace.set(#name, constructor)?;
                }
            }
            JsApiEntry::Const(value) => {
                let name = value.name;
                let expression = value.value;
                quote! {
                    __rong_namespace.set(#name, #expression)?;
                }
            }
            JsApiEntry::TypeAlias(_) => continue,
        };
        registrations.push(registration);
    }

    quote! {
        fn #register(#ctx: &rong::JSContext) -> rong::JSResult<()> {
            let __rong_ctx = #ctx;
            let __rong_namespace = #target;
            #(#registrations)*
            Ok(())
        }
    }
    .into()
}
