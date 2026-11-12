use crate::function::{Constructor, FromParams, IntoJSCallable, ParamsAccessor, RustFunc};
use crate::runtime::NativeCallFrame;
use crate::{
    FromJSValue, HostError, JSArrayOps, JSContext, JSContextImpl, JSErrorFactory,
    JSExceptionThrower, JSFunc, JSObject, JSObjectOps, JSResult, JSTypeOf, JSValue, JSValueImpl,
    PropertyDescriptor, PropertyKey, RongJSError,
};

use std::any::TypeId;
use std::cell::{Ref, RefCell, RefMut};
use std::ops::Deref;

/// JSClass trait for rust type that supports TypeId
/// `TypeId` is currently only available for types which ascribe to `'static`,
pub trait JSClass<V: JSValueImpl>: Sized + 'static {
    // the name of class constructor
    const NAME: &'static str;

    /// Whether instances of this class can be called as functions.
    /// Override to `true` for callable classes (e.g. RustFunc).
    const CALLABLE: bool = false;

    /// Returns the data constructor function for this class
    fn data_constructor() -> Constructor<V>;

    /// Returns the implicit constructor function for this class
    ///
    /// This is the function called when the class is invoked directly without
    /// 'new' (e.g. `MyClass()`).
    ///
    /// Note: This is distinct from `data_constructor()`, which provides the internal
    /// constructor logic.
    fn call_without_new() -> Constructor<V> {
        Constructor::new(|| ())
    }

    /// Configures the class prototype and constructor with methods and properties
    fn class_setup(class: &ClassSetup<V>) -> JSResult<()>;

    /// Marks JavaScript values held by this object to prevent garbage collection.
    ///
    /// Used primarily by the QuickJS engine. Some engines like JavaScriptCore
    /// handle reference tracking automatically.
    ///
    /// IMPORTANT: Do NOT use clone() inside this method as it may break reference
    /// counting in the garbage collector.
    ///
    /// Default implementation does nothing. Override this when you need to prevent
    /// JS objects referenced by Rust from being garbage collected.
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue<V>),
    {
        // Default implementation does nothing
    }
}

#[doc(hidden)]
pub trait JSClassExt<V: JSValueImpl>: JSClass<V> {
    /// Shared constructor body that builds or returns the instance value.
    ///
    /// Engines with different native constructor ABIs can reuse this and keep
    /// their post-construction wiring local. For example, QuickJS/JSC call
    /// `constructor()`, while ArkJS calls `construct_value()` directly and
    /// finishes prototype synchronization in its own callback.
    fn construct_value(ctx: &V::Context, this: V, args: Vec<V>) -> Result<V, V>
    where
        V::Context: JSErrorFactory + JSExceptionThrower,
        V: JSObjectOps + JSArrayOps + JSTypeOf,
    {
        let _frame = NativeCallFrame::enter();
        let ctx = &JSContext::from_borrowed_raw_ptr(ctx.as_raw());
        let mut accessor = ParamsAccessor::new(ctx, this.clone(), args);

        if this.is_undefined() {
            match Self::call_without_new().0.call(&mut accessor) {
                Ok(v) => return Ok(v),
                Err(e) => return Err(e.throw_js_exception(ctx)),
            }
        }

        let instance = match Self::data_constructor().0.call(&mut accessor) {
            Ok(v) => {
                if v.is_exception() {
                    return Err(v);
                }
                v
            }
            Err(e) => return Err(e.throw_js_exception(ctx)),
        };

        match JSObject::from_js_value(ctx, JSValue::from_raw(ctx, instance)) {
            Ok(obj) => Ok(obj.into_value()),
            Err(e) => Err(e.throw_js_exception(ctx)),
        }
    }

    /// Default constructor adapter for engines whose constructor callback
    /// receives the JS constructor object as `this`.
    fn constructor(ctx: &V::Context, this: V, args: Vec<V>) -> V
    where
        V::Context: JSErrorFactory + JSExceptionThrower,
        V: JSObjectOps + JSArrayOps + JSTypeOf,
    {
        let value = match Self::construct_value(ctx, this.clone(), args) {
            Ok(value) => value,
            Err(value) => return value,
        };

        if this.is_undefined() {
            return value;
        }

        let ctx = &JSContext::from_borrowed_raw_ptr(ctx.as_raw());
        let instance = match JSObject::from_js_value(ctx, JSValue::from_raw(ctx, value)) {
            Ok(obj) => obj,
            Err(e) => return e.throw_js_exception(ctx),
        };

        // why not using constructor in class registry?
        // because for extended class, it's NEW Constructor
        let proto = match JSObject::from_js_value(ctx, JSValue::from_raw(ctx, this)).and_then(
            |constructor| {
                // println!(
                //     "Constructor Name is {}",
                //     constructor.get::<_, String>("name").unwrap()
                // );
                constructor.get("prototype")
            },
        ) {
            Ok(proto) => proto,
            Err(e) => return e.throw_js_exception(ctx),
        };

        instance.prototype(proto);
        instance.into_value()
    }

