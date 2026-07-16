use self::thrown_store::{ThrownValueHandle, ThrownValueStore};
use crate::{
    ClassSetup, FromJSValue, HostError, JSClass, JSObject, JSObjectOps, JSResult, JSRuntimeImpl,
    JSTypeOf, JSValue, JSValueImpl, Promise, RongJSError,
    source::{Source, SourceKind},
};
use crate::{JSRuntime, JSValueMapper};
use std::any::TypeId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::rc::{Rc, Weak};
use std::sync::{LazyLock, RwLock};

pub(crate) mod thrown_store;

/// Describes how a JavaScript engine registered promise handlers.
///
/// `core` owns the polling state machine and microtask draining. Engines only
/// report which registration path they performed so the shared layer can apply
/// the right follow-up behavior.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PromiseHandlerRegistration {
    /// The engine did not install any native handlers. `core` must fall back to
    /// `promise.then(onFulfilled, onRejected)`.
    #[default]
    JavaScriptOnly,
    /// The engine installed native handlers and no JavaScript fallback is needed.
    NativeOnly,
    /// The engine installed native handlers, but `core` should install a
    /// JavaScript `.then()` backup if the promise is still pending after the
    /// first microtask drain.
    NativeWithJavaScriptFallbackIfPending,
}

/// JSContextImpl represents a JavaScript context
///
/// # Safety
/// The implementation must ensure that:
/// 1. Value type implements Drop to properly clean up resources
/// 2. Context type implements Drop if it holds any resources that need cleanup
pub trait JSContextImpl {
    /// The JavaScript engine's native context type.
    ///
    /// This represents the raw, engine-specific context handle that is used internally
    type RawContext;

    /// The runtime type associated with this context.
    ///
    /// This specifies the JavaScript runtime implementation that this context belongs to.
    /// The runtime must implement JSRuntimeImpl and have its Context type set to Self.
    type Runtime: JSRuntimeImpl<Context = Self>;

    /// The JavaScript value type associated with this context.
    ///
    /// This specifies the type used to represent JavaScript values within this context.
    /// The value type must implement JSValueImpl and have its Context type set to Self.
    type Value: JSValueImpl<Context = Self>;

    /// Creates a new JavaScript context
    fn new(runtime: &Self::Runtime) -> Self;

    /// Converts the context to its FFI representation
    fn as_raw(&self) -> &Self::RawContext;

    /// Returns a unique identifier for the context that can be used as a key in CTX_OPAQUE
    ///
    /// This identifier must be:
    /// - Unique per context instance
    /// - Stable for the lifetime of the context
    /// - Suitable for use as a HashMap key
    ///
    /// # Returns
    /// A usize value that uniquely identifies this context instance
    fn context_id(ctx: &Self::RawContext) -> usize;

    /// the implementation need to make sure it has the ownship, like as new method
    /// generally, it should increase referen count of FFI Context
    fn from_borrowed_raw(ctx: Self::RawContext) -> Self;

    /// Evaluate JavaScript code
    fn eval(&self, source: Source) -> Self::Value;

    /// Get global object
    fn global(&self) -> Self::Value;

    /// Register class for rust type
    fn register_class<JC>(&self) -> Self::Value
    where
        JC: JSClass<Self::Value>;

    /// Calls a JavaScript function with the specified `this` value and arguments.
    ///
    /// # Arguments
    ///
    /// * `function` - The JavaScript function to call
    /// * `this` - Optional `this` value to use when calling the function
    /// * `argv` - Slice of arguments to pass to the function
    ///
    /// # Returns
    ///
    /// Returns the result of the function call as a JavaScript value
    fn call(&self, function: &Self::Value, this: Self::Value, argv: &[Self::Value]) -> Self::Value;

    /// Creates a new JavaScript Promise and returns a tuple containing:
    /// - The Promise object
    /// - The resolve function to fulfill the promise
    /// - The reject function to reject the promise
    fn promise(&self) -> (Self::Value, Self::Value, Self::Value);

    /// Registers resolve/reject handlers on a promise at the engine level.
    ///
    /// Engines can override this to use native APIs. The shared `core` future
    /// machinery will later drain microtasks and apply any declared JS fallback,
    /// so implementations should focus on registration and avoid embedding shared
    /// scheduling policy here.
    fn register_promise_handlers(
        &self,
        _promise: &Self::Value,
        _on_fulfilled: &Self::Value,
        _on_rejected: &Self::Value,
    ) -> PromiseHandlerRegistration {
        PromiseHandlerRegistration::JavaScriptOnly
    }

