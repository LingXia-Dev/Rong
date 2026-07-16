use crate::function::{FromParams, IntoJSCallable, IntoOnceJSCallable, JSParameterType, RustFunc};
use crate::{
    Class, FromJSValue, HostError, IntoJSValue, JSContext, JSContextImpl, JSObject, JSObjectOps,
    JSResult, JSTypeOf, JSValue, JSValueImpl, JSValueMapper, Promise, PropertyDescriptor,
};
use std::ops::Deref;

mod args;
pub use args::IntoJSArgs;

#[derive(PartialEq, Eq, Hash)]
pub struct JSFunc<V: JSValueImpl>(JSObject<V>);

impl<V: JSValueImpl> Deref for JSFunc<V> {
    type Target = JSObject<V>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<V: JSValueImpl> Clone for JSFunc<V> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<V> IntoJSValue<V> for JSFunc<V>
where
    V: JSValueImpl,
{
    fn into_js_value(self, _ctx: &JSContext<V::Context>) -> JSValue<V> {
        self.0.into_js_value()
    }
}

impl<V> FromJSValue<V> for JSFunc<V>
where
    V: JSTypeOf,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        if value.is_function() {
            JSObject::from_js_value(ctx, value).map(Self)
        } else {
            Err(HostError::not_function().into())
        }
    }
}

impl<V: JSObjectOps> JSFunc<V> {
    fn call_with_argv(&self, this: Option<JSObject<V>>, argv: &[V]) -> V {
        let ctx = &self.context();
        let this = match this {
            Some(obj) => obj.into_value(),
            None => V::create_undefined(ctx.as_ref()),
        };
        ctx.as_ref().call(self.as_value(), this, argv)
    }

    fn call_raw<Args>(&self, this: Option<JSObject<V>>, args: Args) -> V
    where
        Args: IntoJSArgs<V>,
    {
        let ctx = &self.context();
        let argv = args.into_js_args(ctx);
        self.call_with_argv(this, &argv)
    }

    /// Create a new JavaScript function from a Rust function or closure
    ///
    /// # Arguments
    /// * `ctx` - The JavaScript context to create the function in
    /// * `f` - The Rust function or closure to wrap
    ///
    /// # Type Parameters
    /// * `F` - The function type implementing `IntoJSCallable`
    /// * `P` - The parameter type implementing `FromParams`
    /// * `K` - The function kind (SyncFunc or AsyncFunc)
    ///
    /// # Returns
    /// Returns `JSResult<Self>` containing the new JS function if successful
    ///
    /// # Example
    /// ```rust,no_run
    /// use rong_core::prelude::*;
    ///
    /// fn demo<E: JSEngine + 'static>() -> JSResult<()> {
    ///     let runtime = E::runtime();
    ///     let ctx = runtime.context();
    ///     let _func = JSFunc::<E::Value>::new(&ctx, |x: i32| x + 1)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn new<F, P, K>(ctx: &JSContext<V::Context>, f: F) -> JSResult<Self>
    where
        F: IntoJSCallable<V, P, K>,
        P: FromParams<V>,
        V: 'static,
    {
        RustFunc::new(f).into_js(ctx)
    }

    pub fn new_once<F, P, K>(ctx: &JSContext<V::Context>, f: F) -> JSResult<Self>
    where
        F: IntoOnceJSCallable<V, P, K>,
        P: FromParams<V>,
        V: 'static,
    {
        RustFunc::new_once(f).into_js(ctx)
    }

