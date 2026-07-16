use crate::{
    IntoJSValue, JSArrayOps, JSContext, JSErrorFactory, JSFunc, JSObject, JSObjectOps, JSResult,
    JSSymbol, JSTypeOf, JSValue, RongJSError, function::This,
};
use futures::{Stream, StreamExt};
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use tokio::sync::Mutex;

/// Core JavaScript iterator that wraps Rust iterators.
///
/// The returned object is both an **iterator** (has `next()`) and an **iterable**
/// (has `[Symbol.iterator]`), following the standard self-referential pattern.
/// This means you can use it directly with `for...of` or call `.next()` manually.
///
/// # Quick usage
/// ```ignore
/// // From any IntoIterator:
/// let iter = vec!["a", "b", "c"].to_js_iter(&ctx)?;
///
/// // Install on an existing object instead:
/// vec!["a", "b", "c"].install_js_iter(&ctx, &my_obj)?;
/// ```
pub struct JSIterator<V, T>
where
    V: JSObjectOps + 'static,
    T: IntoJSValue<V> + 'static,
{
    inner: Rc<RefCell<Box<dyn Iterator<Item = T> + 'static>>>,
    ctx: JSContext<V::Context>,
}

impl<V, T> JSIterator<V, T>
where
    V: JSObjectOps + 'static,
    T: IntoJSValue<V> + 'static,
{
    pub fn new<I>(iterable: I, ctx: &JSContext<V::Context>) -> Self
    where
        I: IntoIterator<Item = T> + 'static,
        I::IntoIter: 'static,
    {
        Self {
            inner: Rc::new(RefCell::new(Box::new(iterable.into_iter()))),
            ctx: ctx.clone(),
        }
    }

    pub fn next(&self) -> JSResult<JSObject<V>> {
        let result = JSObject::new(&self.ctx);
        let mut iter = self.inner.borrow_mut();

        match iter.next() {
            Some(item) => {
                result.set("done", false)?;
                let value = <T as IntoJSValue<V>>::into_js_value(item, &self.ctx);
                result.set("value", value)?;
            }
            None => {
                result.set("done", true)?;
                result.set("value", JSValue::undefined(&self.ctx))?;
            }
        }

        Ok(result)
    }

    /// Install `next()`, `return()`, and `[Symbol.iterator]` on an existing JS object.
    ///
    /// `return()` signals early termination (e.g. `break` in `for...of`)
    /// by replacing the underlying iterator with an empty one.
    pub fn install_on(&self, ctx: &JSContext<V::Context>, obj: &JSObject<V>) -> JSResult<()> {
        // next()
        let iterator_instance = self.clone();
        let next_fn = JSFunc::new(ctx, move |_ctx: JSContext<V::Context>| -> JSObject<V> {
            iterator_instance.next().unwrap_or_else(|_| {
                let result = JSObject::new(&iterator_instance.ctx);
                result.set("done", true).ok();
                result
                    .set("value", JSValue::undefined(&iterator_instance.ctx))
                    .ok();
                result
            })
        })?;
        obj.set("next", next_fn)?;

        // return() — for early termination (break in for-of)
        let inner_handle = self.inner.clone();
        let return_fn = JSFunc::new(ctx, move |ctx: JSContext<V::Context>| -> JSObject<V> {
            // Replace with an empty iterator to release resources
            *inner_handle.borrow_mut() = Box::new(std::iter::empty());
            let result = JSObject::new(&ctx);
            result.set("done", true).ok();
            result.set("value", JSValue::undefined(&ctx)).ok();
            result
        })?;
        obj.set("return", return_fn)?;

        // [Symbol.iterator] = () => this
        let symbol = ctx
            .global()
            .get::<_, JSObject<V>>("Symbol")?
            .get::<_, JSSymbol<V>>("iterator")?;
        obj.set(
            symbol,
            JSFunc::new(ctx, |this: This<JSObject<V>>| this.0.clone())?,
        )?;
        Ok(())
    }

    /// Create a new JS object that is both iterator and iterable.
    pub fn to_js_iterable(&self, ctx: &JSContext<V::Context>) -> JSResult<JSObject<V>> {
        let obj = JSObject::new(ctx);
        self.install_on(ctx, &obj)?;
        Ok(obj)
    }
}

impl<V, T> Clone for JSIterator<V, T>
where
    V: JSObjectOps + 'static,
    T: IntoJSValue<V> + 'static,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            ctx: self.ctx.clone(),
        }
    }
}

