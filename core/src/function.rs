use crate::{
    HostError, IntoJSValue, JSClass, JSObjectOps, JSResult, JSValueImpl, Promise, PromiseResolver,
};
use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;

mod parameter;
pub use parameter::{
    FromParams, JSClassRef, JSParameterType, Optional, ParamsAccessor, Rest, This, ThisMut,
};

/// Container to hold rust closure/function that's callable from JS.
///
/// Supports various parameter types:
/// - Regular parameters (i32, String, etc.)
/// - This<T> for capturing JS `this` context
/// - Optional<T> for optional parameters
/// - Rest<T> for rest parameters
///
/// Example:
/// ```ignore
/// // Sync function
/// RustFunc::new(|x: i32| x + 1);
///
/// // Async function
/// RustFunc::new(async |x: i32| {
///     // async operation
///     x + 1
/// });
/// ```
pub(crate) struct RustFunc<V: JSValueImpl> {
    func: JSCallable<V>,
    required_params: u32,
}

/// Type alias for Fn closure type
type FnClosure<V> = dyn Fn(&mut ParamsAccessor<V>) -> JSResult<V>;

/// Type alias for FnOnce closure type
type FnOnceClosure<V> = dyn FnOnce(&mut ParamsAccessor<V>) -> JSResult<V>;

// A multi-call function is `Fn` so a call can run while another call of the
// same function is still on the stack: shared, it is cloned out for each call.
pub enum JSCallable<V: JSValueImpl> {
    Fn(Rc<FnClosure<V>>),
    FnOnce(RefCell<Option<Box<FnOnceClosure<V>>>>),
}

/// One call's hold on a function, taken without keeping the function borrowed.
pub(crate) enum Invocation<V: JSValueImpl> {
    Fn(Rc<FnClosure<V>>),
    FnOnce(Box<FnOnceClosure<V>>),
}

impl<V: JSValueImpl> Invocation<V> {
    pub(crate) fn call(self, accessor: &mut ParamsAccessor<V>) -> JSResult<V> {
        match self {
            Self::Fn(f) => f(accessor),
            Self::FnOnce(f) => f(accessor),
        }
    }
}

/// Trait for converting Rust functions into JavaScript callable functions.
/// Type parameters:
/// - V: The JavaScript value type
/// - P: Parameter types tuple
/// - K: marker type to avoid rustc complain confiction implementation
#[doc(hidden)]
pub trait IntoJSCallable<V: JSValueImpl, P, K> {
    fn into_js_callable(self) -> JSCallable<V>;
}

/// same as IntoJSCallable, but it's for once callable function
#[doc(hidden)]
pub trait IntoOnceJSCallable<V: JSValueImpl, P, K> {
    fn into_js_callable(self) -> JSCallable<V>;
}

/// Marker type to let rustc happy to avoid it complain confliction implementation
/// since Fn depends on FnMut, and FnMut depends on FnOnce, when P is the same, rustc
/// consider confliction implementation for Fn,FnMut etc.
#[doc(hidden)]
pub struct KFnMut;
#[doc(hidden)]
pub struct KFnOnce;
#[doc(hidden)]
pub struct KAsyncFnMut;
#[doc(hidden)]
pub struct KAsyncFnOnce;

impl<V: JSValueImpl> RustFunc<V> {
    /// Creates a new RustFunc instance for wrapping a multi-callable Rust closure/function
    pub(crate) fn new<F, P, K>(f: F) -> Self
    where
        F: IntoJSCallable<V, P, K>,
        P: FromParams<V>,
    {
        let required_params = P::param_requirements().required_count() as u32;
        Self {
            func: f.into_js_callable(),
            required_params,
        }
    }

    pub(crate) fn new_once<F, P, K>(f: F) -> Self
    where
        F: IntoOnceJSCallable<V, P, K>,
        P: FromParams<V>,
    {
        let required_params = P::param_requirements().required_count() as u32;
        Self {
            func: f.into_js_callable(),
            required_params,
        }
    }

    /// Calls the function with provided parameter accessor, returning JS result
    pub(crate) fn call(&self, accessor: &mut ParamsAccessor<V>) -> JSResult<V> {
        self.invocation(accessor)?.call(accessor)
    }

