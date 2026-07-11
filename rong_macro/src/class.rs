use proc_macro2::TokenStream;
use quote::quote;
use rong_typegen::{js_method_options, parse_js_class_args, validate_js_method_signature};
use syn::ItemImpl;

/// Configuration options for JavaScript method/property bindings.
///
/// # Property Types
///
/// Properties are automatically categorized as static or instance based on the presence
/// of a self receiver:
/// - Methods with no self receiver become static properties/methods
/// - Methods with self receiver become instance properties/methods
///
/// # Property Attributes
///
/// JavaScript properties have three key attributes that control their behavior:
///
/// ## Configurable
/// - When `true`: Property can be deleted and its attributes can be modified
/// - Default: `true` for all properties created by this macro
/// - Note: This is automatically set and cannot be changed
///
/// ## Enumerable
/// - When `true`: Property shows up in enumerations (`Object.keys()`, `for...in`)
/// - Default: `false` (properties are hidden by default)
/// - Set with: `#[js_method(enumerable)]`
///
/// ## Writable
/// - When `true`: Property value can be changed
/// - Automatically determined by the presence of a setter
/// - Note: Accessor properties (getter/setter) don't use this attribute
///
/// # Examples
///
/// ```ignore
/// use rong_macro::{js_class, js_method};
///
/// #[js_class]
/// struct MyStruct {
///     value: i32,
/// }
///
/// #[js_class]
/// impl MyStruct {
///     // Public property with getter and setter
///     #[js_method(getter, enumerable)]
///     fn value(&self) -> i32 { self.value }
///
///     #[js_method(setter, rename = "value")]
///     fn set_value(&mut self, v: i32) { self.value = v; }
///
///     // Read-only property (getter only)
///     #[js_method(getter)]
///     fn computed(&self) -> i32 { self.value * 2 }
/// }
/// ```
/// Process method attributes and generate JavaScript bindings
pub fn class_impl(input: &ItemImpl, attr: TokenStream) -> syn::Result<TokenStream> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "#[js_class] does not support generic impl blocks",
        ));
    }
    let impl_type = &input.self_ty;

    // Get class name from js_class attribute if present
    let mut js_class_name = quote!(#impl_type).to_string();

    if let Some(rename) = parse_js_class_args(attr)?.rename {
        js_class_name = rename;
    }

    let js_class_name = syn::LitStr::new(&js_class_name, proc_macro2::Span::call_site());

    let mut instance_methods = Vec::new();
    let mut static_methods = Vec::new();
    let mut constructor = None;
    let mut gc_mark_impl = None;

    // Type alias for property definition tuple
    type PropertyDef = (Option<TokenStream>, Option<TokenStream>, bool);
    let mut instance_properties = std::collections::BTreeMap::<String, PropertyDef>::new();
    let mut static_properties = std::collections::BTreeMap::<String, PropertyDef>::new();
    let mut instance_method_names = std::collections::HashSet::new();
    let mut static_method_names = std::collections::HashSet::new();

    // Process each method in the impl block
    for method in &input.items {
        let method = match method {
            syn::ImplItem::Fn(method) => method,
            _ => continue,
        };

        let Some(opts) = js_method_options(&method.attrs)? else {
            continue;
        };
        validate_js_method_signature(method, &opts)?;

        let method_name = &method.sig.ident;
        let is_async = method.sig.asyncness.is_some();

        let js_name = syn::LitStr::new(
            &opts
                .rename
                .clone()
                .unwrap_or_else(|| method_name.to_string()),
            method_name.span(),
        );

        // Check if this is a gc_mark method (special handling)
        if opts.gc_mark {
            if gc_mark_impl.is_some() {
                return Err(syn::Error::new_spanned(
                    method,
                    "a js_class cannot declare more than one gc_mark method",
                ));
            }
            // Generate direct JSClass::gc_mark_with implementation.
            gc_mark_impl = Some(quote! {
                fn gc_mark_with<F>(&self, mark_fn: F)
                where
                    F: FnMut(&rong::JSValue)
                {
                    Self::#method_name(self, mark_fn);
                }
            });
            continue;
        }

        if opts.constructor {
            if constructor.is_some() {
                return Err(syn::Error::new_spanned(
                    method,
                    "a js_class cannot declare more than one constructor",
                ));
            }
            constructor = Some(quote! {
                fn data_constructor() -> rong::function::Constructor<rong::JSEngineValue> {
                    rong::function::Constructor::new(Self::#method_name)
                }
            });
            continue;
        }

        let params = &method.sig.inputs;
        let has_receiver = method.sig.receiver().is_some();
        let returns_js_result = match &method.sig.output {
            syn::ReturnType::Default => false,
            syn::ReturnType::Type(_, ty) => match &**ty {
                syn::Type::Path(p) => p
                    .path
                    .segments
                    .last()
                    .is_some_and(|s| s.ident == "JSResult"),
                _ => false,
            },
        };

        if has_receiver {
            // Remove self parameter for instance methods
            let args: Vec<_> = params
                .iter()
                .skip(1)
                .map(|arg| {
                    if let syn::FnArg::Typed(pat_type) = arg {
                        (&*pat_type.pat, &*pat_type.ty)
                    } else {
                        unreachable!("Already skipped self receiver")
                    }
                })
                .collect();

            let (patterns, types): (Vec<_>, Vec<_>) = args.into_iter().unzip();

            // Handle instance methods with proper This/ThisMut mapping
            let (receiver_type, method_call) = if let Some(receiver) = method.sig.receiver() {
                if receiver.mutability.is_some() {
                    if is_async {
                        return Err(syn::Error::new_spanned(
                            &method.sig.ident,
                            "async methods with `&mut self` are not supported; use `&self` with interior mutability (RefCell/Mutex) or make the method synchronous",
                        ));
                    }

                    // For &mut self methods, use ThisMut and map to Self::method_name
                    (
                        quote! { __this: rong::function::ThisMut<#impl_type> },
                        if returns_js_result {
                            quote! {{
                                let mut __self = __this.borrow_mut()?;
                                Self::#method_name(&mut *__self, #(#patterns),*)
                            }}
                        } else {
                            quote! {{
                                let mut __self = __this.borrow_mut()?;
                                Ok(Self::#method_name(&mut *__self, #(#patterns),*))
                            }}
                        },
                    )
                } else {
                    // For &self methods, borrow the class instance directly from the JS object.
                    (
                        quote! { __this: rong::function::This<rong::function::JSClassRef<#impl_type>> },
                        if is_async {
                            if returns_js_result {
                                quote! {{
                                    let __self = {
                                        let __borrow = __this.borrow()?;
                                        <#impl_type as ::core::clone::Clone>::clone(&*__borrow)
                                    };
                                    Self::#method_name(&__self, #(#patterns),*).await
                                }}
                            } else {
                                quote! {{
                                    let __self = {
                                        let __borrow = __this.borrow()?;
                                        <#impl_type as ::core::clone::Clone>::clone(&*__borrow)
                                    };
                                    Ok(Self::#method_name(&__self, #(#patterns),*).await)
                                }}
                            }
                        } else if returns_js_result {
                            quote! {{
                                let __self = __this.borrow()?;
                                Self::#method_name(&*__self, #(#patterns),*)
                            }}
                        } else {
                            quote! {{
                                let __self = {
                                    __this.borrow()?
                                };
                                Ok(Self::#method_name(&*__self, #(#patterns),*))
                            }}
                        },
                    )
                }
            } else {
                unreachable!("Already checked has_receiver")
            };

            // Handle property getters/setters
            if opts.getter || opts.setter {
                let func = if is_async {
                    quote! {
                        class.new_func(|#receiver_type #(, #patterns: #types)*| async move {
                            #method_call
                        })?
                    }
                } else {
                    quote! {
                        class.new_func(move |#receiver_type #(, #patterns: #types)*| {
                            #method_call
                        })?
                    }
                };

                if instance_method_names.contains(&js_name.value()) {
                    return Err(syn::Error::new_spanned(
                        method,
                        format!("duplicate JavaScript instance member `{}`", js_name.value()),
                    ));
                }
                let entry = instance_properties
                    .entry(js_name.value())
                    .or_insert_with(|| (None, None, opts.enumerable));

                if opts.getter {
                    if entry.0.is_some() {
                        return Err(syn::Error::new_spanned(
                            method,
                            format!("duplicate JavaScript getter `{}`", js_name.value()),
                        ));
                    }
                    entry.0 = Some(func);
                } else {
                    if entry.1.is_some() {
                        return Err(syn::Error::new_spanned(
                            method,
                            format!("duplicate JavaScript setter `{}`", js_name.value()),
                        ));
                    }
                    entry.1 = Some(func);
                }
                entry.2 |= opts.enumerable;
            } else {
                if instance_properties.contains_key(&js_name.value())
                    || !instance_method_names.insert(js_name.value())
                {
                    return Err(syn::Error::new_spanned(
                        method,
                        format!("duplicate JavaScript instance member `{}`", js_name.value()),
                    ));
                }
                // Handle regular instance methods
                let method_def = if is_async {
                    quote! {
                        class.method(
                            #js_name,
                            |#receiver_type, #(#patterns: #types),*| async move {
                                #method_call
                            }
                        )?;
                    }
                } else {
                    quote! {
                        class.method(
                            #js_name,
                            move |#receiver_type, #(#patterns: #types),*| {
                                #method_call
                            }
                        )?;
                    }
                };
                instance_methods.push(method_def);
            }
        } else {
            let args: Vec<_> = params
                .iter()
                .map(|arg| {
                    if let syn::FnArg::Typed(pat_type) = arg {
                        (&*pat_type.pat, &*pat_type.ty)
                    } else {
                        unreachable!("Static methods don't have self receiver")
                    }
                })
                .collect();

            let (patterns, types): (Vec<_>, Vec<_>) = args.into_iter().unzip();

            // Handle static property accessors or regular static methods
            if opts.getter || opts.setter {
                let func = if is_async {
                    quote! {
                        class.new_func(|#(#patterns: #types),*| async move {
                            Self::#method_name(#(#patterns),*).await
                        })?
                    }
                } else {
                    quote! {
                        class.new_func(move |#(#patterns: #types),*| {
                            Self::#method_name(#(#patterns),*)
                        })?
                    }
                };

                if static_method_names.contains(&js_name.value()) {
                    return Err(syn::Error::new_spanned(
                        method,
                        format!("duplicate JavaScript static member `{}`", js_name.value()),
                    ));
                }
                let entry = static_properties
                    .entry(js_name.value())
                    .or_insert_with(|| (None, None, opts.enumerable));

                if opts.getter {
                    if entry.0.is_some() {
                        return Err(syn::Error::new_spanned(
                            method,
                            format!("duplicate JavaScript getter `{}`", js_name.value()),
                        ));
                    }
                    entry.0 = Some(func);
                } else {
                    if entry.1.is_some() {
                        return Err(syn::Error::new_spanned(
                            method,
                            format!("duplicate JavaScript setter `{}`", js_name.value()),
                        ));
                    }
                    entry.1 = Some(func);
                }
                entry.2 |= opts.enumerable;
            } else {
                if static_properties.contains_key(&js_name.value())
                    || !static_method_names.insert(js_name.value())
                {
                    return Err(syn::Error::new_spanned(
                        method,
                        format!("duplicate JavaScript static member `{}`", js_name.value()),
                    ));
                }
                // Handle regular static method
                let method_def = if is_async {
                    quote! {
                        class.static_method(
                            #js_name,
                            |#(#patterns: #types),*| async move {
                                Self::#method_name(#(#patterns),*).await
                            }
                        )?;
                    }
                } else {
                    quote! {
                        class.static_method(
                            #js_name,
                            move |#(#patterns: #types),*| {
                                Self::#method_name(#(#patterns),*)
                            }
                        )?;
                    }
                };
                static_methods.push(method_def);
            }
        }
    }

    let constructor = constructor.unwrap_or_else(|| {
        quote! {
            fn data_constructor() -> rong::function::Constructor<rong::JSEngineValue> {
                rong::function::Constructor::new(|_: ()| panic!("No constructor defined"))
            }
        }
    });

    // Generate instance property definitions
    for (name, (getter, setter, enumerable)) in instance_properties {
        let descriptor = match (getter.as_ref(), setter.as_ref()) {
            (Some(getter), Some(setter)) => {
                quote! { rong::PropertyDescriptor::from_accessor(#getter, #setter) }
            }
            (Some(getter), None) => quote! { rong::PropertyDescriptor::from_getter(#getter) },
            (None, Some(setter)) => quote! { rong::PropertyDescriptor::from_setter(#setter) },
            (None, None) => quote! { rong::PropertyDescriptor::new() },
        };

        let mut parts = Vec::new();

        // Always set configurable by default
        parts.push(quote! { .configurable() });

        // Set enumerable if specified
        if enumerable {
            parts.push(quote! { .enumerable() });
        }

        let property = quote! {
            class.property(#name, #descriptor #(#parts)*)?;
        };

        instance_methods.push(property);
    }

    // Generate static property definitions
    for (name, (getter, setter, enumerable)) in static_properties {
        let descriptor = match (getter.as_ref(), setter.as_ref()) {
            (Some(getter), Some(setter)) => {
                quote! { rong::PropertyDescriptor::from_accessor(#getter, #setter) }
            }
            (Some(getter), None) => quote! { rong::PropertyDescriptor::from_getter(#getter) },
            (None, Some(setter)) => quote! { rong::PropertyDescriptor::from_setter(#setter) },
            (None, None) => quote! { rong::PropertyDescriptor::new() },
        };

        let mut parts = Vec::new();

        // Always set configurable by default
        parts.push(quote! { .configurable() });

        // Set enumerable if specified
        if enumerable {
            parts.push(quote! { .enumerable() });
        }

        static_methods.push(quote! {
            class.static_property(#name, #descriptor #(#parts)*)?;
        });
    }

    let output = quote! {
        impl rong::JSClass<rong::JSEngineValue> for #impl_type {
            const NAME: &'static str = #js_class_name;

            #constructor

            fn class_setup(class: &rong::ClassSetup<rong::JSEngineValue>) -> JSResult<()> {
                #(#instance_methods)*
                #(#static_methods)*
                Ok(())
            }

            #gc_mark_impl
        }
    };

    // println!("Generated code:\n{}", output.to_string());
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualified_js_method_attributes_match_runtime_extraction() {
        let path: syn::Path = syn::parse_quote!(rong::js_method);
        assert!(rong_typegen::path_last_is(&path, "js_method"));
    }

    #[test]
    fn accessor_and_gc_signatures_fail_closed() {
        let getter: syn::ImplItemFn = syn::parse_quote! {
            #[js_method(getter)]
            fn value(&self, unexpected: String) -> String { unexpected }
        };
        let options = rong_typegen::js_method_options(&getter.attrs)
            .unwrap()
            .unwrap();
        assert!(validate_js_method_signature(&getter, &options).is_err());

        let setter: syn::ImplItemFn = syn::parse_quote! {
            #[js_method(setter)]
            fn value(&mut self, ctx: JSContext, value: String) {}
        };
        let options = rong_typegen::js_method_options(&setter.attrs)
            .unwrap()
            .unwrap();
        assert!(validate_js_method_signature(&setter, &options).is_ok());

        let gc: syn::ImplItemFn = syn::parse_quote! {
            #[js_method(gc_mark)]
            fn gc_mark(mark: F) {}
        };
        let options = rong_typegen::js_method_options(&gc.attrs).unwrap().unwrap();
        assert!(validate_js_method_signature(&gc, &options).is_err());
    }
}