/// Core JavaScript async iterator that wraps Rust streams.
///
/// The returned object is both an **async iterator** (has `next()`) and an
/// **async iterable** (has `[Symbol.asyncIterator]`).
///
/// # Quick usage
/// ```ignore
/// // From any Stream:
/// let iter = my_stream.to_js_async_iter(&ctx)?;
///
/// // Install on an existing object (e.g. ReadableStream):
/// my_stream.install_js_async_iter(&ctx, &stream_obj)?;
/// ```
pub struct JSAsyncIterator<V, T>
where
    V: JSObjectOps + 'static,
    T: IntoJSValue<V> + 'static,
{
    stream: Rc<Mutex<Pin<Box<dyn Stream<Item = T> + 'static>>>>,
    ctx: JSContext<V::Context>,
}

impl<V, T> JSAsyncIterator<V, T>
where
    V: JSObjectOps + JSArrayOps + JSTypeOf + 'static,
    T: IntoJSValue<V> + 'static,
    V::Context: JSErrorFactory,
{
    pub fn new<S>(stream: S, ctx: &JSContext<V::Context>) -> Self
    where
        S: Stream<Item = T> + 'static,
    {
        Self {
            stream: Rc::new(Mutex::new(Box::pin(stream))),
            ctx: ctx.clone(),
        }
    }

    async fn next_item(stream: Rc<Mutex<Pin<Box<dyn Stream<Item = T> + 'static>>>>) -> Option<T> {
        let mut stream = stream.lock().await;
        stream.next().await
    }

    fn result_from_item(ctx: &JSContext<V::Context>, item: Option<T>) -> JSResult<JSObject<V>> {
        let result = JSObject::new(ctx);

        match item {
            Some(item) => {
                result.set("done", false)?;
                let value = <T as IntoJSValue<V>>::into_js_value(item, ctx);
                if value.is_exception() {
                    return Err(RongJSError::from_thrown_value(value));
                }
                result.set("value", value)?;
            }
            None => {
                result.set("done", true)?;
                result.set("value", JSValue::undefined(ctx))?;
            }
        }

        Ok(result)
    }

    pub async fn next(&self) -> JSResult<JSObject<V>> {
        Self::result_from_item(&self.ctx, Self::next_item(self.stream.clone()).await)
    }

    /// Install `next()`, `return()`, and `[Symbol.asyncIterator]` on an existing JS object.
    ///
    /// `return()` signals early termination (e.g. `break` in `for await...of`)
    /// by dropping the underlying stream.
    pub fn install_on(&self, ctx: &JSContext<V::Context>, obj: &JSObject<V>) -> JSResult<()> {
        // next()
        let stream_handle = self.stream.clone();
        let next_fn = JSFunc::new(ctx, move |ctx: JSContext<V::Context>| -> JSObject<V> {
            match ctx.promise() {
                Ok((promise, resolve, reject)) => {
                    let stream = stream_handle.clone();
                    ctx.spawn_task(async move {
                        let item = Self::next_item(stream).await;
                        let task_ctx = resolve.context();
                        match Self::result_from_item(&task_ctx, item) {
                            Ok(result) => {
                                let _ = resolve.call::<_, ()>(None, (result,));
                            }
                            Err(e) => {
                                let err = e.into_catch_value(&task_ctx);
                                let _ = reject.call::<_, ()>(None, (err,));
                            }
                        }
                    });
                    promise.into_object()
                }
                Err(_) => {
                    let result = JSObject::new(&ctx);
                    result.set("done", true).ok();
                    result.set("value", JSValue::undefined(&ctx)).ok();
                    result
                }
            }
        })?;
        obj.set("next", next_fn)?;

        // return() — for early termination (break in for-await-of)
        let stream_handle = self.stream.clone();
        let return_fn = JSFunc::new(ctx, move |ctx: JSContext<V::Context>| -> JSObject<V> {
            let stream = stream_handle.clone();
            match ctx.promise() {
                Ok((promise, resolve, _reject)) => {
                    ctx.spawn_task(async move {
                        // Lock and replace with an empty stream to release resources
                        let mut guard = stream.lock().await;
                        *guard = Box::pin(futures::stream::empty());
                        let task_ctx = resolve.context();
                        let result = JSObject::new(&task_ctx);
                        result.set("done", true).ok();
                        result.set("value", JSValue::undefined(&task_ctx)).ok();
                        let _ = resolve.call::<_, ()>(None, (result,));
                    });
                    promise.into_object()
                }
                Err(_) => {
                    let result = JSObject::new(&ctx);
                    result.set("done", true).ok();
                    result.set("value", JSValue::undefined(&ctx)).ok();
                    result
                }
            }
        })?;
        obj.set("return", return_fn)?;

        // [Symbol.asyncIterator] = () => this
        let symbol = ctx
            .global()
            .get::<_, JSObject<V>>("Symbol")?
            .get::<_, JSSymbol<V>>("asyncIterator")?;
        obj.set(
            symbol,
            JSFunc::new(ctx, |this: This<JSObject<V>>| this.0.clone())?,
        )?;
        Ok(())
    }

    /// Create a new JS object that is both async iterator and async iterable.
    pub fn to_js_async_iterable(&self, ctx: &JSContext<V::Context>) -> JSResult<JSObject<V>> {
        let obj = JSObject::new(ctx);
        self.install_on(ctx, &obj)?;
        Ok(obj)
    }
}