    /// Checks the arguments and takes what one call needs, so the caller can
    /// release its borrow of the function before running it.
    pub(crate) fn invocation(&self, accessor: &ParamsAccessor<V>) -> JSResult<Invocation<V>> {
        let num_args = accessor.args_len() as u32;
        if num_args < self.required_params {
            return Err(HostError::invalid_arg_count(self.required_params, num_args).into());
        }

        match &self.func {
            JSCallable::Fn(f) => Ok(Invocation::Fn(f.clone())),
            JSCallable::FnOnce(f) => f
                .take()
                .map(Invocation::FnOnce)
                .ok_or_else(|| HostError::once_fn_called().into()),
        }
    }

    /// Returns the number of required parameters for the function
    pub(crate) fn parameter_required_count(&self) -> u32 {
        self.required_params
    }
}

/// A wrapper for a Rust function that can be used to handles both class construction (`new T()`)
/// and implicit constructor invocation(`T()`) behaviors in JavaScript.
///
/// This struct encapsulates a `RustFunc` and provides a way to create new instances
/// of JavaScript objects using Rust functions. It is designed to be used with the
/// `JSClass` trait to define JavaScript classes with Rust constructors.
///
/// # Type Parameters
///
/// * `V`: The JavaScript value type that the constructor will work with. This type
///   must implement the `JSValueImpl` trait.
pub struct Constructor<V: JSValueImpl>(pub(crate) RustFunc<V>);

impl<V: JSValueImpl> Constructor<V> {
    pub fn new<F, P, K>(f: F) -> Self
    where
        F: IntoJSCallable<V, P, K>,
        P: FromParams<V>,
    {
        Self(RustFunc::new(f))
    }

    /// Calls the constructor with provided parameter accessor, returning JS result
    pub fn call(&mut self, accessor: &mut ParamsAccessor<V>) -> JSResult<V> {
        self.0.call(accessor)
    }
}

impl<V> JSClass<V> for RustFunc<V>
where
    V: JSValueImpl + crate::JSObjectOps + 'static,
{
    const NAME: &'static str = "RustFunc";
    const CALLABLE: bool = true;

    fn data_constructor() -> Constructor<V> {
        // RustFunction class don't need data constructor
        panic!("Never 'new RustFunc()' in JS");
    }

    fn class_setup(class: &crate::ClassSetup<V>) -> JSResult<()> {
        // Ensure host functions behave like real JS Function:
        // RustFunc.prototype -> Function.prototype so .call/.apply/bind exist
        let fn_proto = class
            .context()
            .global()
            .get::<_, crate::JSObject<V>>("Function")?
            .get::<_, crate::JSObject<V>>("prototype")?;
        class.prototype_object().prototype(fn_proto);
        Ok(())
    }
}

macro_rules! impl_js_callable_func {
    ($($t:ident),* $(,)?) => {
        // Sync FnMut function/closure implementation
        impl<V, R, Fun $(,$t)*> IntoJSCallable<V, ($($t,)*), KFnMut> for Fun
        where
            Fun: Fn($($t),*) -> R + 'static,
            V: JSValueImpl,
            ($($t,)*): FromParams<V>,
            R: IntoJSValue<V>
        {
            fn into_js_callable(self) -> JSCallable<V> {
                let f = self;
                let closure = move |accessor: &mut ParamsAccessor<V>| {
                    let params = <($($t,)*)>::from_params(accessor)?;
                    #[allow(non_snake_case)]
                    let ($($t,)*) = params;
                    let result = f($($t),*);
                    Ok(<R as IntoJSValue<V>>::into_js_value(result, accessor.context()).into_value())
                };
                JSCallable::Fn(Rc::new(closure))
            }
        }


        // Async FnMut implementation
        impl<V, R, Fut, Fun $(,$t)*> IntoJSCallable<V, ($($t,)*), KAsyncFnMut> for Fun
        where
            Fun: Fn($($t),*) -> Fut + 'static,
            Fut: Future<Output=R> +'static,
            V: JSValueImpl + JSObjectOps + 'static,
            ($($t,)*): FromParams<V> ,
            R: IntoJSValue<V> + PromiseResolver<V> + 'static,
        {
            fn into_js_callable(self) -> JSCallable<V> {
                let f = self;
                let closure = move |accessor: &mut ParamsAccessor<V>| {
                    let params = <($($t,)*)>::from_params(accessor)?;
                    #[allow(non_snake_case)]
                    let ($($t,)*) = params;
                    // Keep `this` alive for the future's lifetime
                    let this = accessor.get_this();
                    let fut = f($($t),*);
                    let ctx = accessor.context();
                    let promise = Promise::from_future(ctx, Some(this), fut)?;
                    Ok(<Promise<V> as IntoJSValue<V>>::into_js_value(promise, ctx).into_value())
                };
                JSCallable::Fn(Rc::new(closure))
            }
        }
    };
 }

