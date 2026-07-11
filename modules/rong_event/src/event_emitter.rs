use rong::{function::*, *};
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::rc::Rc;
use std::sync::Mutex;

/// Represents a key that can be used to identify an event type.
///
/// This enum supports both string and symbol event types, allowing for
/// flexible event identification similar to Node.js's EventEmitter.
///
/// # Variants
/// - `String(String)`: A string-based event type identifier
/// - `Symbol(JSSymbol)`: A symbol-based event type identifier
#[derive(Clone)]
pub enum EventKey {
    String(String),
    Symbol(JSSymbol),
}

impl From<String> for EventKey {
    fn from(s: String) -> Self {
        EventKey::String(s)
    }
}

impl From<&str> for EventKey {
    fn from(s: &str) -> Self {
        EventKey::String(s.to_string())
    }
}

impl From<JSSymbol> for EventKey {
    fn from(s: JSSymbol) -> Self {
        EventKey::Symbol(s)
    }
}

impl PartialEq for EventKey {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::String(l), Self::String(r)) => l == r,
            (Self::Symbol(l), Self::Symbol(r)) => l == r,
            _ => false,
        }
    }
}

impl Eq for EventKey {}

impl Hash for EventKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
    }
}

impl FromJSValue<JSEngineValue> for EventKey {
    fn from_js_value(ctx: &JSContext, value: JSValue) -> JSResult<Self> {
        if let Ok(key) = String::from_js_value(ctx, value.clone()) {
            return Ok(EventKey::String(key));
        }
        if let Ok(symbol) = JSSymbol::from_js_value(ctx, value) {
            return Ok(EventKey::Symbol(symbol));
        }
        Err(HostError::new(
            rong::error::E_INVALID_ARG,
            "EventKey must be Symbol or String!",
        )
        .with_name("TypeError")
        .into())
    }
}

impl IntoJSValue<JSEngineValue> for EventKey {
    fn into_js_value(self, ctx: &JSContext) -> JSValue {
        match self {
            EventKey::String(k) => JSValue::from_rust(ctx, k),
            EventKey::Symbol(k) => k.into_js_value(ctx),
        }
    }
}

// blanket implementing to make EventKey can be as extracted from JSFunc
impl rong::function::JSParameterType for EventKey {}

/// Represents an event listener
#[derive(Clone, PartialEq)]
pub struct EventListener {
    listener: JSFunc,
    // A boolean indicating the listener should be invoked at most once
    once: bool,
}

/// Trait for objects that can emit events
///
/// When implementing this trait, users can use `EmitterExt` to add
/// Node.js style event emitter prototype methods to their class.
///
/// # Example
/// ```ignore
/// use rong::js_class;
/// use event::EventEmitter;
///
/// #[js_class]
/// struct MyEmitter {
///     events: EventEmitter,
/// }
///
/// impl Emitter for MyEmitter {
///     fn get_event_emitter(&self) -> EventEmitter {
///         &self.events
///     }
/// }
///
/// // Then use EmitterExt to add prototype methods
/// MyEmitter::add_node_event_target_prototype(ctx)?;
/// ```
pub trait Emitter
where
    Self: JSClass<JSEngineValue>,
{
    /// Get the internal events container
    fn get_event_emitter(&self) -> EventEmitter;

    /// Callback triggered when an event listener is added or removed
    ///
    /// This can be overridden to implement custom behavior when
    /// listeners change. The default implementation does nothing.
    fn on_event_changed(&self, _key: EventKey, _added: bool) -> JSResult<()> {
        Ok(())
    }
}

/// A trait for converting Rust errors into JavaScript event emissions.
///
/// This trait provides a convenient way to handle Rust errors by converting them into
/// JavaScript 'error' events, following the Node.js error handling pattern.
///
/// When an error occurs, it will:
/// 1. Try to emit an 'error' event with the error message
/// 2. If there are error listeners, the error will be handled by them
/// 3. If there are no error listeners, the error will be thrown as a JavaScript error
///
// # Returns
/// - `Ok(true)` if the error was emitted and handled by listeners
/// - `Ok(false)` if there was no error to emit
/// - `Err(...)` if there was an error but no listeners to handle it
pub trait EmitError {
    fn emit_error<M>(
        self,
        this: This<JSObject>,
        ctx: &JSContext,
        id: &'static str,
    ) -> JSResult<bool>
    where
        M: EmitterExt;
}

