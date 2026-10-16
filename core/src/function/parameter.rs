use crate::{
    FromJSValue, JSArrayOps, JSClass, JSContext, JSContextImpl, JSObject, JSObjectOps, JSResult,
    JSTypeOf, JSValue, JSValueImpl,
};
use std::cell::{Ref, RefMut};
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::ops::Deref;

/// Arguments retrieved from the JavaScript side for calling Rust functions.
pub struct ParamsAccessor<'a, V: JSValueImpl> {
    ctx: &'a JSContext<V::Context>,
    this: V,
    args: VecDeque<V>,
    is_last_param: bool,
}

impl<'a, V: JSValueImpl> ParamsAccessor<'a, V> {
    pub fn new(ctx: &'a JSContext<V::Context>, this: V, args: Vec<V>) -> Self {
        Self {
            ctx,
            this,
            args: args.into(),
            is_last_param: false,
        }
    }

    fn set_last_param(&mut self, is_last: bool) {
        self.is_last_param = is_last;
    }

    fn next_arg(&mut self) -> Option<V> {
        self.args.pop_front()
    }

    pub fn get_this(&self) -> V {
        self.this.clone()
    }

    pub(crate) fn context(&self) -> &JSContext<V::Context> {
        self.ctx
    }

    // length changed since its content will be removed
    pub(crate) fn args_len(&self) -> usize {
        self.args.len()
    }
}

/// Represents the `this` context in JavaScript function calls.
///
/// # Usage
/// - Used to capture the JavaScript `this` context in Rust functions
/// - Must be the first parameter if present
/// - Does not count towards required parameter count
///
/// # Example
/// ```ignore
/// use rong_core::function::parameter::This;
///
/// fn method(this: This<MyStruct>, x: i32) {
///     let my_struct: &MyStruct = &this;
/// }
/// ```
pub struct This<T>(pub T);

/// Represents the `this` context in JavaScript function calls with mutable access
pub struct ThisMut<T, V: JSValueImpl>(pub(crate) JSObject<V>, PhantomData<T>);

/// A non-owning handle to a JS class instance.
///
/// Unlike `FromJSValue<T>` for clone-enabled classes, this keeps the original JS
/// object identity and lets Rust borrow the underlying class data on demand.
pub struct JSClassRef<T, V: JSValueImpl>(JSObject<V>, PhantomData<T>);

/// Represents an optional parameter in JavaScript function calls.
///
/// # Usage
/// - Used for parameters that may or may not be provided
/// - Wraps the parameter type in `Option<T>`
/// - Does not count towards required parameter count
/// - Can appear anywhere in the parameter list
///
/// # Example
/// ```ignore
/// use rong_core::function::parameter::Optional;
///
/// fn func(x: i32, opt: Optional<String>) {
///     // Access the optional value via deref
///     if let Some(s) = &*opt {
///         println!("Optional param provided: {}", s);
///     }
/// }
/// ```
pub struct Optional<T>(pub Option<T>);

/// Represents rest parameters in JavaScript function calls.
///
/// # Usage
/// - Collects all remaining arguments into a `Vec<T>`
/// - Must be the last parameter if present
/// - Does not count towards required parameter count
/// - Useful for variadic functions
///
/// # Example
/// ```ignore
/// use rong_core::function::parameter::Rest;
///
/// fn variadic(x: i32, rest: Rest<String>) {
///     // Access the rest parameters via deref
///     for s in &*rest {
///         println!("Rest param: {}", s);
///     }
/// }
/// ```
pub struct Rest<T>(pub Vec<T>);

impl<T> Deref for This<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, V> ThisMut<T, V>
where
    V: JSObjectOps,
    T: JSClass<V>,
{
    pub fn object(&self) -> JSObject<V> {
        self.0.clone()
    }

    pub fn borrow_mut(&self) -> JSResult<RefMut<'_, T>> {
        self.0.borrow_mut::<T>()
    }
}

impl<T, V> JSClassRef<T, V>
where
    V: JSObjectOps,
    T: JSClass<V>,
{
    pub fn object(&self) -> JSObject<V> {
        self.0.clone()
    }

    pub fn borrow(&self) -> JSResult<Ref<'_, T>> {
        self.0.borrow::<T>()
    }

    pub fn borrow_mut(&self) -> JSResult<RefMut<'_, T>> {
        self.0.borrow_mut::<T>()
    }
}

impl<T, V> Clone for JSClassRef<T, V>
where
    V: JSObjectOps,
{
    fn clone(&self) -> Self {
        Self(self.0.clone(), PhantomData)
    }
}

