use proc_macro2::TokenStream;
use quote::quote;
use syn::{Expr, ItemImpl, Lit, Meta};

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
/// use rong_macro::{js_export, js_method, js_class};
///
/// #[js_export]
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
///     #[js_method(setter)]
///     fn set_value(&mut self, v: i32) { self.value = v; }
///
///     // Read-only property (getter only)
///     #[js_method(getter)]
///     fn computed(&self) -> i32 { self.value * 2 }
/// }
/// ```
#[derive(Default)]
struct MethodOpts {
    rename: Option<String>,
    getter: bool,
    setter: bool,
    enumerable: bool,
    gc_mark: bool,
}

/// Process method attributes and generate JavaScript bindings
pub fn class_impl(input: &ItemImpl, attr: TokenStream) -> syn::Result<TokenStream> {
    let impl_type = &input.self_ty;

    // Get class name from js_class attribute if present
    let mut js_export_name = quote!(#impl_type).to_string();

    // Parse the rename attribute from the macro arguments
    if !attr.is_empty() {
        let meta = syn::parse2::<Meta>(attr)?;
        if let Meta::NameValue(nv) = meta
            && nv.path.is_ident("rename")
            && let Expr::Lit(expr_lit) = nv.value
            && let Lit::Str(s) = expr_lit.lit
        {
            js_export_name = s.value();
        }
    }

    let js_export_name = syn::LitStr::new(&js_export_name, proc_macro2::Span::call_site());

    let mut instance_methods = Vec::new();
    let mut static_methods = Vec::new();
    let mut constructor = None;
    let mut gc_mark_impl = None;

    // Type alias for property definition tuple
    type PropertyDef = (Option<TokenStream>, Option<TokenStream>, bool);
    let mut instance_properties: std::collections::HashMap<String, PropertyDef> =
        std::collections::HashMap::new();
    let mut static_properties: std::collections::HashMap<String, PropertyDef> =
        std::collections::HashMap::new();

    // Process each method in the impl block
    for method in &input.items {
        let method = match method {
            syn::ImplItem::Fn(method) => method,
            _ => continue,
        };

        // Skip methods that don't have #[js_method] attribute
        if !method
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("js_method"))
        {
            continue;
        }

        let method_name = &method.sig.ident;
        let is_async = method.sig.asyncness.is_some();

        // Parse method attributes
        let mut opts = MethodOpts::default();
        for attr in &method.attrs {
            if attr.path().is_ident("js_method")
                && let Meta::List(list) = &attr.meta
            {
                for nested in list.parse_args_with(
                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                )? {
                    match nested {
                        Meta::Path(path) => {
                            if path.is_ident("getter") {
                                opts.getter = true;
                            } else if path.is_ident("setter") {
                                opts.setter = true;
                            } else if path.is_ident("enumerable") {
                                opts.enumerable = true;
                            } else if path.is_ident("gc_mark") {
                                opts.gc_mark = true;
                            }
                        }
                        Meta::NameValue(nv) => {
                            if nv.path.is_ident("rename")
                                && let Expr::Lit(expr_lit) = &nv.value
                                && let Lit::Str(s) = &expr_lit.lit
                            {
                                opts.rename = Some(s.value());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let js_name = syn::LitStr::new(
            &opts.rename.unwrap_or_else(|| method_name.to_string()),
            method_name.span(),
        );

        // Check if this is a gc_mark method (special handling)
        if opts.gc_mark {
            // Make sure it's a method with &self receiver (not static)
            if let Some(receiver) = method.sig.receiver()
                && receiver.mutability.is_none()
            {
                // Generate direct JSClass::gc_mark_with implementation
                gc_mark_impl = Some(quote! {
                    // Implement gc_mark_with by calling the user's method
                    fn gc_mark_with<F>(&self, mark_fn: F)
                    where
                        F: FnMut(&rong::JSValue)
                    {
                        Self::#method_name(self, mark_fn);
                    }
                });
                continue;
            }
        }

        // Check if this is a constructor. Scan all metas so `constructor` is
        // recognized even alongside other options (e.g. `ts_args`).
        if method.attrs.iter().any(|attr| {
            attr.path().is_ident("js_method")
                && attr
                    .meta
                    .require_list()
                    .ok()
                    .and_then(|list| {
                        list.parse_args_with(
                            syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                        )
                        .ok()
                    })
                    .is_some_and(|metas| metas.iter().any(|m| m.path().is_ident("constructor")))
        }) {
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

                let entry = instance_properties
                    .entry(js_name.value())
                    .or_insert_with(|| (None, None, opts.enumerable));

                if opts.getter {
                    entry.0 = Some(func);
                } else {
                    entry.1 = Some(func);
                }
                entry.2 |= opts.enumerable;
            } else {
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

                let entry = static_properties
                    .entry(js_name.value())
                    .or_insert_with(|| (None, None, opts.enumerable));

                if opts.getter {
                    entry.0 = Some(func);
                } else {
                    entry.1 = Some(func);
                }
                entry.2 |= opts.enumerable;
            } else {
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
            const NAME: &'static str = #js_export_name;

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