impl<T, E> EmitError for std::result::Result<T, E>
where
    E: ToString,
{
    fn emit_error<M>(
        self,
        this: This<JSObject>,
        ctx: &JSContext,
        id: &'static str,
    ) -> JSResult<bool>
    where
        M: EmitterExt,
    {
        let _ = id;
        match self {
            Err(err) => {
                let key = EventKey::String(String::from("error"));
                let err = err.to_string();
                let value = JSValue::from_rust(ctx, err.as_str());

                match M::do_emit(this, key.clone(), Rest(vec![value])) {
                    Ok(has) if has => Ok(true),
                    _ => Err(HostError::new(rong::error::E_ERROR, err).into()),
                }
            }
            Ok(_) => Ok(false),
        }
    }
}

// Automatically implement EmitterExt for all types that implement Emitter
impl<T> EmitterExt for T where T: Emitter + IntoJSValue<JSEngineValue> {}

pub trait EmitterExt
where
    Self: Emitter + IntoJSValue<JSEngineValue>,
{
    /// Inherits the prototype of the nodejs EventEmitter class constructor, adding node
    /// event emitter related prototype methods to the JavaScript environment
    fn add_node_event_target_prototype(ctx: &JSContext) -> JSResult<()> {
        let proto = Class::prototype::<Self>(ctx)?;

        // method: on and addListener
        let on = JSFunc::new(
            ctx,
            |this: This<JSObject>, key: EventKey, listener: JSFunc| {
                Self::add_event_listener(this, key, listener, false, false)
            },
        )?
        .name("on")?;
        proto.set("on", on.clone())?;
        proto.set("addListener", on)?;

        // method: once
        let once = JSFunc::new(
            ctx,
            |this: This<JSObject>, key: EventKey, listener: JSFunc| {
                Self::add_event_listener(this, key, listener, false, true)
            },
        )?
        .name("once")?;
        proto.set("once", once)?;

        // method: off and removeListener
        let off = JSFunc::new(ctx, Self::remove_event_listener)?.name("off")?;
        proto.set("off", off.clone())?;
        proto.set("removeListener", off)?;

        // methods: prependListener, prependOnceListener
        let prepend = JSFunc::new(
            ctx,
            |this: This<JSObject>, key: EventKey, listener: JSFunc| {
                Self::add_event_listener(this, key, listener, true, false)
            },
        )?
        .name("prependListener")?;
        let prepend_once = JSFunc::new(
            ctx,
            |this: This<JSObject>, key: EventKey, listener: JSFunc| {
                Self::add_event_listener(this, key, listener, true, true)
            },
        )?
        .name("prependOnceListener")?;
        proto.set("prependListener", prepend)?;
        proto.set("prependOnceListener", prepend_once)?;

        // method: eventNames
        let event_names = JSFunc::new(ctx, Self::event_names)?.name("eventNames")?;
        proto.set("eventNames", event_names)?;

        // method: emit
        let emit = JSFunc::new(ctx, Self::do_emit)?.name("emit")?;
        proto.set("emit", emit)?;

        // method: getMaxListeners
        let max_listener = JSFunc::new(ctx, Self::get_max_listeners)?.name("getMaxListeners")?;
        proto.set("getMaxListeners", max_listener)?;

        // method: setMaxListeners
        let max_listener = JSFunc::new(ctx, Self::set_max_listeners)?.name("setMaxListeners")?;
        proto.set("setMaxListeners", max_listener)?;

        // method: removeAllListeners
        let remove_all =
            JSFunc::new(ctx, Self::remove_all_listeners)?.name("removeAllListeners")?;
        proto.set("removeAllListeners", remove_all)?;

        // method: listenerCount
        let listener_count = JSFunc::new(ctx, Self::listener_count)?.name("listenerCount")?;
        proto.set("listenerCount", listener_count)?;

        Ok(())
    }

    /// Inherits the prototype of the Web EventTarget class constructor, adding Web
    /// event target related prototype methods to the JavaScript environment
    fn add_web_event_target_prototype(ctx: &JSContext) -> JSResult<()> {
        let proto = Class::prototype::<Self>(ctx)?;

        let on = JSFunc::new(
            ctx,
            |this: This<JSObject>, key: EventKey, listener: JSFunc| {
                Self::add_event_listener(this, key, listener, false, false)
            },
        )?
        .name("addEventListener")?;
        proto.set("addEventListener", on)?;

        let off = JSFunc::new(ctx, Self::remove_event_listener)?.name("removeEventListener")?;
        proto.set("removeEventListener", off)?;

        let dispatch = JSFunc::new(ctx, Self::dispatch_event)?.name("dispatchEvent")?;
        proto.set("dispatchEvent", dispatch)?;

        Ok(())
    }

    fn add_event_listener(
        this: This<JSObject>,
        key: EventKey,
        listener: JSFunc,
        prepend: bool,
        once: bool,
    ) -> JSResult<()> {
        let target = this.borrow::<Self>()?;
        let events = target.get_event_emitter();
        let is_new = events.add_listener(key.clone(), listener, prepend, once)?;
        if is_new {
            target.on_event_changed(key, true)?;
        }
        Ok(())
    }

    fn remove_event_listener(
        this: This<JSObject>,
        key: EventKey,
        listener: JSFunc,
    ) -> JSResult<()> {
        let target = this.borrow::<Self>()?;
        let events = target.get_event_emitter();
        events.remove_listener(key, listener);
        Ok(())
    }

    fn event_names(this: This<JSObject>) -> JSResult<Vec<EventKey>> {
        let target = this.borrow::<Self>()?;
        let events = target.get_event_emitter();
        events.event_names()
    }

    /// Emits an event with the given key and arguments.
    ///
    /// Returns `JSResult<bool>` where:
    /// - `true` if the event was successfully emitted
    /// - `false` if there were no listeners for the event
    /// - `Err` if an error occurred during emission
    fn do_emit(this: This<JSObject>, key: EventKey, args: Rest<JSValue>) -> JSResult<bool> {
        let target = this.borrow::<Self>()?;
        let events = target.get_event_emitter();
        let mut is_empty = false;
        let has = events.do_emit(this.0.clone(), key.clone(), args.0, &mut is_empty);
        if is_empty {
            target.on_event_changed(key, false)?;
        }
        has
    }

    fn dispatch_event(this: This<JSObject>, event: JSValue) -> JSResult<bool> {
        if let Some(obj) = event.clone().into_object() {
            let event_type = match obj.get::<_, String>("type") {
                Ok(t) => t,
                Err(_) => return Ok(true),
            };

            let key = EventKey::String(event_type);
            Self::do_emit(this, key, Rest(vec![event]))?;
        }
        Ok(true)
    }

    fn get_max_listeners(this: This<JSObject>) -> JSResult<u32> {
        let target = this.borrow::<Self>()?;
        let events = target.get_event_emitter();
        Ok(events.get_max_listeners())
    }

    fn set_max_listeners(this: This<JSObject>, num: u32) -> JSResult<JSObject> {
        let target = this.borrow::<Self>()?;
        let events = target.get_event_emitter();
        events.set_max_listeners(num);
        Ok(this.0.clone())
    }

    fn remove_all_listeners(this: This<JSObject>, key: Optional<EventKey>) -> JSResult<()> {
        let target = this.borrow::<Self>()?;
        let events = target.get_event_emitter();
        events.remove_all_listeners(key.0)?;
        Ok(())
    }

    fn listener_count(
        this: This<JSObject>,
        key: EventKey,
        listener: Optional<JSFunc>,
    ) -> JSResult<u32> {
        let target = this.borrow::<Self>()?;
        let events = target.get_event_emitter();
        events.listener_count(key, listener.0)
    }
}

