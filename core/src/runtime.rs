use crate::function::RustFunc;
use crate::value::JSBytesData;
use crate::{
    JSArrayOps, JSContext, JSContextImpl, JSErrorFactory, JSExceptionThrower, JSObjectOps,
    JSResult, JSTypeOf, JSValueConversion, JSValueImpl,
};
use std::any::TypeId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const RONG_VERSION: &str = env!("CARGO_PKG_VERSION");
const RONG_REVISION: Option<&str> = option_env!("RONG_GIT_REVISION");

pub trait JSRuntimeImpl {
    /// The raw runtime handle type associated with this runtime.
    type RawRuntime;

    /// The JavaScript context type associated with this runtime
    type Context: JSContextImpl;

    /// Creates JavaScript runtime.
    fn new() -> Self;

    /// Converts the runtime to its raw underlying representation.
    ///
    /// # Key Notes
    /// - This provides access to the low-level, engine-specific runtime handle.
    /// - Useful for advanced use cases requiring direct interaction with the engine.
    /// - The returned value is a copy of the raw runtime handle.
    fn to_raw(&self) -> Self::RawRuntime;

    /// Runs all pending jobs in the JavaScript runtime.
    /// This includes executing any queued promise callbacks, microtasks, and other pending operations.
    ///
    /// # Key Notes
    /// - return -1 means this JSRuntime does not need to call this API
    fn run_pending_jobs(&self) -> i32 {
        -1
    }

    /// Runs garbage collection on the JavaScript runtime.
    ///
    /// # Key Notes
    /// - This method triggers a garbage collection cycle to reclaim unused memory.
    /// - The exact behavior depends on the underlying JavaScript engine implementation.
    /// - Use this judiciously as it may impact performance.
    fn run_gc(&self);
}

pub struct JSRuntime<R: JSRuntimeImpl> {
    pub(crate) inner: Rc<R>,
    services: ServiceContainer,
    pub(crate) engine: &'static str,
}

impl<R: JSRuntimeImpl> Clone for JSRuntime<R> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            services: self.services.clone(),
            engine: self.engine,
        }
    }
}

impl<R: JSRuntimeImpl + 'static> JSRuntime<R> {
    /// Creates a new JavaScript context instance associated with this runtime.
    ///
    /// # Key Notes
    /// - Automatically registers the Rust function class for interop capabilities.
    /// - Contexts are isolated execution environments within the same runtime.
    ///
    /// # Example
    /// ```rust,no_run
    /// use rong_core::prelude::*;
    ///
    /// fn demo<E: JSEngine + 'static>() {
    ///     let runtime = E::runtime();
    ///     let _context = runtime.context();
    /// }
    /// ```
    pub fn context(&self) -> JSContext<R::Context>
    where
        R::Context: JSContextImpl<Runtime = R>,
        <R::Context as JSContextImpl>::Value:
            JSObjectOps + JSTypeOf + JSValueConversion + JSArrayOps + 'static,
        R::Context: JSErrorFactory + JSExceptionThrower,
    {
        let ctx = JSContext::<R::Context>::new(self);
        ctx.register_builtin_class()
            .expect("Failed to register builtin class");

        ctx.global()
            .set("Rong", ctx.host_namespace())
            .expect("Failed to add Rong object");
        ctx.host_namespace()
            .set("version", RONG_VERSION)
            .expect("Failed to add Rong.version");
        ctx.host_namespace()
            .set("revision", RONG_REVISION.unwrap_or("unknown"))
            .expect("Failed to add Rong.revision");

        ctx
    }

    /// # Warning
    /// testing purposes only and don't use it in production code.
    #[doc(hidden)]
    pub fn run_pending_jobs(&self) -> i32 {
        self.inner.run_pending_jobs()
    }

    /// Runs garbage collection on the JavaScript runtime.
    ///
    /// # Key Notes
    /// - This method triggers a garbage collection cycle to reclaim unused memory.
    /// - The exact behavior depends on the underlying JavaScript engine implementation.
    /// - Use this judiciously as it may impact performance.
    pub fn run_gc(&self) {
        self.inner.run_gc();
    }

    /// Get or initialize a runtime service
    /// If the service is not registered, it will be initialized with default values
    pub fn get_or_init_service<T: JSRuntimeService + Default>(&self) -> &T {
        if let Some(service) = self.services.get::<T>() {
            service
        } else {
            let service = T::default();
            self.services.register(service);
            self.services
                .get::<T>()
                .expect("Service should be registered")
        }
    }
}

