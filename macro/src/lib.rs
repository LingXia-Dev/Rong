use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{DeriveInput, ItemImpl, parse_macro_input};

mod class;
mod deserialize;
mod r#enum;
mod instance;
mod serialize;

/// Expose a Rust struct or enum as a JavaScript object.
///
/// This macro generates the necessary code to make a Rust type usable in JavaScript,
/// including type conversions and object registration.
///
/// For structs:
/// - Generates class instance implementation
/// - Allows method and property definitions
/// - Supports constructors and static methods
/// - Always implements `IntoJSValue` and `JSParameterType`
/// - Implements `FromJSValue` only with `#[js_export(clone)]`
///
/// For enums:
/// - Implements `FromJSValue`, `IntoJSValue`, and `JSParameterType` traits
/// - Provides automatic type conversion and error handling
/// - each variant required to implement `FromJSValue`, `IntoJSValue`
///
/// # Example (Struct)
/// ```ignore
/// #[js_export]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
/// ```
///
/// # Example (Enum)
/// ```ignore
/// #[js_export]
/// enum Status {
///     Pending(String),
///     Complete(i32),
/// }
/// ```
#[proc_macro_attribute]
pub fn js_export(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let attr2: TokenStream2 = attr.into();

    match &input.data {
        syn::Data::Enum(_) => match r#enum::impl_enum_conversions(&input) {
            Ok(expanded) => expanded.into(),
            Err(err) => err.to_compile_error().into(),
        },
        _ => {
            // For structs, use the existing class implementation
            let object_attr = syn::parse_quote!(#[js_export(#attr2)]);
            let mut new_input = input.clone();
            new_input.attrs.push(object_attr);

            match instance::class_instance_impl(&new_input) {
                Ok(expanded) => expanded.into(),
                Err(err) => err.to_compile_error().into(),
            }
        }
    }
}

/// Define JavaScript methods and properties for a class.
///
/// This macro can only be applied to impl blocks and processes method definitions
/// marked with `#[js_method]`. Methods can be exposed as:
/// - Regular methods
/// - Property getters/setters
/// - Static methods/properties
/// - Async methods (automatically converted to JavaScript Promises)
///
/// # Attributes
/// - `rename = "name"`: Use a different name for the class in JavaScript
///   If not specified, the impl block type name will be used
///
/// # Method Types
/// - Instance methods: Take `&self` or `&mut self`
/// - Static methods: No self parameter
/// - Constructors: Marked with `#[js_method(constructor)]`
/// - Async methods: Methods marked with `async` keyword
///
/// # Example
/// ```ignore
/// use rong_macro::{js_export, js_method, js_class};
///
/// #[js_export]
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
///     // Async instance method
///     #[js_method]
///     async fn move_by_async(&mut self, dx: i32, dy: i32) {
///         // Async operation
///         self.x += dx;
///         self.y += dy;
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
/// await point.moveByAsync(10, 20);
///
/// // Using async static method
/// let newPoint = await PointX.createAsync(5, 6);
/// ```
#[proc_macro_attribute]
pub fn js_class(attr: TokenStream, item: TokenStream) -> TokenStream {
    // First try to parse as impl block
    let result = syn::parse::<ItemImpl>(item.clone());

    // Return error if not an impl block
    if result.is_err() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[js_class] can only be used on impl blocks",
        )
        .to_compile_error()
        .into();
    }

    let input = result.unwrap();
    let attr2: TokenStream2 = attr.into();

    let impl_tokens = match class::class_impl(&input, attr2) {
        Ok(tokens) => tokens,
        Err(err) => return err.to_compile_error().into(),
    };

    let expanded = quote! {
        #input

        #impl_tokens
    };

    TokenStream::from(expanded)
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
/// - `gc_mark`: Use this method to implement garbage collection marking
///
/// # Property Attributes
/// - All properties are configurable by default
/// - Properties are non-enumerable by default
/// - Writable state is determined by the presence of a setter
///
/// # Examples
/// ```ignore
/// use rong_macro::{js_export, js_method, js_class};
///
/// #[js_export]
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

/// Derive macro for implementing deserialization from JavaScript values to Rust structs.
///
/// This macro automatically implements the `FromJSObj` trait for a struct, allowing it
/// to be deserialized from JavaScript objects. Fields can be renamed using the `rename`
/// attribute to match different JavaScript property names.
///
/// # Attributes
/// - `rename = "name"`: Use a different name for the field in JavaScript
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
/// #[derive(FromJSObj)]
/// struct Person {
///     #[rename = "firstName"]
///     first_name: String,
///     #[rename = "lastName"]
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
#[proc_macro_derive(FromJSObj, attributes(rename, js_default, ts_type, ts_skip))]
pub fn derive_from_js_object(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    TokenStream::from(deserialize::impl_deserialize(input))
}

#[proc_macro_derive(FromJSValue)]
pub fn derive_from_js_value(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // Generate the FromJSValue implementation
    let expanded = quote! {
        impl rong::FromJSValue<rong::JSEngineValue> for #name
        where Self: TryFromJSValue,
        {
            fn from_js_value(_ctx: &rong::JSContext, value: rong::JSValue) -> rong::JSResult<Self> {
                Self::try_from_js(value)
            }
        }

        impl rong::function::JSParameterType for #name {}
    };

    TokenStream::from(expanded)
}

/// Derive macro for implementing serialization from Rust structs to JavaScript objects.
///
/// This macro automatically implements the `IntoJSValue` trait for a struct, allowing it
/// to be serialized to JavaScript objects. Fields can be renamed using the `rename`
/// attribute to match different JavaScript property names.
///
/// # Attributes
/// - `rename = "name"`: Use a different name for the field in JavaScript
///
/// # Field Types
/// - All field types must implement `IntoJSValue`
/// - Optional fields (`Option<T>`) will be omitted if `None`, or set to the value if `Some(T)`
/// - Common types like `String`, `i32`, `f64`, `bool`, etc. are already supported
///
/// # Example
/// ```ignore
/// #[derive(IntoJSObj)]
/// struct Person {
///     #[rename = "firstName"]
///     first_name: String,
///     #[rename = "lastName"]
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
#[proc_macro_derive(IntoJSObj, attributes(rename, ts_type, ts_skip))]
pub fn derive_into_js_object(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    TokenStream::from(serialize::impl_serialize(input))
}