    /// Free resources of a class instance by finalizer
    fn free(value: V)
    where
        V: JSObjectOps,
    {
        Class::free::<Self>(value);
    }

    /// call object as function
    fn call(ctx: &V::Context, function: V, this: V, args: Vec<V>) -> V
    where
        V: JSObjectOps + JSArrayOps + 'static,
        V::Context: JSErrorFactory + JSExceptionThrower,
    {
        let _frame = NativeCallFrame::enter();
        let ctx = &JSContext::from_borrowed_raw_ptr(ctx.as_raw());
        let mut accessor = ParamsAccessor::new(ctx, this, args);

        let obj = match JSObject::from_js_value(ctx, JSValue::from_raw(ctx, function)) {
            Ok(obj) => obj,
            Err(e) => return e.throw_js_exception(ctx),
        };

        let mut func = match obj.borrow_mut::<RustFunc<V>>() {
            Ok(f) => f,
            Err(_) => {
                return RongJSError::from(HostError::not_function()).throw_js_exception(ctx);
            }
        };

        // ignore checking whether v is exception or error, sicne no one use it again here
        match func.call(&mut accessor) {
            Ok(v) => v,
            Err(e) => e.throw_js_exception(ctx),
        }
    }
}

// Blanket implementation
impl<T, V: JSValueImpl> JSClassExt<V> for T where T: JSClass<V> {}

/// Represents a JavaScript class constructor
///
/// This struct encapsulates a JavaScript object that serves as a class constructor.
/// It is used to create class instances and manage class lifecycle.
pub struct Class<V: JSValueImpl>(pub(crate) JSObject<V>);

impl<V> Deref for Class<V>
where
    V: JSValueImpl,
{
    type Target = JSObject<V>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<V> Class<V>
where
    V: JSValueImpl + JSObjectOps,
{
    /// Create a new instance of the class
    pub fn instance<JC: JSClass<V>>(self, value: JC) -> JSObject<V> {
        let context = self.0.context();
        let ptr = Box::into_raw(Box::new(RefCell::new(value)));

        let instance = V::make_instance(context.as_ref(), self.0.clone().into_value(), ptr as _);

        // instance object inherits Constructor's prototype
        let _ = self
            .0
            .get::<_, JSObject<V>>("prototype")
            .map(|proto| instance.set_prototype(proto.into_value()));
        JSObject::from_raw(&context, instance)
    }

    /// Check if the object is an instance of the specified class
    pub fn instance_of<JC: JSClass<V>>(object: &JSObject<V>) -> bool {
        let context = object.context();
        if let Ok(class) = Class::<V>::lookup::<JC>(&context) {
            object.as_value().instance_of(class.0.into_value())
        } else {
            false
        }
    }

    /// Free resources of a class instance
    pub(crate) fn free<JC: JSClass<V>>(instance: V) {
        let value = instance.clone();
        let ptr = value.get_opaque() as *mut RefCell<JC>;
        if !ptr.is_null() {
            // SAFETY: ptr was created by Box::into_raw in instance()
            unsafe {
                let _ = Box::from_raw(ptr);
            };
        }
    }

    /// Returns the registered constructor for a class type.
    pub fn lookup<JC: JSClass<V>>(context: &JSContext<V::Context>) -> JSResult<Self> {
        let constructor = context
            .get_class_registry()
            .and_then(|registry| registry.borrow().get(&TypeId::of::<JC>()).cloned())
            .ok_or_else(|| {
                HostError::new(
                    crate::error::E_INVALID_STATE,
                    format!("JS Class {} is not registered", std::any::type_name::<JC>()),
                )
            })?;

        Ok(Self(JSObject::from_raw(context, constructor)))
    }

    pub fn prototype<JC: JSClass<V>>(context: &JSContext<V::Context>) -> JSResult<JSObject<V>> {
        let class = Class::lookup::<JC>(context)?;
        class.0.get::<_, JSObject<V>>("prototype")
    }

    /// Construct a Class constructor from a JSObject if it is an instance of the specified class
    ///
    /// # Arguments
    /// * `obj` - The JSObject to check and convert
    ///
    /// # Returns
    /// - `Some(Class)` if the object is an instance of the specified class
    /// - `None` if the object is not an instance of the specified class
    pub fn from_object<JC: JSClass<V>>(obj: &JSObject<V>) -> Option<Self> {
        if Self::instance_of::<JC>(obj) {
            Some(Self(obj.clone()))
        } else {
            None
        }
    }
}

impl<V> JSObject<V>
where
    V: JSValueImpl + JSObjectOps,
{
    /// Borrow the underlying data from an instance
    pub fn borrow<T>(&self) -> JSResult<Ref<'_, T>>
    where
        T: JSClass<V>,
    {
        if !Class::instance_of::<T>(self) {
            return Err(HostError::new(
                crate::error::E_TYPE,
                format!("Not instance of {}", std::any::type_name::<T>()),
            )
            .with_name("TypeError")
            .into());
        }

        let ptr = self.as_value().get_opaque() as *mut RefCell<T>;
        if ptr.is_null() {
            Err(HostError::new(
                crate::error::E_INTERNAL,
                format!("Failed to borrow for type {}", std::any::type_name::<T>()),
            )
            .into())
        } else {
            // SAFETY: ptr was created by Box::into_raw in instance()
            unsafe { &*ptr }.try_borrow().map_err(|_| {
                HostError::new(
                    crate::error::E_INTERNAL,
                    format!("Failed to borrow for type {}", std::any::type_name::<T>()),
                )
                .into()
            })
        }
    }

    /// Mutably borrow the underlying data from an instance
    pub fn borrow_mut<T>(&self) -> JSResult<RefMut<'_, T>>
    where
        T: JSClass<V>,
    {
        if !Class::instance_of::<T>(self) {
            return Err(HostError::new(
                crate::error::E_TYPE,
                format!("Not instance of {}", std::any::type_name::<T>()),
            )
            .with_name("TypeError")
            .into());
        }

        let ptr = self.as_value().get_opaque() as *mut RefCell<T>;
        if ptr.is_null() {
            Err(HostError::new(
                crate::error::E_INTERNAL,
                format!("Failed to borrow for type {}", std::any::type_name::<T>()),
            )
            .into())
        } else {
            // SAFETY: ptr was created by Box::into_raw in instance()
            unsafe { &*ptr }.try_borrow_mut().map_err(|_| {
                HostError::new(
                    crate::error::E_INTERNAL,
                    format!("Failed to borrow for type {}", std::any::type_name::<T>()),
                )
                .into()
            })
        }
    }

    pub fn prototype(&self, proto: JSObject<V>) -> bool {
        self.as_value().set_prototype(proto.into_value())
    }
}

pub struct ClassSetup<'a, V: JSValueImpl> {
    constructor: JSObject<V>,
    prototype: JSObject<V>,
    context: &'a JSContext<V::Context>,
}