impl<R: JSRuntimeImpl> Drop for JSRuntime<R> {
    fn drop(&mut self) {
        if Rc::strong_count(&self.inner) == 1 {
            // Drain services while the engine is still alive. ServiceContainer's
            // subsequent drop then observes an empty map and cannot run hooks twice.
            self.services.shutdown();
        }
    }
}

impl<C: JSContextImpl> JSContext<C> {
    /// used to create object instance as function
    pub(crate) fn register_builtin_class(&self) -> JSResult<()>
    where
        C::Value: JSObjectOps + JSTypeOf + JSValueConversion + JSArrayOps + 'static,
        C: JSErrorFactory + JSExceptionThrower,
    {
        self.register_hidden_class::<RustFunc<C::Value>>()?;
        self.register_hidden_class::<JSBytesData>()?;
        Ok(())
    }
}

pub trait JSEngine: Sized {
    type Value: JSValueImpl<Context = Self::Context>
        + JSObjectOps
        + JSTypeOf
        + JSValueConversion
        + JSArrayOps
        + 'static;
    type Context: JSContextImpl<Value = Self::Value, Runtime = Self::Runtime>
        + JSErrorFactory
        + JSExceptionThrower;
    type Runtime: JSRuntimeImpl<Context = Self::Context> + 'static;

    /// JS engine name
    fn name() -> &'static str;

    /// JS engine version
    fn version() -> String;

    /// Creates a new JavaScript runtime instance.
    ///
    /// # Key Notes
    /// - One thread have only one runtime instance.
    /// - This ensures proper isolation and thread-safety in JavaScript execution.
    fn runtime() -> JSRuntime<Self::Runtime> {
        let runtime = Rc::new(Self::Runtime::new());
        JSRuntime {
            inner: runtime,
            services: ServiceContainer::new(),
            engine: Self::name(),
        }
    }
}

/// A trait for runtime services that can be attached to JSRuntime
pub trait JSRuntimeService: 'static {
    /// Called when the service is being initialized
    fn on_init(&self) {}

    /// Called when the service is being shutdown
    fn on_shutdown(&self) {}
}

/// A container for runtime services with proper lifecycle management
#[derive(Clone)]
struct ServiceContainer {
    services: Rc<RefCell<HashMap<TypeId, Box<dyn JSRuntimeService>>>>,
}

impl ServiceContainer {
    fn new() -> Self {
        Self {
            services: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    fn register<T: JSRuntimeService>(&self, extension: T) {
        let mut services = self.services.borrow_mut();
        extension.on_init();
        services.insert(TypeId::of::<T>(), Box::new(extension));
    }

    fn get<T: JSRuntimeService>(&self) -> Option<&T> {
        // SAFETY: This is safe because:
        // 1. We only insert services through register<T>
        // 2. TypeId is unique for each type
        // 3. The extension is never removed until container is dropped
        // 4. The RefCell ensures we don't have multiple mutable borrows
        unsafe {
            let services = self.services.borrow();
            services
                .get(&TypeId::of::<T>())
                .map(|ext| &*(ext.as_ref() as *const dyn JSRuntimeService as *const T))
        }
    }

    fn shutdown(&self) {
        let services = self
            .services
            .borrow_mut()
            .drain()
            .map(|(_, service)| service)
            .collect::<Vec<_>>();
        for ext in services {
            ext.on_shutdown();
        }
    }
}

impl Drop for ServiceContainer {
    fn drop(&mut self) {
        // Only shutdown if we're the last reference
        if Rc::strong_count(&self.services) == 1 {
            self.shutdown();
        }
    }
}