macro_rules! impl_js_oncecallable_func {
    ($($t:ident),* $(,)?) => {
        // FnOnce function/closure implementation
        impl<V, R, Fun $(,$t)*> IntoOnceJSCallable<V, ($($t,)*), KFnOnce> for Fun
        where
            Fun: FnOnce($($t),*) -> R + 'static,
            V: JSValueImpl,
            ($($t,)*): FromParams<V>,
            R: IntoJSValue<V>
        {
            fn into_js_callable(self) -> JSCallable<V> {
                let f = self;
                let closure = move |accessor: &mut ParamsAccessor<V>| {
                    let params = <($($t,)*)>::from_params(accessor)?;
                    #[allow(non_snake_case)]
                    let ($($t,)*) = params;
                    let result = f($($t),*);
                    Ok(<R as IntoJSValue<V>>::into_js_value(result, accessor.context()).into_value())
                };
                JSCallable::FnOnce(RefCell::new(Some(Box::new(closure))))
            }
        }

        // Async Fn implementation
        impl<V, R,Fut, Fun $(,$t)*> IntoOnceJSCallable<V, ($($t,)*), KAsyncFnOnce> for Fun
        where
            Fun: FnOnce($($t),*) -> Fut + 'static,
            Fut: Future<Output=R> +'static,
            V: JSValueImpl + JSObjectOps + 'static,
            ($($t,)*): FromParams<V>,
            R: IntoJSValue<V> + PromiseResolver<V> + 'static,
        {
            fn into_js_callable(self) -> JSCallable<V> {
                let f = self;
                let closure = move |accessor: &mut ParamsAccessor<V>| {
                    let params = <($($t,)*)>::from_params(accessor)?;
                    #[allow(non_snake_case)]
                    let ($($t,)*) = params;
                    // Keep `this` alive for the future's lifetime
                    let this = accessor.get_this();
                    let fut = f($($t),*);
                    let ctx = accessor.context();
                    let promise = Promise::from_future(ctx, Some(this), fut)?;
                    Ok(<Promise<V> as IntoJSValue<V>>::into_js_value(promise, ctx).into_value())
                };
                JSCallable::FnOnce(RefCell::new(Some(Box::new(closure))))
            }
        }
    };
 }

impl_js_callable_func!();
impl_js_callable_func!(P1);
impl_js_callable_func!(P1, P2);
impl_js_callable_func!(P1, P2, P3);
impl_js_callable_func!(P1, P2, P3, P4);
impl_js_callable_func!(P1, P2, P3, P4, P5);
impl_js_callable_func!(P1, P2, P3, P4, P5, P6);
impl_js_callable_func!(P1, P2, P3, P4, P5, P6, P7);
impl_js_callable_func!(P1, P2, P3, P4, P5, P6, P7, P8);

impl_js_oncecallable_func!();
impl_js_oncecallable_func!(P1);
impl_js_oncecallable_func!(P1, P2);
impl_js_oncecallable_func!(P1, P2, P3);
impl_js_oncecallable_func!(P1, P2, P3, P4);
impl_js_oncecallable_func!(P1, P2, P3, P4, P5);
impl_js_oncecallable_func!(P1, P2, P3, P4, P5, P6);
impl_js_oncecallable_func!(P1, P2, P3, P4, P5, P6, P7);
impl_js_oncecallable_func!(P1, P2, P3, P4, P5, P6, P7, P8);