    /// Compiles JavaScript source code into bytecode format
    ///
    /// # Arguments
    /// * `source` - The JavaScript source code to compile
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` - The compiled bytecode as bytes if compilation succeeds
    /// * `Err(RongJSError)` - If compilation fails with one of these errors:
    ///   - `HostError::new(E_COMPILE, ...)`: General compilation error
    ///   - `HostError::new(E_NOT_SUPPORTED, ...).with_data(err_data!({ feature: "bytecode" }))`:
    ///     Bytecode compilation not supported by runtime
    fn compile_to_bytecode(&self, source: Source) -> Result<Vec<u8>, RongJSError>;

    /// Executes previously compiled bytecode
    ///
    /// # Arguments
    /// * `bytes` - The bytecode bytes to execute
    ///
    /// # Returns
    /// The result of executing the bytecode as a JavaScript value
    fn run_bytecode(&self, bytes: &[u8]) -> Self::Value;
}

pub trait JSRawContext {
    type RawContext;
}

pub struct JSContext<C: JSContextImpl> {
    rc: Rc<JSContextInner<C>>,
}

struct JSContextInner<C: JSContextImpl> {
    inner: C,
    runtime: JSRuntime<C::Runtime>,
    rong: C::Value,
    services: ContextServiceContainer,
}

/// A trait for context-scoped services that can be attached to JSContext.
///
/// This is similar to JSRuntimeService but scoped to a single JSContext instance.
/// Implementors can use on_shutdown to release resources that should be cleaned
/// up when the owning JSContext is dropped.
pub trait JSContextService: 'static {
    /// Called when the JSContext that owns this service is being shutdown.
    fn on_shutdown(&self) {}
}

/// A container for context services with proper lifecycle management.
#[derive(Clone)]
struct ContextServiceContainer {
    services: Rc<RefCell<HashMap<TypeId, Box<dyn JSContextService>>>>,
}

struct ContextState<T: 'static>(T);

#[derive(Clone, Default)]
struct ContextTaskRegistry {
    inner: Rc<ContextTaskRegistryInner>,
}

#[derive(Default)]
struct ContextTaskRegistryInner {
    closed: std::cell::Cell<bool>,
    tasks: RefCell<Vec<tokio::task::JoinHandle<()>>>,
}

impl ContextTaskRegistry {
    fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + 'static,
    {
        if self.inner.closed.get() {
            return;
        }

        let task = tokio::task::spawn_local(future);
        let mut tasks = self.inner.tasks.borrow_mut();
        tasks.retain(|task| !task.is_finished());
        tasks.push(task);
    }

    fn abort_all(&self) -> Vec<tokio::task::JoinHandle<()>> {
        self.inner.closed.set(true);
        let tasks = self.inner.tasks.take();
        for task in &tasks {
            task.abort();
        }
        tasks
    }

    async fn shutdown(&self) {
        for task in self.abort_all() {
            let _ = task.await;
        }
    }
}

impl JSContextService for ContextTaskRegistry {
    fn on_shutdown(&self) {
        self.abort_all();
    }
}

impl<T: 'static> JSContextService for ContextState<T> {}

impl ContextServiceContainer {
    fn new() -> Self {
        Self {
            services: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    fn register<T: JSContextService>(&self, service: T) {
        let mut services = self.services.borrow_mut();
        services.insert(TypeId::of::<T>(), Box::new(service));
    }

    fn get<T: JSContextService>(&self) -> Option<&T> {
        // SAFETY: This is safe because:
        // 1. We only insert services through register<T>
        // 2. TypeId is unique for each type
        // 3. The service is never removed until container is shut down
        // 4. The RefCell ensures we don't have multiple mutable borrows
        unsafe {
            let services = self.services.borrow();
            services
                .get(&TypeId::of::<T>())
                .map(|svc| &*(svc.as_ref() as *const dyn JSContextService as *const T))
        }
    }

    fn register_state<T: 'static>(&self, value: T) {
        let mut services = self.services.borrow_mut();
        services.insert(TypeId::of::<T>(), Box::new(ContextState(value)));
    }

    fn get_state<T: 'static>(&self) -> Option<&T> {
        // SAFETY: We store `ContextState<T>` under `TypeId::of::<T>()` in `register_state`.
        unsafe {
            let services = self.services.borrow();
            services
                .get(&TypeId::of::<T>())
                .map(|svc| {
                    &*(svc.as_ref() as *const dyn JSContextService as *const ContextState<T>)
                })
                .map(|state| &state.0)
        }
    }

    fn shutdown(&self) {
        let mut services = self.services.borrow_mut();
        for (_, svc) in services.drain() {
            svc.on_shutdown();
        }
    }
}

impl<C: JSContextImpl> AsRef<C> for JSContext<C> {
    fn as_ref(&self) -> &C {
        &self.rc.inner
    }
}

impl<C: JSContextImpl> JSContext<C> {
    /// Creates a new JavaScript context.
    ///
    /// This function:
    /// 1. Creates a JSContext instance with proper internal structure
    /// 2. Stores the context address in an opaque data structure for FFI callbacks
    /// 3. Sets up weak references to prevent memory leaks
    ///
    /// The context can be safely shared between callbacks and async tasks.
    ///
    /// # Safety
    /// - The context must be dropped on the same thread it was created on
    /// - The runtime must outlive the context
    ///
    /// # Example
    /// ```rust,no_run
    /// use rong_core::prelude::*;
    ///
    /// fn demo<E: JSEngine + 'static>() {
    ///     let runtime = E::runtime();
    ///     let _ctx = runtime.context();
    /// }
    /// ```
    pub(crate) fn new(runtime: &JSRuntime<C::Runtime>) -> Self
    where
        C::Value: JSObjectOps,
    {
        let raw_ctx = C::new(&runtime.inner);
        let rong = C::Value::new_object(&raw_ctx);

        let inner = JSContextInner {
            inner: raw_ctx,
            runtime: runtime.clone(),
            rong,
            services: ContextServiceContainer::new(),
        };

        let ctx = JSContext { rc: Rc::new(inner) };
        let weak = Rc::downgrade(&ctx.rc);

        // save stale address to opaque
        let opaque = ContextOpaque::<C::Value>::new(weak);
        let raw_ctx = ctx.rc.inner.as_raw();
        let key = C::context_id(raw_ctx);
        CTX_OPAQUE
            .write()
            .unwrap()
            .insert(key, Box::into_raw(opaque) as usize);

        ctx
    }