impl EventEmitter {
    /// Returns whether this emitter currently has any listeners for `key`.
    pub fn has_listeners(&self, key: &EventKey) -> bool {
        self.inner
            .lock()
            .map(|events| {
                events
                    .listeners
                    .get(key)
                    .is_some_and(|listeners| !listeners.is_empty())
            })
            .unwrap_or(false)
    }

    /// Returns the first listener function for the given event key, or None if no listeners exist
    pub fn get_listener(&self, key: &EventKey) -> Option<JSFunc> {
        self.inner.lock().ok().and_then(|inner| {
            inner
                .listeners
                .get(key)
                .and_then(|listeners| listeners.front().map(|l| l.listener.clone()))
        })
    }

    /// Adds an event listener
    ///
    /// # Arguments
    /// - `key`: The event key
    /// - `listener`: The event listener of JS Function
    /// - `prepend`: Whether to add to the beginning of the listener list
    /// - `once`: Whether the listener should only execute once
    ///
    /// # Returns
    /// Returns a JSResult<bool> indicating if this is a new event type
    ///
    /// # Errors
    /// Returns an error if the listener count exceeds the maximum limit
    fn add_listener(
        &self,
        key: EventKey,
        listener: JSFunc,
        prepend: bool,
        once: bool,
    ) -> JSResult<bool> {
        let mut events = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let is_new = !events.listeners.contains_key(&key);

        let max_listener = events.max_listener;

        let current_len = events
            .listeners
            .get(&key)
            .map(|listeners| listeners.len())
            .unwrap_or(0);

        if current_len as u32 >= max_listener {
            let warning = format!(
                "EventEmitter overflow: {} listeners added. Use setMaxListeners() to increase limit",
                current_len + 1,
            );
            return Err(HostError::new(rong::error::E_INVALID_STATE, warning).into());
        }

        let listener = EventListener { listener, once };
        let listeners = events.listeners.entry(key).or_default();
        if prepend {
            listeners.push_front(listener);
        } else {
            listeners.push_back(listener);
        }

        Ok(is_new)
    }