impl<'a, V> ClassSetup<'a, V>
where
    V: JSObjectOps,
{
    pub(crate) fn new(
        constructor: JSObject<V>,
        context: &'a JSContext<V::Context>,
    ) -> JSResult<Self> {
        let constructor = Class(constructor);
        let prototype = constructor.0.get::<_, JSObject<V>>("prototype")?;
        Ok(Self {
            constructor: constructor.0,
            prototype,
            context,
        })
    }

    /// Access the underlying JS context
    pub fn context(&self) -> &JSContext<V::Context> {
        self.context
    }

    /// Access the prototype object of this class
    pub fn prototype_object(&self) -> JSObject<V> {
        self.prototype.clone()
    }

    pub fn method<F, P, K: 'static>(&self, name: &str, f: F) -> JSResult<()>
    where
        F: IntoJSCallable<V, P, K> + 'static,
        P: FromParams<V>,
        V: JSObjectOps + 'static,
    {
        let func = JSFunc::new(self.context, f)?;
        let func = func.name(name)?;
        self.prototype.set(name, func)?;
        Ok(())
    }

    pub fn static_method<F, P, K: 'static>(&self, name: &str, f: F) -> JSResult<()>
    where
        F: IntoJSCallable<V, P, K> + 'static,
        P: FromParams<V>,
        V: JSObjectOps + 'static,
    {
        let func = JSFunc::new(self.context, f)?;
        let func = func.name(name)?;
        self.constructor.set(name, func)?;
        Ok(())
    }

    pub fn property<Key>(&self, k: Key, descriptor: PropertyDescriptor<V>) -> JSResult<()>
    where
        Key: for<'b> Into<PropertyKey<'b, V>>,
    {
        self.prototype.define_property(k, descriptor)?;
        Ok(())
    }

    pub fn static_property<Key>(&self, k: Key, descriptor: PropertyDescriptor<V>) -> JSResult<()>
    where
        Key: for<'b> Into<PropertyKey<'b, V>>,
    {
        self.constructor.define_property(k, descriptor)?;
        Ok(())
    }

    pub fn new_func<F, P, K: 'static>(&self, f: F) -> JSResult<JSFunc<V>>
    where
        F: IntoJSCallable<V, P, K> + 'static,
        P: FromParams<V>,
        V: JSObjectOps + 'static,
    {
        JSFunc::new(self.context, f)
    }
}