    /// Returns the host namespace object injected into the JavaScript global scope.
    pub fn host_namespace(&self) -> JSObject<C::Value> {
        let value = JSValue::from_raw(self, self.rc.as_ref().rong.clone());
        value.into()
    }

    /// Creates a JSContext from an FFI context pointer.
    ///
    /// This is used in callback scenarios where the JS engine provides a context pointer.
    /// From the JS engine's perspective, contexts created from the mainline and from
    /// callbacks are equivalent since they operate within the same execution context.
    ///
    /// # Safety
    /// - The provided FFI context must be valid and properly aligned
    /// - The caller must ensure the context pointer remains valid for the duration of use
    /// - This should only be called with context pointers obtained from the JS engine
    /// - The returned reference must not outlive the original context
    ///
    /// # Example
    /// ```ignore
    /// // Pseudo-code: this is only usable with a real engine-provided raw context pointer.
    /// unsafe {
    ///     let ctx = JSContext::from_borrowed_raw_ptr(ffi_ctx);
    ///     // Use ctx for the duration of the callback
    /// }
    /// ```
    #[doc(hidden)]
    pub fn from_borrowed_raw_ptr(ptr: &C::RawContext) -> Self {
        let data = Self::_get_opaque(ptr);
        if data.is_null() {
            panic!("[JSContext] opaque is empty");
        } else {
            let ctx_inner = unsafe { &(*data).ctx_inner };
            if let Some(ctx) = ctx_inner.upgrade() {
                Self { rc: ctx }
            } else {
                panic!("[JSContext] context has been dropped");
            }
        }
    }