    /// Calls the JavaScript function with the given arguments.
    ///
    /// # Arguments
    /// * `args` - Arguments to pass to the function. Can be:
    ///   - A single value implementing `IntoJSArg`
    ///   - A tuple of values implementing `IntoJSArg` (up to 12 arguments)
    ///
    /// # Returns
    /// Returns `Ok(R)` if the call succeeds, where `R` is the return type.
    /// Returns `Err(RongJSError)` if the call fails or throws an exception.
    ///
    /// # Examples
    /// ```rust,no_run
    /// use rong_core::prelude::*;
    ///
    /// fn demo<E: JSEngine + 'static>() -> JSResult<()> {
    ///     let runtime = E::runtime();
    ///     let ctx = runtime.context();
    ///     let func = JSFunc::<E::Value>::new(&ctx, |x: i32| x + 1)?;
    ///
    ///     // Call with single argument
    ///     let _result: i32 = func.call(None, (42,))?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn call<Args, R>(&self, this: Option<JSObject<V>>, args: Args) -> JSResult<R>
    where
        Args: IntoJSArgs<V>,
        R: FromJSValue<V>,
        V: JSObjectOps,
    {
        let result = self.call_raw(this, args);
        result.try_convert::<R>()
    }

    /// Calls the JavaScript function asynchronously with the given arguments.
    /// If the function returns a Promise, waits for it to resolve.
    ///
    /// # Arguments
    /// * `this` - Optional this value for the function call
    /// * `args` - Arguments to pass to the function. Can be:
    ///   - A single value implementing `IntoJSArg`
    ///   - A tuple of values implementing `IntoJSArg` (up to 12 arguments)
    ///
    /// # Returns
    /// Returns `Ok(R)` if the call succeeds, where `R` is the return type.
    /// Returns `Err(RongJSError)` if the call fails or throws an exception.
    ///
    /// # Examples
    /// ```rust,no_run
    /// use rong_core::prelude::*;
    ///
    /// async fn demo<E: JSEngine + 'static>() -> JSResult<()> {
    ///     let runtime = E::runtime();
    ///     let ctx = runtime.context();
    ///     let func = JSFunc::<E::Value>::new(&ctx, |x: i32| x + 1)?;
    ///
    ///     let _result: i32 = func.call_async(None, (42,)).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn call_async<Args, R>(&self, this: Option<JSObject<V>>, args: Args) -> JSResult<R>
    where
        Args: IntoJSArgs<V>,
        R: FromJSValue<V> + 'static,
        V: JSObjectOps + JSTypeOf + 'static,
    {
        let result = self.call_raw(this, args);

        if result.is_promise() {
            let promise = {
                let ctx = self.context();
                Promise::from_js_value(&ctx, JSValue::from_raw(&ctx, result))?
            };
            promise.into_future::<R>().await
        } else {
            result.try_convert::<R>()
        }
    }

    /// set name of JS Function
    pub fn name(self, name: &str) -> JSResult<Self> {
        let ctx = &self.0.context();
        let name_value = JSValue::from_rust(ctx, name);
        // Per JS spec, Function#name is non-writable, non-enumerable, configurable
        self.0.define_property(
            "name",
            PropertyDescriptor::from_value(name_value)
                .readonly()
                .hidden()
                .configurable(),
        )?;
        Ok(self)
    }

    pub(crate) fn into_value(self) -> V {
        self.0.into_value()
    }

    pub(crate) fn invoke_with_argv(&self, this: Option<JSObject<V>>, argv: &[V]) -> V {
        self.call_with_argv(this, argv)
    }
}

impl<V> RustFunc<V>
where
    V: JSObjectOps + 'static,
{
    fn into_js(self, ctx: &JSContext<V::Context>) -> JSResult<JSFunc<V>> {
        let length = self.parameter_required_count();
        let class = Class::lookup::<RustFunc<V>>(ctx)?;
        let obj = class.instance::<RustFunc<V>>(self);
        obj.define_property(
            "length",
            crate::PropertyDescriptor::from_rust(ctx, length as i32)
                .readonly()
                .hidden()
                .non_configurable(),
        )?;
        Ok(JSFunc(obj))
    }
}

// blanket implementing.
// Type JSFunc can be as parameter of JS callback of rust function
impl<V: JSValueImpl> JSParameterType for JSFunc<V> {}
