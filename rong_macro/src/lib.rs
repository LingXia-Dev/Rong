use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{DeriveInput, ItemImpl, parse_macro_input};

mod api;
mod class;
mod deserialize;
mod r#enum;
mod instance;
mod numeric_enum;
mod serialize;

/// Convert an untagged Rust enum to and from the first matching JavaScript value.
///
/// Every variant must contain exactly one unnamed field whose type implements
/// `FromJSValue` and `IntoJSValue`.
///
/// ```ignore
/// #[js_union]
/// enum StringOrNumber {
///     String(String),
///     Number(f64),
/// }
/// ```
#[proc_macro_attribute]
pub fn js_union(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[js_union] does not accept options",
        )
        .to_compile_error()
        .into();
    }
    let input = parse_macro_input!(item as DeriveInput);
    if input.attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "js_numeric_enum")
    }) {
        return syn::Error::new_spanned(
            &input,
            "an enum cannot be both #[js_union] and #[js_numeric_enum]",
        )
        .to_compile_error()
        .into();
    }
    match r#enum::impl_enum_conversions(&input) {
        Ok(expanded) => expanded.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Expose a numeric enum as JavaScript numbers plus a JS constant object.
///
/// This is for APIs such as `Rong.SeekMode.Start === 0`: each unit variant must
/// have an explicit integer value. The generated type accepts numbers from JS,
/// returns numbers to JS, and provides `Type::js_object(ctx)` for namespace
/// registration.
///
/// ```ignore
/// #[js_numeric_enum]
/// enum SeekMode {
///     Start = 0,
///     Current = 1,
///     End = 2,
/// }
/// ```
#[proc_macro_attribute]
pub fn js_numeric_enum(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[js_numeric_enum] does not accept options",
        )
        .to_compile_error()
        .into();
    }
    let input = parse_macro_input!(item as DeriveInput);
    if input.attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "js_union")
    }) {
        return syn::Error::new_spanned(
            &input,
            "an enum cannot be both #[js_numeric_enum] and #[js_union]",
        )
        .to_compile_error()
        .into();
    }
    match numeric_enum::impl_numeric_enum(&input) {
        Ok(expanded) => expanded.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Declare a crate's runtime JavaScript bindings and TypeScript-only API types.
///
/// The declaration generates the namespace registration function and is also
/// consumed directly from Rust source by `rong-typegen`.
///
/// ```ignore
/// rong::js_api! {
///     fn register_fs(ctx) {
///         namespace RongNamespace = ctx.host_namespace();
///         fn file = rong_file::file;
///         const SeekMode: "typeof SeekMode" = file::SeekMode::js_object(ctx)?;
///     }
/// }
/// register_fs(ctx)?;
/// ```
///
/// `fn`, `class`, and `const` entries install runtime namespace properties.
/// `type` entries are TypeScript-only aliases emitted by `rong-typegen`.
#[proc_macro]
pub fn js_api(input: TokenStream) -> TokenStream {
    api::expand(input)
}

/// Make a Rust struct a JavaScript class, or define its JavaScript members.
///
/// Apply it to the struct to generate class-instance conversion. Use
/// `#[js_class(clone)]` when JavaScript instances may be cloned back into Rust.
/// Apply it to the impl block to process methods marked with `#[js_method]`.
/// Methods can be exposed as:
/// - Regular methods
/// - Property getters/setters
/// - Static methods/properties
/// - Async methods (automatically converted to JavaScript Promises)
///
/// # Attributes
/// - Struct: `clone` permits cloning a JavaScript instance back into Rust.
/// - Impl: `rename = "name"` changes the JavaScript class name.
///
/// # Method Types
/// - Instance methods: Take `&self` or `&mut self`
/// - Static methods: No self parameter
/// - Constructors: Marked with `#[js_method(constructor)]`
/// - Async methods: Methods marked with `async` keyword
///
/// # Example
/// ```ignore
/// use rong_macro::{js_class, js_method};
///
/// #[js_class]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// #[js_class(rename = "PointX")]  // Class will be named "PointX" in JavaScript
/// impl Point {
///     // Constructor
///     #[js_method(constructor)]
///     fn new(x: i32, y: i32) -> Self {
///         Self { x, y }
///     }
///
///     // Instance property
///     #[js_method(getter, enumerable)]
///     fn x(&self) -> i32 { self.x }
///
///     // Static method
///     #[js_method]
///     fn create(x: i32, y: i32) -> Self {
///         Self { x, y }
///     }
///
///     // Async instance methods use `&self`; use interior mutability when state
///     // must change across an await point.
///     #[js_method]
///     async fn distance_async(&self) -> f64 {
///         ((self.x.pow(2) + self.y.pow(2)) as f64).sqrt()
///     }
///
///     // Async static method
///     #[js_method]
///     async fn create_async(x: i32, y: i32) -> Self {
///         // Async operation
///         Self { x, y }
///     }
/// }
/// ```
///
/// # Async Methods
/// Async methods are automatically converted to JavaScript Promises:
/// - Rust async methods become JavaScript async functions
/// - Return values are wrapped in Promises
/// - Can be used with JavaScript `async/await` syntax
/// - Support both instance and static methods
/// - Can be used as property getters/setters
///
/// JavaScript usage:
/// ```javascript
/// // Using async instance method
/// let point = new PointX(1, 2);
/// const distance = await point.distanceAsync();
///
/// // Using async static method
/// let newPoint = await PointX.createAsync(5, 6);
/// ```
#[proc_macro_attribute]
pub fn js_class(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2: TokenStream2 = attr.into();
    if let Ok(input) = syn::parse::<ItemImpl>(item.clone()) {
        let impl_tokens = match class::class_impl(&input, attr2) {
            Ok(tokens) => tokens,
            Err(err) => return err.to_compile_error().into(),
        };
        return quote! {
            #input
            #impl_tokens
        }
        .into();
    }

    let input = match syn::parse::<DeriveInput>(item) {
        Ok(input) if matches!(&input.data, syn::Data::Struct(_)) => input,
        Ok(input) => {
            return syn::Error::new_spanned(input, "#[js_class] requires a struct or impl block")
                .to_compile_error()
                .into();
        }
        Err(error) => return error.to_compile_error().into(),
    };
    let mut input = input;
    input.attrs.push(syn::parse_quote!(#[js_class(#attr2)]));
    match instance::class_instance_impl(&input) {
        Ok(expanded) => expanded.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

/// Configure how a Rust method is exposed to JavaScript.
///
/// This attribute can only be applied to methods, not to impl blocks.
/// For impl blocks, use `#[js_class]` instead.
///
/// This attribute configures the behavior of individual methods when they are
/// exposed to JavaScript. It supports various options for controlling how the
/// method appears and behaves in JavaScript.
///
/// # Options
/// - `getter`: Expose as a property getter
/// - `setter`: Expose as a property setter
/// - `enumerable`: Make the property visible in enumerations
/// - `rename = "name"`: Use a different name in JavaScript
/// - `constructor`: Mark as the class constructor
/// - `private`: Render a constructor as private in generated TypeScript; only
///   valid together with `constructor`
/// - `ts_params = "..."`: Override the generated TypeScript parameter list
/// - `ts_return = "..."`: Override the generated TypeScript return type
/// - `gc_mark`: Use this method to implement garbage collection marking
///
/// # Property Attributes
/// - All properties are configurable by default
/// - Properties are non-enumerable by default
/// - Writable state is determined by the presence of a setter
///
/// # Examples
/// ```ignore
/// use rong_macro::{js_class, js_method};
///
/// #[js_class]
/// struct MyClass {
///     value: i32,
/// }
///
/// #[js_class]  // Use js_class for impl block
/// impl MyClass {
///     // Constructor
///     #[js_method(constructor)]
///     fn new() -> Self { Self { value: 0 } }
///
///     // Public property with custom name
///     #[js_method(getter, enumerable, rename = "value")]
///     fn get_value(&self) -> i32 { self.value }
///
///     // Regular method
///     #[js_method(rename = "calculateTotal")]
///     fn calc_total(&self) -> i32 { self.value * 2 }
/// }
/// ```
#[proc_macro_attribute]
pub fn js_method(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Try to parse as impl block to check for misuse
    if syn::parse::<ItemImpl>(item.clone()).is_ok() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "Use #[js_class] for impl blocks, not #[js_method]",
        )
        .to_compile_error()
        .into();
    }

    // Just pass through the original item if it's not an impl block
    item
}

/// Derive `FromJSValue` for a named-field Rust struct representing a JS object.
///
/// Fields can be renamed using the `js_name`
/// attribute to match different JavaScript property names.
///
/// # Attributes
/// - `js_name = "name"`: Use a different name for the field in JavaScript
/// - `js_default`: Use `Default::default()` if the field is missing
/// - `js_default = "value"`: Use a specific default value if the field is missing
///
/// # Field Types
/// - **Required fields**: Must exist in the JavaScript object, will error if missing
/// - **Optional fields**: Use `Option<T>` type, will be `None` if missing
/// - **Fields with defaults**: Use `#[js_default]` or `#[js_default = "value"]`, will use default if missing
/// - All field types must implement `FromJSValue`
///
/// # Example
/// ```ignore
/// #[derive(FromJSObject)]
/// struct Person {
///     #[js_name = "firstName"]
///     first_name: String,
///     #[js_name = "lastName"]
///     last_name: String,
///     age: i32,
///     // Optional field - will be None if missing
///     nickname: Option<String>,
///     // Field with default value
///     #[js_default = "active"]
///     status: String,
///     // Field using Default::default()
///     #[js_default]
///     score: i32,
/// }
/// ```
///
/// # JavaScript Usage
/// ```javascript
/// // This will successfully deserialize
/// const complete = {
///     firstName: "John",
///     lastName: "Doe",
///     age: 30,
///     nickname: "Johnny",
///     status: "premium"
/// };
/// // Result: Person { first_name: "John", last_name: "Doe", age: 30,
/// //                  nickname: Some("Johnny"), status: "premium", score: 0 }
///
/// // This will also work (using defaults)
/// const minimal = {
///     firstName: "Jane",
///     lastName: "Smith",
///     age: 25
/// };
/// // Result: Person { first_name: "Jane", last_name: "Smith", age: 25,
/// //                  nickname: None, status: "active", score: 0 }
///
/// // This will fail with clear error message
/// const incomplete = {
///     firstName: "John",
///     lastName: "Doe"
///     // Missing required field 'age'
/// };
/// // Error: "Required field 'age' is missing"
/// ```
///
/// # Error Handling
/// The macro provides detailed error messages:
/// - Missing required fields: "Required field 'field_name' is missing"
/// - Type conversion errors: "Failed to convert field 'field_name': [original error]"
#[proc_macro_derive(FromJSObject, attributes(js_name, js_default, ts_type, ts_skip))]
pub fn derive_from_js_object(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match deserialize::impl_deserialize(input) {
        Ok(expanded) => expanded.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_derive(FromJSValue)]
pub fn derive_from_js_value(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    if !input.generics.params.is_empty() {
        return syn::Error::new_spanned(
            &input.generics,
            "FromJSValue does not support generic types",
        )
        .to_compile_error()
        .into();
    }
    let name = &input.ident;

    // Generate the FromJSValue implementation
    let expanded = quote! {
        impl rong::FromJSValue<rong::JSEngineValue> for #name
        where Self: rong::TryFromJSValue,
        {
            fn from_js_value(_ctx: &rong::JSContext, value: rong::JSValue) -> rong::JSResult<Self> {
                <Self as rong::TryFromJSValue>::try_from_js(value)
            }
        }

        impl rong::function::JSParameterType for #name {}
    };

    TokenStream::from(expanded)
}

/// Derive `IntoJSValue` for a named-field Rust struct representing a JS object.
///
/// This macro automatically implements the `IntoJSValue` trait for a struct, allowing it
/// to be serialized to JavaScript objects. Fields can be renamed using the `js_name`
/// attribute to match different JavaScript property names.
///
/// # Attributes
/// - `js_name = "name"`: Use a different name for the field in JavaScript
///
/// # Field Types
/// - All field types must implement `IntoJSValue`
/// - Optional fields (`Option<T>`) will be omitted if `None`, or set to the value if `Some(T)`
/// - Common types like `String`, `i32`, `f64`, `bool`, etc. are already supported
///
/// # Example
/// ```ignore
/// #[derive(IntoJSObject)]
/// struct Person {
///     #[js_name = "firstName"]
///     first_name: String,
///     #[js_name = "lastName"]
///     last_name: String,
///     age: i32,
///     // Optional field - will be omitted if None
///     nickname: Option<String>,
/// }
/// ```
///
/// # JavaScript Usage
/// ```javascript
/// // The struct will be converted to:
/// {
///     firstName: "John",
///     lastName: "Doe",
///     age: 30,
///     nickname: "Johnny"  // Only present if Some(value)
/// }
/// ```
#[proc_macro_derive(IntoJSObject, attributes(js_name, ts_type, ts_skip))]
pub fn derive_into_js_object(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match serialize::impl_serialize(input) {
        Ok(expanded) => expanded.into(),
        Err(error) => error.to_compile_error().into(),
    }
}