    /// Evaluate JavaScript code and return the result
    ///
    /// # Arguments
    /// * `source` - The JavaScript source code to evaluate
    ///
    /// # Returns
    /// * `Ok(T)` - The result of the evaluation if successful
    /// * `Err(RongJSError)` - If evaluation fails or throws an exception
    ///
    /// # Examples
    /// ```rust,no_run
    /// use rong_core::prelude::*;
    ///
    /// fn demo<E: JSEngine + 'static>() -> JSResult<()> {
    ///     let runtime = E::runtime();
    ///     let ctx = runtime.context();
    ///
    ///     let result: i32 = ctx.eval(Source::from_bytes("1 + 2"))?;
    ///     assert_eq!(result, 3);
    ///     Ok(())
    /// }
    /// ```
    pub fn eval<T>(&self, source: Source) -> JSResult<T>
    where
        C::Value: JSObjectOps,
        T: FromJSValue<C::Value>,
    {
        let result = match source.kind() {
            SourceKind::ByteCode(code) => self.rc.inner.run_bytecode(code),
            SourceKind::JavaScript(code) => self.rc.inner.eval(Source::from_bytes(code.clone())),
        };
        result.try_convert::<T>()
    }

    /// Get the global object of the JavaScript context
    ///
    /// # Returns
    /// A JSObject representing the global object
    ///
    /// # Examples
    /// ```rust,no_run
    /// use rong_core::prelude::*;
    ///
    /// fn demo<E: JSEngine + 'static>() -> JSResult<()> {
    ///     let runtime = E::runtime();
    ///     let ctx = runtime.context();
    ///
    ///     let global = ctx.global();
    ///     global.set("myVar", 42)?;
    ///
    ///     let result: i32 = global.get("myVar")?;
    ///     assert_eq!(result, 42);
    ///     Ok(())
    /// }
    /// ```
    pub fn global(&self) -> JSObject<C::Value>
    where
        C::Value: JSTypeOf,
    {
        let raw = self.rc.inner.global();
        JSObject::from_js_value(self, JSValue::from_raw(self, raw)).unwrap()
    }

    fn register_class_with_visibility<JC>(&self, expose_on_global: bool) -> JSResult<()>
    where
        JC: JSClass<C::Value>,
        C::Value: JSObjectOps,
    {
        let registry = self
            .get_class_registry()
            .ok_or_else(|| HostError::new(crate::error::E_INTERNAL, "No Class registry!"))?;

        if registry.borrow().contains_key(&TypeId::of::<JC>()) {
            return Ok(());
        }

        let value = self.rc.inner.register_class::<JC>();

        let constructor = JSValue::from_raw(self, value.clone());
        JC::class_setup(&ClassSetup::new(constructor.clone().into(), self)?)?;
        if expose_on_global {
            let obj = self.global();
            obj.set(JC::NAME, constructor)?;
        }

        registry.borrow_mut().insert(TypeId::of::<JC>(), value);

        Ok(())
    }

    /// Register a JavaScript class for a Rust type.
    ///
    /// This function registers a JavaScript class constructor in the global object
    /// and stores it in the context's class registry. The class can then be used
    /// to create instances in JavaScript.
    ///
    /// ```ignore
    /// // Pseudo-code (requires a JS engine + a type that implements JSClass)
    /// context.register_class::<MyClass>();
    /// ```
    pub fn register_class<JC>(&self) -> JSResult<()>
    where
        JC: JSClass<C::Value>,
        C::Value: JSObjectOps,
    {
        self.register_class_with_visibility::<JC>(true)
    }

    /// Register a JavaScript class without exposing its constructor on the global object.
    ///
    /// This is for Rust-owned interop classes that must exist in the class registry
    /// but should not be directly constructed by JavaScript code.
    pub fn register_hidden_class<JC>(&self) -> JSResult<()>
    where
        JC: JSClass<C::Value>,
        C::Value: JSObjectOps,
    {
        self.register_class_with_visibility::<JC>(false)
    }

    /// Get class registry from context
    pub(crate) fn get_class_registry(&self) -> Option<&RefCell<HashMap<TypeId, C::Value>>> {
        let data = self.get_opaque();
        if data.is_null() {
            None
        } else {
            unsafe { Some(&(*data).registry) }
        }
    }