    fn remove_listener(&self, key: EventKey, listener: JSFunc) {
        let mut events = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        events
            .listeners
            .entry(key)
            .and_modify(|listeners| listeners.retain(|l| l.listener != listener));
    }

    fn event_names(&self) -> JSResult<Vec<EventKey>> {
        let events = self
            .inner
            .lock()
            .map_err(|_| HostError::new(rong::error::E_INTERNAL, "EventEmitter is poisoned"))?;
        Ok(events.listeners.keys().cloned().collect::<Vec<_>>())
    }

    fn do_emit(
        &self,
        this: JSObject,
        key: EventKey,
        args: Vec<JSValue>,
        is_empty: &mut bool,
    ) -> JSResult<bool> {
        // Capture the listeners in a temporary vector so callback execution does not hold the lock.
        let listeners_snapshot = {
            let events = self
                .inner
                .lock()
                .map_err(|_| HostError::new(rong::error::E_INTERNAL, "EventEmitter is poisoned"))?;
            match events.listeners.get(&key) {
                Some(listeners) if !listeners.is_empty() => {
                    let mut snapshot = Vec::with_capacity(listeners.len());
                    snapshot.extend(listeners.iter().cloned());
                    snapshot
                }
                _ => return Ok(false),
            }
        };

        let mut once_listeners_to_remove: Vec<JSFunc> = Vec::new();

        // Execute callbacks without holding the mutex to avoid deadlocks when listeners mutate state.
        for listener in listeners_snapshot.iter() {
            if listener.once {
                once_listeners_to_remove.push(listener.listener.clone());
            }

            // Propagate exceptions to JS (throw/reject).
            if let Err(e) = listener
                .listener
                .call::<_, ()>(Some(this.clone()), (args.clone(),))
            {
                if !once_listeners_to_remove.is_empty() {
                    let mut events = self.inner.lock().map_err(|_| {
                        HostError::new(rong::error::E_INTERNAL, "EventEmitter is poisoned")
                    })?;
                    if let Some(listeners) = events.listeners.get_mut(&key) {
                        listeners.retain(|l| !once_listeners_to_remove.contains(&l.listener));
                        if listeners.is_empty() {
                            events.listeners.remove(&key);
                            *is_empty = true;
                        }
                    } else {
                        *is_empty = true;
                    }
                }

                return Err(e);
            }
        }

        let mut events = self
            .inner
            .lock()
            .map_err(|_| HostError::new(rong::error::E_INTERNAL, "EventEmitter is poisoned"))?;
        let mut should_mark_empty = false;

        {
            if let Some(listeners) = events.listeners.get_mut(&key) {
                if !once_listeners_to_remove.is_empty() {
                    listeners.retain(|l| !once_listeners_to_remove.contains(&l.listener));
                }

                if listeners.is_empty() {
                    should_mark_empty = true;
                }
            } else {
                should_mark_empty = true;
            }
        }

        if should_mark_empty {
            events.listeners.remove(&key);
            *is_empty = true;
        }

        Ok(true)
    }