impl<V, T> Clone for JSAsyncIterator<V, T>
where
    V: JSObjectOps + 'static,
    T: IntoJSValue<V> + 'static,
{
    fn clone(&self) -> Self {
        Self {
            stream: self.stream.clone(),
            ctx: self.ctx.clone(),
        }
    }
}

/// Extension trait for converting iterables to JavaScript iterators.
pub trait IntoJSIteratorExt<V, T>
where
    V: JSObjectOps + 'static,
{
    /// Create a new JS iterator/iterable object from this Rust iterator.
    fn to_js_iter(self, ctx: &JSContext<V::Context>) -> JSResult<JSObject<V>>;

    /// Install `next()`, `return()`, and `[Symbol.iterator]` on an existing JS object.
    fn install_js_iter(self, ctx: &JSContext<V::Context>, obj: &JSObject<V>) -> JSResult<()>;
}

impl<V, T, I> IntoJSIteratorExt<V, T> for I
where
    V: JSObjectOps + 'static,
    T: IntoJSValue<V> + 'static,
    I: IntoIterator<Item = T> + 'static,
    I::IntoIter: 'static,
{
    fn to_js_iter(self, ctx: &JSContext<V::Context>) -> JSResult<JSObject<V>> {
        let js_iter = JSIterator::new(self, ctx);
        js_iter.to_js_iterable(ctx)
    }

    fn install_js_iter(self, ctx: &JSContext<V::Context>, obj: &JSObject<V>) -> JSResult<()> {
        let js_iter = JSIterator::new(self, ctx);
        js_iter.install_on(ctx, obj)
    }
}

/// Extension trait for converting streams to JavaScript async iterators.
pub trait IntoJSAsyncIteratorExt<V, T>
where
    V: JSObjectOps + 'static,
{
    /// Create a new JS async iterator/iterable object from this Rust stream.
    fn to_js_async_iter(self, ctx: &JSContext<V::Context>) -> JSResult<JSObject<V>>;

    /// Install `next()`, `return()`, and `[Symbol.asyncIterator]` on an existing JS object.
    fn install_js_async_iter(self, ctx: &JSContext<V::Context>, obj: &JSObject<V>) -> JSResult<()>;
}

impl<V, T, S> IntoJSAsyncIteratorExt<V, T> for S
where
    V: JSObjectOps + JSArrayOps + JSTypeOf + 'static,
    T: IntoJSValue<V> + 'static,
    S: Stream<Item = T> + 'static,
    V::Context: JSErrorFactory,
{
    fn to_js_async_iter(self, ctx: &JSContext<V::Context>) -> JSResult<JSObject<V>> {
        let js_iter = JSAsyncIterator::new(self, ctx);
        js_iter.to_js_async_iterable(ctx)
    }

    fn install_js_async_iter(self, ctx: &JSContext<V::Context>, obj: &JSObject<V>) -> JSResult<()> {
        let js_iter = JSAsyncIterator::new(self, ctx);
        js_iter.install_on(ctx, obj)
    }
}

/// Install `[Symbol.asyncIterator]` on an existing JS object (self-referential).
///
/// This is a lightweight helper for objects that already have a `next()` method
/// and just need the symbol to become async-iterable.
pub fn install_async_iterator_symbol<V: JSObjectOps + 'static>(
    ctx: &JSContext<V::Context>,
    obj: &JSObject<V>,
) -> JSResult<()> {
    let symbol = ctx
        .global()
        .get::<_, JSObject<V>>("Symbol")?
        .get::<_, JSSymbol<V>>("asyncIterator")?;
    obj.set(
        symbol,
        JSFunc::new(ctx, |this: This<JSObject<V>>| this.0.clone())?,
    )?;
    Ok(())
}

/// Install `[Symbol.iterator]` on an existing JS object (self-referential).
///
/// This is a lightweight helper for objects that already have a `next()` method
/// and just need the symbol to become iterable.
pub fn install_iterator_symbol<V: JSObjectOps + 'static>(
    ctx: &JSContext<V::Context>,
    obj: &JSObject<V>,
) -> JSResult<()> {
    let symbol = ctx
        .global()
        .get::<_, JSObject<V>>("Symbol")?
        .get::<_, JSSymbol<V>>("iterator")?;
    obj.set(
        symbol,
        JSFunc::new(ctx, |this: This<JSObject<V>>| this.0.clone())?,
    )?;
    Ok(())
}