    pub(crate) fn capture_thrown(&self, value: JSValue<C::Value>) -> ThrownValueHandle {
        let data = self.get_opaque();
        if data.is_null() {
            panic!("[JSContext] opaque is empty");
        }

        let context_id = C::context_id(self.as_ref().as_raw());

        #[cfg(debug_assertions)]
        tracing::debug!(target: "rong", context_id, "capturing thrown JS value");

        let mut store = unsafe { (*data).thrown.try_borrow_mut() }
            .expect("[JSContext] Fatal: ThrownValueStore already borrowed. Recursive error handling detected.");

        store.insert(context_id, value.into_value())
    }

    pub(crate) fn resolve_thrown(&self, handle: ThrownValueHandle) -> Option<JSValue<C::Value>> {
        let data = self.get_opaque();
        if data.is_null() {
            return None;
        }

        let context_id = C::context_id(self.as_ref().as_raw());
        let store = unsafe { (*data).thrown.try_borrow().ok()? };
        store
            .get(context_id, handle)
            .map(|v| JSValue::from_raw(self, v))
    }

    pub(crate) fn take_thrown(&self, handle: ThrownValueHandle) -> Option<JSValue<C::Value>> {
        let data = self.get_opaque();
        if data.is_null() {
            return None;
        }

        let context_id = C::context_id(self.as_ref().as_raw());
        let mut store = unsafe { (*data).thrown.try_borrow_mut().ok()? };
        store
            .take(context_id, handle)
            .map(|v| JSValue::from_raw(self, v))
    }

    pub fn runtime(&self) -> &JSRuntime<C::Runtime> {
        &self.rc.runtime
    }

    /// Register a context-scoped service of a specific type.
    ///
    /// The service is stored by its concrete type. Only one instance of each
    /// service type can be registered for a given JSContext.
    pub fn set_service<T: JSContextService>(&self, service: T) {
        self.rc.services.register::<T>(service);
    }

    /// Get a previously registered context-scoped service by type.
    ///
    /// Returns None if no service of the requested type has been registered.
    pub fn get_service<T: JSContextService>(&self) -> Option<&T> {
        self.rc.services.get::<T>()
    }

    /// Store context-scoped state without implementing `JSContextService`.
    ///
    /// This is intended for simple values that don't need cleanup when the context is dropped.
    /// If you need cleanup, implement `JSContextService` and use `set_service` so `on_shutdown`
    /// can run during context teardown.
    pub fn set_state<T: 'static>(&self, value: T) {
        self.rc.services.register_state(value);
    }

    /// Get context-scoped state previously stored via `set_state`.
    pub fn get_state<T: 'static>(&self) -> Option<&T> {
        self.rc.services.get_state::<T>()
    }

    fn task_registry(&self) -> ContextTaskRegistry {
        if let Some(registry) = self.get_service::<ContextTaskRegistry>() {
            return registry.clone();
        }

        let registry = ContextTaskRegistry::default();
        self.set_service(registry.clone());
        registry
    }

    /// Spawn work that must not outlive this JavaScript context.
    pub fn spawn_task<F>(&self, future: F)
    where
        F: Future<Output = ()> + 'static,
    {
        self.task_registry().spawn(future);
    }

    /// Cancel and drain all context-owned async work before releasing the context.
    pub async fn shutdown_tasks(&self) {
        self.task_registry().shutdown().await;
    }

    /// Compile JavaScript source code to bytecode
    ///
    /// # Arguments
    /// * `code` - The JavaScript source code to compile. Accepts:
    ///   - &str: JavaScript source code as string
    ///   - &[u8]: JavaScript source code as bytes
    ///   - String: Owned JavaScript source code
    ///   - `Vec<u8>`: Owned JavaScript source code as bytes
    ///
    /// # Returns
    /// * `Ok(Source)` - Compiled bytecode wrapped in a Source, ready to be evaluated
    /// * `Err(RongJSError)` - If compilation fails
    ///
    /// # Example
    /// ```rust,no_run
    /// use rong_core::prelude::*;
    ///
    /// fn demo<E: JSEngine + 'static>() -> JSResult<()> {
    ///     let runtime = E::runtime();
    ///     let ctx = runtime.context();
    ///
    ///     // From string literal
    ///     let _bytecode = ctx.compile_to_bytecode("function add(a, b) { return a + b; }")?;
    ///
    ///     // From bytes
    ///     let _bytecode = ctx.compile_to_bytecode(b"let x = 1;")?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn compile_to_bytecode<T: AsRef<[u8]>>(&self, code: T) -> JSResult<Source> {
        self.rc
            .inner
            .compile_to_bytecode(Source::from_bytes(code.as_ref()))
            .map(Source::from_bytecode)
    }

    /// Evaluate JavaScript code and handle both Promise and immediate results
    ///
    /// This function evaluates the provided JavaScript source code and:
    /// 1. If the result is a Promise, waits for it to resolve and returns the resolved value
    /// 2. If the result is not a Promise, returns it immediately
    ///
    /// # Arguments
    /// * `source` - The JavaScript source code to evaluate
    ///
    /// # Returns
    /// * `Ok(T)` - The result of the evaluation or resolved Promise value
    /// * `Err(RongJSError)` - If evaluation fails, throws an exception, or Promise rejects
    pub async fn eval_async<T>(&self, source: Source) -> JSResult<T>
    where
        C::Value: JSTypeOf + JSObjectOps + 'static,
        T: FromJSValue<C::Value> + 'static,
    {
        let result = match source.kind() {
            SourceKind::ByteCode(code) => self.rc.inner.run_bytecode(code),
            SourceKind::JavaScript(code) => self.rc.inner.eval(Source::from_bytes(code.clone())),
        };

        if result.is_promise() {
            let promise = Promise::from_js_value(self, JSValue::from_raw(self, result))?;
            promise.into_future::<T>().await
        } else {
            result.try_convert::<T>()
        }
    }

    fn get_opaque(&self) -> *mut ContextOpaque<C::Value> {
        let key = self.rc.inner.as_raw();
        Self::_get_opaque(key)
    }

    fn _get_opaque(raw_ctx: &C::RawContext) -> *mut ContextOpaque<C::Value> {
        let key = C::context_id(raw_ctx);
        if let Some(opaque_ptr) = CTX_OPAQUE.read().unwrap().get(&key) {
            *opaque_ptr as *mut ContextOpaque<C::Value>
        } else {
            std::ptr::null_mut()
        }
    }
}