    fn remove_all_listeners(&self, key: Option<EventKey>) -> JSResult<()> {
        let mut events = self
            .inner
            .lock()
            .map_err(|_| HostError::new(rong::error::E_INTERNAL, "EventEmitter is poisoned"))?;
        match key {
            Some(key) => {
                events.listeners.remove(&key);
            }
            None => {
                // Clear all listeners and ensure proper cleanup
                for listeners in events.listeners.values_mut() {
                    // Use drain to ensure each listener is properly dropped
                    listeners.drain(..).for_each(|_| {});
                }
                events.listeners.clear();
            }
        }
        Ok(())
    }

    fn listener_count(&self, key: EventKey, listener: Option<JSFunc>) -> JSResult<u32> {
        let events = self
            .inner
            .lock()
            .map_err(|_| HostError::new(rong::error::E_INTERNAL, "EventEmitter is poisoned"))?;
        let count = if let Some(listeners) = events.listeners.get(&key) {
            if let Some(listener) = listener {
                listeners.iter().filter(|l| l.listener == listener).count()
            } else {
                listeners.len()
            }
        } else {
            0
        };
        Ok(count as _)
    }

    pub fn set_max_listeners(&self, max: u32) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.max_listener = max;
        }
    }

    pub fn get_max_listeners(&self) -> u32 {
        self.inner
            .lock()
            .map(|guard| guard.max_listener)
            .unwrap_or(10)
    }
}

/// Represents an event emitter that follows the Node.js EventEmitter pattern.
///
/// This struct provides an implementation of the event emitter pattern,
/// allowing objects to emit named events that cause listener functions to be called.
///
/// # Key Features
/// - Thread-safe event handling through internal Mutex
/// - Support for multiple listeners per event
/// - Configurable maximum number of listeners
/// - Once-only event listeners
#[js_class]
pub struct EventEmitter {
    inner: Rc<Mutex<EventEmitterInner>>,
}

struct EventEmitterInner {
    listeners: HashMap<EventKey, VecDeque<EventListener>>,
    max_listener: u32,
}

// Ensure EventEmitter releases resources when dropped
impl Drop for EventEmitter {
    fn drop(&mut self) {
        let count = Rc::strong_count(&self.inner);
        if count == 1 {
            let _ = self.remove_all_listeners(None);
        }
    }
}

impl Default for EventEmitter {
    fn default() -> Self {
        Self {
            inner: Rc::new(Mutex::new(EventEmitterInner {
                listeners: HashMap::new(),
                max_listener: 10,
            })),
        }
    }
}

#[js_class]
impl EventEmitter {
    #[js_method(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    #[js_method(gc_mark)]
    pub fn gc_mark_with<F>(&self, mut mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
        if let Ok(events) = self.inner.lock() {
            for (key, listeners) in &events.listeners {
                if let EventKey::Symbol(symbol) = key {
                    mark_fn(symbol.as_js_value());
                }

                for listener in listeners {
                    mark_fn(listener.listener.as_js_value());
                }
            }
        }
    }
}

impl Emitter for EventEmitter {
    fn get_event_emitter(&self) -> EventEmitter {
        self.clone()
    }
}