impl<T, V> Deref for JSClassRef<T, V>
where
    V: JSValueImpl,
{
    type Target = JSObject<V>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> Deref for Optional<T> {
    type Target = Option<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> Deref for Rest<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Represents parameter requirements for a function
/// - required_count: number of mandatory parameters
/// - exhaustive: if true, no extra parameters are allowed beyond the required ones
pub trait FromParams<V: JSValueImpl>: Sized {
    fn from_params(accessor: &mut ParamsAccessor<V>) -> JSResult<Self>;
    fn param_requirements() -> ParamRequirement;
}

pub struct ParamRequirement {
    required_count: usize,
    exhaustive: bool,
}

impl ParamRequirement {
    pub fn required_count(&self) -> usize {
        self.required_count
    }

    const fn single() -> Self {
        Self {
            required_count: 1,
            exhaustive: true,
        }
    }

    const fn optional() -> Self {
        Self {
            required_count: 0,
            exhaustive: false,
        }
    }

    const fn any() -> Self {
        Self {
            required_count: 0,
            exhaustive: false,
        }
    }
}

pub trait ParameterKind {
    fn param_requirement() -> ParamRequirement;
}

pub struct Regular<T>(PhantomData<T>);
impl<T> ParameterKind for Regular<T> {
    fn param_requirement() -> ParamRequirement {
        ParamRequirement::single()
    }
}

pub struct ThisKind<T>(PhantomData<T>);
impl<T> ParameterKind for ThisKind<T> {
    fn param_requirement() -> ParamRequirement {
        ParamRequirement::any()
    }
}

pub struct ThisMutKind<T>(PhantomData<T>);
impl<T> ParameterKind for ThisMutKind<T> {
    fn param_requirement() -> ParamRequirement {
        ParamRequirement::any()
    }
}

pub struct OptionalKind<T>(PhantomData<T>);
impl<T> ParameterKind for OptionalKind<T> {
    fn param_requirement() -> ParamRequirement {
        ParamRequirement::optional()
    }
}

pub struct RestKind<T>(PhantomData<T>);
impl<T> ParameterKind for RestKind<T> {
    fn param_requirement() -> ParamRequirement {
        ParamRequirement::any()
    }
}

impl<C: JSContextImpl> ParameterKind for JSContext<C> {
    fn param_requirement() -> ParamRequirement {
        ParamRequirement::any()
    }
}

pub trait GetParam<V: JSValueImpl> {
    type Kind: ParameterKind;
    fn get_param(accessor: &mut ParamsAccessor<V>) -> JSResult<Self>
    where
        Self: Sized;
}

impl<T, V> GetParam<V> for T
where
    V: JSValueImpl,
    T: FromJSValue<V> + Sized,
    T: JSParameterType,
{
    type Kind = Regular<T>;

    fn get_param(accessor: &mut ParamsAccessor<V>) -> JSResult<Self> {
        let value = accessor.next_arg().unwrap(); // it's safe, since RustFunc::call ensures
        T::from_js_value(accessor.ctx, JSValue::from_raw(accessor.ctx, value))
    }
}

impl<V: JSValueImpl> GetParam<V> for JSContext<V::Context> {
    type Kind = JSContext<V::Context>;

    fn get_param(accessor: &mut ParamsAccessor<V>) -> JSResult<Self> {
        let ctx = accessor.context();
        Ok(ctx.clone())
    }
}

impl<T, V> GetParam<V> for This<T>
where
    V: JSValueImpl,
    T: FromJSValue<V> + JSParameterType,
{
    type Kind = ThisKind<T>;

    fn get_param(accessor: &mut ParamsAccessor<V>) -> JSResult<Self> {
        let value = accessor.get_this();
        let val = T::from_js_value(accessor.ctx, JSValue::from_raw(accessor.ctx, value))?;
        Ok(Self(val))
    }
}

impl<T, V> GetParam<V> for ThisMut<T, V>
where
    V: JSObjectOps,
    T: JSClass<V>,
{
    type Kind = ThisMutKind<T>;

    fn get_param(accessor: &mut ParamsAccessor<V>) -> JSResult<Self> {
        let value = accessor.get_this();

        let obj =
            JSObject::from_js_value(accessor.context(), JSValue::from_raw(accessor.ctx, value))?;
        if !crate::Class::instance_of::<T>(&obj) {
            return Err(crate::HostError::new(
                crate::error::E_TYPE,
                format!("Not instance of {}", std::any::type_name::<T>()),
            )
            .with_name("TypeError")
            .into());
        }
        Ok(ThisMut(obj, PhantomData))
    }
}

impl<T, V> GetParam<V> for Optional<T>
where
    V: JSValueImpl + JSTypeOf,
    T: FromJSValue<V>,
{
    type Kind = OptionalKind<T>;

    /// An explicit `undefined` is an omitted argument, as in JS default
    /// parameters: `f(undefined)` and `f()` both yield `Optional(None)`.
    fn get_param(accessor: &mut ParamsAccessor<V>) -> JSResult<Self> {
        match accessor.next_arg() {
            Some(v) => {
                let value = JSValue::from_raw(accessor.ctx, v);
                if value.is_undefined() {
                    return Ok(Optional(None));
                }
                T::from_js_value(accessor.ctx, value).map(|t| Optional(Some(t)))
            }
            None => Ok(Optional(None)),
        }
    }
}

impl<T, V> GetParam<V> for Rest<T>
where
    V: JSValueImpl,
    T: FromJSValue<V>,
{
    type Kind = RestKind<T>;

    fn get_param(accessor: &mut ParamsAccessor<V>) -> JSResult<Self> {
        let mut values = Vec::new();
        if accessor.is_last_param {
            while let Some(value) = accessor.next_arg() {
                values.push(T::from_js_value(
                    accessor.ctx,
                    JSValue::from_raw(accessor.ctx, value),
                )?);
            }
        }
        Ok(Rest(values))
    }
}

// Allow Vec<T> as a direct parameter, interpreting a single JS Array argument.
// This avoids requiring `impl<T> JSParameterType for Vec<T>`.
impl<T, V> GetParam<V> for Vec<T>
where
    V: JSValueImpl + JSTypeOf + JSObjectOps + JSArrayOps,
    T: FromJSValue<V>,
{
    type Kind = Regular<Vec<T>>;

    fn get_param(accessor: &mut ParamsAccessor<V>) -> JSResult<Self> {
        let value = accessor.next_arg().unwrap(); // safe: call site ensures arg exists
        <Vec<T> as FromJSValue<V>>::from_js_value(
            accessor.ctx,
            JSValue::from_raw(accessor.ctx, value),
        )
    }
}

/// Marker trait for types that can be used as JSFunc function parameters.
/// When used with JSFunc::new, the parameter types will be automatically
/// converted from JSValue to their Rust equivalents.
pub trait JSParameterType {}

impl JSParameterType for () {}
impl JSParameterType for i8 {}
impl JSParameterType for u8 {}
impl JSParameterType for i16 {}
impl JSParameterType for u16 {}
impl JSParameterType for i32 {}
impl JSParameterType for u32 {}
impl JSParameterType for i64 {}
impl JSParameterType for u64 {}
impl JSParameterType for f32 {}
impl JSParameterType for f64 {}
impl JSParameterType for bool {}
impl JSParameterType for String {}
impl JSParameterType for isize {}
impl JSParameterType for usize {}

/// for IntoJSArg
/// &str does not implement FromJSValue
impl JSParameterType for &str {}

/// `Option<T>` can be used as a parameter type for async functions
impl<T> JSParameterType for Option<T> where T: JSParameterType {}

impl<T, V> FromJSValue<V> for JSClassRef<T, V>
where
    V: JSObjectOps,
    T: JSClass<V>,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        let obj = JSObject::from_js_value(ctx, value)?;
        if !crate::Class::instance_of::<T>(&obj) {
            return Err(crate::HostError::new(
                crate::error::E_TYPE,
                format!("Not instance of {}", std::any::type_name::<T>()),
            )
            .with_name("TypeError")
            .into());
        }
        Ok(Self(obj, PhantomData))
    }
}