/// Container to hold the context-specific data for a JSContext.
///
/// # Fields
/// - `registry`: A pointer to a RefCell containing a HashMap that maps TypeId to type that implements JSValueImpl
/// - `ctx_inner`: Weak reference to the JSContextInner used to build JSContext from callback case
struct ContextOpaque<V: JSValueImpl> {
    registry: RefCell<HashMap<TypeId, V>>,
    ctx_inner: Weak<JSContextInner<V::Context>>,
    thrown: RefCell<ThrownValueStore<V>>,
}

impl<V: JSValueImpl> ContextOpaque<V> {
    fn new(ctx_inner: Weak<JSContextInner<V::Context>>) -> Box<Self> {
        Box::new(Self {
            registry: RefCell::new(HashMap::new()),
            ctx_inner,
            thrown: RefCell::new(ThrownValueStore::new()),
        })
    }
}

// Global HashMap to store ContextOpaque<V>
// Like JavaScriptCore engine, it does not provide API to save opaque on JS Context,
// that's why we introduce general solution CTX_OPAQUE
static CTX_OPAQUE: LazyLock<RwLock<HashMap<usize, usize>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

impl<C: JSContextImpl> Drop for JSContext<C> {
    fn drop(&mut self) {
        if Rc::strong_count(&self.rc) == 1 {
            // First, shutdown all context-scoped services.
            self.rc.services.shutdown();

            let raw_ctx = self.rc.inner.as_raw();
            let key = C::context_id(raw_ctx);
            let data = CTX_OPAQUE
                .write()
                .unwrap()
                .remove(&key)
                .map(|ptr| ptr as *mut ContextOpaque<C::Value>)
                .unwrap_or(std::ptr::null_mut());

            if !data.is_null() {
                unsafe {
                    // cleanup class registry
                    let registry = &(*data).registry;
                    registry.borrow_mut().clear();

                    // cleanup ContextOpaque
                    let _ = Box::from_raw(data);
                }
            }
        }
    }
}

impl<C: JSContextImpl> Clone for JSContext<C> {
    fn clone(&self) -> Self {
        Self {
            rc: Rc::clone(&self.rc),
        }
    }
}

impl<C: JSContextImpl> std::fmt::Debug for JSContext<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "JSContext {{ address: {:p}, ref_count: {} }}",
            self as *const _,
            Rc::strong_count(&self.rc)
        )
    }
}
