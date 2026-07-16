use crate::{
    FromJSValue, IntoJSValue, JSContext, JSContextImpl, JSErrorFactory, JSFunc, JSObject,
    JSObjectOps, JSResult, JSTypeOf, JSValue, JSValueImpl, PromiseHandlerRegistration, RongJSError,
    function::JSParameterType,
};
use std::cell::RefCell;
use std::future::Future;
use std::marker::PhantomData;
use std::ops::Deref;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

/// Type alias for the return value of `promise()` function
type PromiseResult<V> = Result<(Promise<V>, JSFunc<V>, JSFunc<V>), RongJSError>;

/// Represents a JavaScript Promise object.
///
/// This struct wraps a JavaScript Promise and provides methods to interact with it.
pub struct Promise<V: JSValueImpl> {
    obj: JSObject<V>,
}

impl<V: JSValueImpl> Clone for Promise<V> {
    fn clone(&self) -> Self {
        Self {
            obj: self.obj.clone(),
        }
    }
}

impl<V: JSValueImpl> Deref for Promise<V> {
    type Target = JSObject<V>;
    fn deref(&self) -> &Self::Target {
        &self.obj
    }
}

impl<V> IntoJSValue<V> for Promise<V>
where
    V: JSValueImpl,
{
    fn into_js_value(self, _ctx: &JSContext<V::Context>) -> JSValue<V> {
        self.obj.into_js_value()
    }
}

impl<V> FromJSValue<V> for Promise<V>
where
    V: JSTypeOf,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        let obj = JSObject::from_js_value(ctx, value)?;
        Ok(Self { obj })
    }
}

impl<C: JSContextImpl> JSContext<C> {
    /// Creates a new JavaScript Promise and returns the Promise along with its resolve and reject functions.
    ///
    /// # Returns
    /// A `Result` containing a tuple of:
    /// - The `Promise` object
    /// - The resolve function (`JSFunc<V>`)
    /// - The reject function (`JSFunc<V>`)
    ///
    /// # Errors
    /// Returns a `RongJSError` if the Promise creation fails.
    pub fn promise(&self) -> PromiseResult<C::Value>
    where
        C::Value: JSTypeOf,
    {
        let (promise, resolver, reject) = self.as_ref().promise();
        let promise = JSObject::from_js_value(self, JSValue::from_raw(self, promise))?;
        let resolver = JSFunc::from_js_value(self, JSValue::from_raw(self, resolver))?;
        let reject = JSFunc::from_js_value(self, JSValue::from_raw(self, reject))?;
        Ok((Promise { obj: promise }, resolver, reject))
    }
}

impl<V: JSValueImpl + 'static> Promise<V>
where
    V: JSObjectOps,
{
    /// Creates a new JavaScript Promise using the provided context.
    ///
    /// # Arguments
    /// * `ctx` - The JavaScript context in which to create the Promise.
    ///
    /// # Returns
    /// A `Result` containing a tuple of:
    /// - The `Promise` object
    /// - The resolve function (`JSFunc<V>`)
    /// - The reject function (`JSFunc<V>`)
    ///
    /// # Errors
    /// Returns a `RongJSError` if the Promise creation fails.
    pub fn new(ctx: &JSContext<V::Context>) -> PromiseResult<V> {
        ctx.promise()
    }

    /// Converts a Rust Future into a JavaScript Promise.
    ///
    /// # Returns
    /// A `Result` containing the Promise object
    ///
    /// # Errors
    /// Returns a `RongJSError` if the Promise creation fails.
    pub fn from_future<F, R>(
        ctx: &JSContext<V::Context>,
        root: Option<V>,
        future: F,
    ) -> JSResult<Promise<V>>
    where
        F: Future<Output = R> + 'static,
        R: IntoJSValue<V> + 'static,
        R: PromiseResolver<V>,
    {
        let (promise, resolve, reject) = ctx.promise()?;

        // Spawn a new async task to handle the future and keep `root` alive
        ctx.spawn_task(async move {
            let result = future.await;
            // Keep the optional root alive until the future completes
            let _keep_root_alive = root;
            result.resolve_promise(resolve, reject);
        });

        Ok(promise)
    }

    /// Returns the `then` method of the Promise.
    ///
    /// This method is used to attach callbacks that will be called when the Promise is resolved.
    ///
    /// # Returns
    /// A `JSFunc<V>` representing the `then` method of the Promise.
    pub fn then(&self) -> JSResult<JSFunc<V>> {
        self.obj.get("then")
    }

    /// Returns the `catch` method of the Promise.
    ///
    /// This method is used to attach callbacks that will be called when the Promise is rejected.
    ///
    /// # Returns
    /// A `JSFunc<V>` representing the `catch` method of the Promise.
    pub fn catch(&self) -> JSResult<JSFunc<V>> {
        self.obj.get("catch")
    }

    pub fn into_object(self) -> JSObject<V> {
        self.obj
    }
}