impl<T, V> JSParameterType for JSClassRef<T, V> where V: JSValueImpl {}

macro_rules! impl_from_params {
    ($($T:ident),*) => {
        impl<V: JSValueImpl, $($T,)*> FromParams<V> for ($($T,)*)
        where
            $($T: GetParam<V>,)*
        {
            #[allow(unused_variables)]
            fn from_params(accessor: &mut ParamsAccessor<V>) -> JSResult<Self> {
                let param_count = count_idents!($($T),*);
                #[allow(unused_mut)]
                let mut current_param = 0;

                Ok(($(
                    {
                        current_param += 1;
                        accessor.set_last_param(current_param == param_count);
                        $T::get_param(accessor)?
                    },
                )*))
            }

            fn param_requirements() -> ParamRequirement {

                #[allow(unused_mut)]
                let mut req = ParamRequirement {
                    required_count: 0,
                    exhaustive: true,
                };

                $(
                    let param_req = <$T::Kind>::param_requirement();
                    req.required_count += param_req.required_count;
                    if !param_req.exhaustive {
                        req.exhaustive = false;
                    }
                )*
                req
            }
        }
    }
}

// Helper macro to count identifiers
macro_rules! count_idents {
    () => { 0 };
    ($head:ident $(,$tail:ident)*) => { 1 + count_idents!($($tail),*) };
}

// Implement for common tuple sizes
impl_from_params!();
impl_from_params!(A);
impl_from_params!(A, B);
impl_from_params!(A, B, C);
impl_from_params!(A, B, C, D);
impl_from_params!(A, B, C, D, E);
impl_from_params!(A, B, C, D, E, F);
impl_from_params!(A, B, C, D, E, F, G);
impl_from_params!(A, B, C, D, E, F, G, H);