/// Converts a Rust future result into JavaScript Promise resolution
/// using the provided resolve/reject callbacks
pub trait PromiseResolver<V: JSValueImpl> {
    fn resolve_promise(self, resolve: JSFunc<V>, reject: JSFunc<V>);
}

fn drain_microtasks<V>(ctx: &JSContext<V::Context>)
where
    V: JSValueImpl,
    <V::Context as crate::JSContextImpl>::Runtime: 'static,
{
    let _ = ctx.runtime().run_pending_jobs();
}

// Implement for RongJSError types
impl<V> PromiseResolver<V> for RongJSError
where
    V: JSObjectOps + crate::JSArrayOps,
    V::Context: JSErrorFactory,
    <V::Context as crate::JSContextImpl>::Runtime: 'static,
{
    fn resolve_promise(self, _resolve: JSFunc<V>, reject: JSFunc<V>) {
        let ctx = reject.context();
        let js_err = self.into_catch_value(&ctx);
        let _ = reject.call::<_, ()>(None, (js_err,));
        drain_microtasks::<V>(&ctx);
    }
}

// Implement for regular types
impl<V, T> PromiseResolver<V> for T
where
    T: IntoJSValue<V> + JSParameterType,
    V: JSObjectOps,
    <V::Context as crate::JSContextImpl>::Runtime: 'static,
{
    fn resolve_promise(self, resolve: JSFunc<V>, _reject: JSFunc<V>) {
        let ctx = resolve.context();
        let _ = resolve.call::<_, ()>(None, (self,));
        drain_microtasks::<V>(&ctx);
    }
}

// Specialized support for resolving Vec<T> without requiring JSParameterType on Vec
impl<V, T> PromiseResolver<V> for Vec<T>
where
    V: JSObjectOps + JSTypeOf + crate::JSArrayOps + 'static,
    T: IntoJSValue<V>,
    <V::Context as crate::JSContextImpl>::Runtime: 'static,
{
    fn resolve_promise(self, resolve: JSFunc<V>, _reject: JSFunc<V>) {
        let ctx = resolve.context();
        let arg = <Vec<T> as IntoJSValue<V>>::into_js_value(self, &ctx).into_value();
        let this = V::create_undefined(ctx.as_ref());
        let argv = [arg];
        let _ = ctx.as_ref().call(resolve.as_value(), this, &argv);
        drain_microtasks::<V>(&ctx);
    }
}

// Implement for JSResult types
impl<V, T> PromiseResolver<V> for JSResult<T>
where
    T: IntoJSValue<V>,
    V: JSObjectOps + crate::JSArrayOps,
    V::Context: JSErrorFactory,
    <V::Context as crate::JSContextImpl>::Runtime: 'static,
{
    fn resolve_promise(self, resolve: JSFunc<V>, reject: JSFunc<V>) {
        match self {
            Ok(value) => {
                let ctx = resolve.context();
                let arg = <T as IntoJSValue<V>>::into_js_value(value, &ctx).into_value();
                let this = V::create_undefined(ctx.as_ref());
                let argv = [arg];
                let _ = ctx.as_ref().call(resolve.as_value(), this, &argv);
                drain_microtasks::<V>(&ctx);
            }
            Err(err) => {
                let ctx = reject.context();
                let js_error_value = err.into_catch_value(&ctx).into_value();
                let this = V::create_undefined(ctx.as_ref());
                let argv = [js_error_value];
                let _ = ctx.as_ref().call(reject.as_value(), this, &argv);
                drain_microtasks::<V>(&ctx);
            }
        }
    }
}

pub struct PromiseFuture<V: JSValueImpl, T> {
    state: Option<Rc<RefCell<PromiseState<T>>>>,
    promise: Promise<V>,
    _marker: PhantomData<T>,
}

enum PromiseState<T> {
    Pending(Waker),
    Resolved(JSResult<T>),
}

impl<V: JSValueImpl, T> Unpin for PromiseFuture<V, T> {}

fn promise_state_is_pending<T>(state: &Rc<RefCell<PromiseState<T>>>) -> bool {
    matches!(&*state.borrow(), PromiseState::Pending(_))
}

fn attach_js_promise_handlers<V>(
    promise: &Promise<V>,
    resolve: &JSFunc<V>,
    reject: &JSFunc<V>,
) -> JSResult<()>
where
    V: JSValueImpl + JSObjectOps + 'static,
{
    promise
        .then()?
        .call::<_, ()>(Some(promise.obj.clone()), (resolve.clone(), reject.clone()))
}

fn register_future_handlers<V, T>(
    ctx: &JSContext<V::Context>,
    promise: &Promise<V>,
    resolve: &JSFunc<V>,
    reject: &JSFunc<V>,
    state: &Rc<RefCell<PromiseState<T>>>,
) -> JSResult<()>
where
    V: JSValueImpl + JSObjectOps + 'static,
    <V::Context as crate::JSContextImpl>::Runtime: 'static,
{
    match ctx.as_ref().register_promise_handlers(
        promise.obj.as_value(),
        resolve.as_value(),
        reject.as_value(),
    ) {
        PromiseHandlerRegistration::JavaScriptOnly => {
            attach_js_promise_handlers(promise, resolve, reject)?;
            drain_microtasks::<V>(ctx);
        }
        PromiseHandlerRegistration::NativeOnly => {
            drain_microtasks::<V>(ctx);
        }
        PromiseHandlerRegistration::NativeWithJavaScriptFallbackIfPending => {
            drain_microtasks::<V>(ctx);
            if promise_state_is_pending(state) {
                attach_js_promise_handlers(promise, resolve, reject)?;
                drain_microtasks::<V>(ctx);
            }
        }
    }

    Ok(())
}

impl<V: JSValueImpl + 'static> Promise<V> {
    /// Converts the Promise into a Future that resolves to a value of type T.
    ///
    /// # Example
    /// ```rust,no_run
    /// use rong_core::prelude::*;
    ///
    /// fn demo<E: JSEngine + 'static>() -> JSResult<()> {
    ///     let runtime = E::runtime();
    ///     let ctx = runtime.context();
    ///
    ///     let promise: Promise<E::Value> = ctx.eval(Source::from_bytes("Promise.resolve(1)"))?;
    ///     let _future = promise.into_future::<i32>();
    ///     Ok(())
    /// }
    /// ```
    pub fn into_future<T>(self) -> PromiseFuture<V, T>
    where
        T: FromJSValue<V> + 'static,
    {
        PromiseFuture {
            state: None,
            promise: self,
            _marker: PhantomData::<T>,
        }
    }
}

impl<V: JSValueImpl + 'static, T> Future for PromiseFuture<V, T>
where
    T: FromJSValue<V> + 'static,
    V: JSObjectOps,
{
    type Output = JSResult<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        // First poll: Initialize state and setup callbacks
        if this.state.is_none() {
            // Create initial state with waker
            let state = Rc::new(RefCell::new(PromiseState::Pending(cx.waker().clone())));
            this.state = Some(state.clone());

            let ctx = &this.promise.obj.context();

            // Clone state for callbacks
            let state = this.state.clone().unwrap();

            // resolved callback used to wake up future and save resolved value
            let resolve_state = state.clone();
            let resolve = JSFunc::new(ctx, move |ctx: JSContext<V::Context>, value: JSValue<V>| {
                //println!("resolve callback called");
                let mut state = resolve_state.borrow_mut();
                if !matches!(&*state, PromiseState::Pending(_)) {
                    return;
                }

                let resolved = T::from_js_value(&ctx, value);

                if let PromiseState::Pending(waker) =
                    std::mem::replace(&mut *state, PromiseState::Resolved(resolved))
                {
                    waker.wake_by_ref();
                }
            })
            .unwrap();

            // rejected callback used to wake up future and save rejected value
            let reject_state = state.clone();
            let reject = JSFunc::new(
                ctx,
                move |_ctx: JSContext<V::Context>, reason: JSValue<V>| {
                    //println!("reject callback called");
                    let mut state = reject_state.borrow_mut();
                    if !matches!(&*state, PromiseState::Pending(_)) {
                        return;
                    }
                    if let PromiseState::Pending(waker) = std::mem::replace(
                        &mut *state,
                        PromiseState::Resolved(Err(RongJSError::from_thrown_value(reason))),
                    ) {
                        waker.wake_by_ref();
                    }
                },
            )
            .unwrap();

            register_future_handlers(ctx, &this.promise, &resolve, &reject, &state)?;

            // If the callback already fired during the microtask drain above,
            // return the result immediately instead of pending forever.
            if let Some(state) = &this.state {
                let s = state.borrow();
                if matches!(&*s, PromiseState::Resolved(_)) {
                    drop(s);
                    let mut s = state.borrow_mut();
                    match std::mem::replace(&mut *s, PromiseState::Pending(cx.waker().clone())) {
                        PromiseState::Resolved(result) => return Poll::Ready(result),
                        other => {
                            *s = other;
                        }
                    }
                }
            }

            return Poll::Pending;
        }

        if let Some(state) = &this.state {
            let mut state = state.borrow_mut();

            // Then check Promise state
            match &*state {
                PromiseState::Resolved(Ok(_)) => {
                    if let PromiseState::Resolved(Ok(success)) =
                        std::mem::replace(&mut *state, PromiseState::Pending(cx.waker().clone()))
                    {
                        return Poll::Ready(Ok(success));
                    }
                }
                PromiseState::Resolved(Err(_)) => {
                    if let PromiseState::Resolved(Err(err)) =
                        std::mem::replace(&mut *state, PromiseState::Pending(cx.waker().clone()))
                    {
                        return Poll::Ready(Err(err));
                    }
                }
                PromiseState::Pending(_) => {
                    // Update the waker
                    *state = PromiseState::Pending(cx.waker().clone());
                }
            }
        }

        Poll::Pending
    }
}
