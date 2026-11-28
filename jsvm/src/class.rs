#![allow(clippy::missing_const_for_thread_local)]

use crate::{JSVMContext, JSVMValue, jsvm};
use rong_core::{JSClass, JSClassExt, JSContextImpl, JSTypeOf, JSValueImpl};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::ptr;

/// Data stored alongside each callable function instance's callback.
/// Passed via the callback struct's `data` field.
struct CallableFuncState {
    callback: jsvm::JSVM_CallbackStruct,
    func_ref: jsvm::JSVM_Ref,
}

struct CallableWrapperData {
    native_data: *mut std::ffi::c_void,
    native_finalizer: jsvm::JSVM_Finalize,
    state: *mut CallableFuncState,
}

/// Per-class registration info, keyed by the id stored on the constructor.
struct ClassInfo {
    env: jsvm::JSVM_Env,
    finalizer_ptr: usize,
    callable: bool,
    constructor_callback: *mut jsvm::JSVM_CallbackStruct,
    /// For callable classes: the monomorphized native_func_callback::<JC> pointer.
    native_callback:
        Option<unsafe extern "C" fn(jsvm::JSVM_Env, jsvm::JSVM_CallbackInfo) -> jsvm::JSVM_Value>,
    /// Unique type tag for instance_of checks via OH_JSVM_CheckObjectTypeTag.
    type_tag: jsvm::JSVM_TypeTag,
}

/// What a constructor's class registration says about its instances.
#[derive(Clone, Copy)]
pub(crate) struct ClassMeta {
    pub(crate) id: u32,
    pub(crate) finalizer: jsvm::JSVM_Finalize,
    pub(crate) callable: bool,
    native_callback:
        Option<unsafe extern "C" fn(jsvm::JSVM_Env, jsvm::JSVM_CallbackInfo) -> jsvm::JSVM_Value>,
    pub(crate) type_tag: jsvm::JSVM_TypeTag,
}

/// Own property on each registered constructor holding its `CLASS` id. A
/// handle address cannot key the map: the slot is reused once the native call
/// that registered the class returns.
const CLASS_ID_KEY: &CStr = c"__rong_class_id";

thread_local! {
    /// Maps class id → class registration info.
    static CLASS: RefCell<HashMap<u32, ClassInfo>> = RefCell::new(HashMap::new());

    static NEXT_CLASS_ID: Cell<u32> = const { Cell::new(1) };

    /// When true, the generic_constructor was triggered by make_instance
    /// (via OH_JSVM_NewInstance) and should skip data_constructor — just return `this`.
    /// make_instance will handle wrapping native data afterwards.
    pub(crate) static MAKE_INSTANCE: Cell<bool> = const { Cell::new(false) };

    /// When inside generic_constructor (JS `new Class()`), holds the current `this_arg`
    /// and the class being constructed, keyed by its finalizer. make_instance wraps
    /// data onto that `this` instead of creating a second instance (JSVM ignores
    /// constructor return values), but only for that class: an instance of another
    /// class built inside the constructor needs its own object.
    pub(crate) static CONSTRUCTOR_THIS: Cell<(jsvm::JSVM_Value, usize)> = const { Cell::new((std::ptr::null_mut(), 0)) };

}

unsafe fn read_callback_info(
    env: jsvm::JSVM_Env,
    info: jsvm::JSVM_CallbackInfo,
) -> Option<(
    Vec<jsvm::JSVM_Value>,
    jsvm::JSVM_Value,
    *mut std::ffi::c_void,
)> {
    let mut argc: usize = 0;
    let mut this_arg: jsvm::JSVM_Value = ptr::null_mut();
    let mut data: *mut std::ffi::c_void = ptr::null_mut();
    let status = unsafe {
        jsvm::OH_JSVM_GetCbInfo(
            env,
            info,
            &mut argc,
            ptr::null_mut(),
            &mut this_arg,
            &mut data,
        )
    };
    if status != jsvm::JSVM_Status_JSVM_OK {
        return None;
    }

    let mut argv = vec![ptr::null_mut(); argc];
    let status = unsafe {
        jsvm::OH_JSVM_GetCbInfo(
            env,
            info,
            &mut argc,
            argv.as_mut_ptr(),
            &mut this_arg,
            &mut data,
        )
    };
    if status != jsvm::JSVM_Status_JSVM_OK {
        return None;
    }

    argv.truncate(argc);
    Some((argv, this_arg, data))
}

unsafe fn sync_instance_prototype_from_new_target(
    env: jsvm::JSVM_Env,
    info: jsvm::JSVM_CallbackInfo,
    this_arg: jsvm::JSVM_Value,
) -> bool {
    let mut new_target: jsvm::JSVM_Value = ptr::null_mut();
    if unsafe { jsvm::OH_JSVM_GetNewTarget(env, info, &mut new_target) }
        != jsvm::JSVM_Status_JSVM_OK
        || new_target.is_null()
    {
        return true;
    }

    let mut proto: jsvm::JSVM_Value = ptr::null_mut();
    if unsafe {
        jsvm::OH_JSVM_GetNamedProperty(env, new_target, c"prototype".as_ptr() as _, &mut proto)
    } != jsvm::JSVM_Status_JSVM_OK
    {
        return false;
    }

    (unsafe { jsvm::OH_JSVM_ObjectSetPrototypeOf(env, this_arg, proto) })
        == jsvm::JSVM_Status_JSVM_OK
}

pub(crate) unsafe fn define_hidden_value_property(
    env: jsvm::JSVM_Env,
    object: jsvm::JSVM_Value,
    name: &CStr,
    value: jsvm::JSVM_Value,
) -> jsvm::JSVM_Status {
    let descriptor = jsvm::JSVM_PropertyDescriptor {
        utf8name: name.as_ptr(),
        name: ptr::null_mut(),
        method: ptr::null_mut(),
        getter: ptr::null_mut(),
        setter: ptr::null_mut(),
        value,
        // Hidden sidecar: not writable, enumerable, or configurable.
        attributes: jsvm::JSVM_PropertyAttributes_JSVM_DEFAULT,
    };

    unsafe { jsvm::OH_JSVM_DefineProperties(env, object, 1, &descriptor) }
}

pub(crate) unsafe fn finalize_native_data(
    env: jsvm::JSVM_Env,
    finalizer: jsvm::JSVM_Finalize,
    data: *mut (),
) {
    if data.is_null() {
        return;
    }

    if let Some(finalizer) = finalizer {
        unsafe {
            finalizer(env, data as *mut std::ffi::c_void, ptr::null_mut());
        }
    }
}

unsafe extern "C" fn generic_constructor<JC>(
    env: jsvm::JSVM_Env,
    info: jsvm::JSVM_CallbackInfo,
) -> jsvm::JSVM_Value
where
    JC: JSClass<JSVMValue>,
{
    unsafe {
        let Some((argv, this_arg, data)) = read_callback_info(env, info) else {
            let mut undefined: jsvm::JSVM_Value = ptr::null_mut();
            jsvm::OH_JSVM_GetUndefined(env, &mut undefined);
            return undefined;
        };

        // If called from make_instance, skip data_constructor — just return this.
        // make_instance will wrap native data via OH_JSVM_Wrap afterwards.
        if MAKE_INSTANCE.get() {
            return this_arg;
        }

        let ctx = JSVMContext::from_borrowed_raw(env);
        let this = JSVMValue::from_borrowed_raw(env, this_arg);
        let args: Vec<JSVMValue> = argv
            .into_iter()
            .map(|value| JSVMValue::from_borrowed_raw(env, value))
            .collect();

        // Store this_arg so make_instance wraps data onto it instead of
        // creating a second instance (JSVM ignores constructor return values).
        // Restore the outer value afterwards: constructors can nest.
        // The callback data is the class id (see `register_class_internal`).
        let class_key = data as usize;
        let outer = CONSTRUCTOR_THIS.replace((this_arg, class_key));

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            <JC as JSClassExt<JSVMValue>>::construct_value(&ctx, this, args)
        }));
        CONSTRUCTOR_THIS.set(outer);

        match result {
            Ok(Ok(value)) => {
                let this = JSVMValue::from_borrowed_raw(env, this_arg);
                if this.is_undefined() {
                    value.into_raw_value()
                } else {
                    if sync_instance_prototype_from_new_target(env, info, this_arg) {
                        this_arg
                    } else {
                        let mut undefined: jsvm::JSVM_Value = ptr::null_mut();
                        jsvm::OH_JSVM_GetUndefined(env, &mut undefined);
                        undefined
                    }
                }
            }
            Ok(Err(_)) | Err(_) => {
                let mut undefined: jsvm::JSVM_Value = ptr::null_mut();
                jsvm::OH_JSVM_GetUndefined(env, &mut undefined);
                undefined
            }
        }
    }
}

/// Native function callback for callable class instances (RustFunc).
///
/// Unlike `callAsFunctionCallback` (which gives callee as `this_arg`), functions created
/// via `OH_JSVM_CreateFunction` receive the proper JS `this` as `this_arg`.
/// The function object (callee) is retrieved from `data`.
unsafe extern "C" fn native_func_callback<JC>(
    env: jsvm::JSVM_Env,
    info: jsvm::JSVM_CallbackInfo,
) -> jsvm::JSVM_Value
where
    JC: JSClass<JSVMValue>,
{
    unsafe {
        let Some((argv, this_arg, data)) = read_callback_info(env, info) else {
            let mut undefined: jsvm::JSVM_Value = ptr::null_mut();
            jsvm::OH_JSVM_GetUndefined(env, &mut undefined);
            return undefined;
        };

        let ctx = JSVMContext::from_borrowed_raw(env);
        let func_data = &*(data as *const CallableFuncState);
        let mut func_value: jsvm::JSVM_Value = ptr::null_mut();
        jsvm::OH_JSVM_GetReferenceValue(env, func_data.func_ref, &mut func_value);
        if func_value.is_null() {
            let mut undefined: jsvm::JSVM_Value = ptr::null_mut();
            jsvm::OH_JSVM_GetUndefined(env, &mut undefined);
            return undefined;
        }

        let function = JSVMValue::from_borrowed_raw(env, func_value);
        let this = JSVMValue::from_borrowed_raw(env, this_arg);
        let args: Vec<JSVMValue> = argv
            .into_iter()
            .map(|value| JSVMValue::from_borrowed_raw(env, value))
            .collect();

        // Delegate to JSClassExt::call which unwraps the RustFunc from the function object
        let value = <JC as JSClassExt<JSVMValue>>::call(&ctx, function, this, args);
        if value.is_exception() {
            let mut undefined: jsvm::JSVM_Value = ptr::null_mut();
            jsvm::OH_JSVM_GetUndefined(env, &mut undefined);
            return undefined;
        }
        value.into_raw_value()
    }
}

/// Creates a native JS function backed by a CALLABLE class instance.
///
/// Instead of using `callAsFunctionCallback` (which doesn't provide the correct JS `this`
/// for method/getter calls), this creates a real function via `OH_JSVM_CreateFunction`.
/// The RustFunc data is wrapped on the function via `OH_JSVM_Wrap`, and a persistent
/// reference to the function is stored in the callback's `data` so the callback can
/// access the RustFunc closure.
pub(crate) fn make_callable_instance(
    ctx: &JSVMContext,
    constructor: JSVMValue,
    data: *mut (),
) -> JSVMValue {
    let _scope = crate::scope::OpScope::enter(ctx.to_raw());
    let Some(meta) = class_meta(&constructor) else {
        return JSVMValue::create_undefined(ctx);
    };
    let native_finalizer = meta.finalizer;
    let Some(callback_fn) = meta.native_callback else {
        return JSVMValue::create_undefined(ctx);
    };

    unsafe {
        let mut func_value: jsvm::JSVM_Value = ptr::null_mut();
        let state = Box::into_raw(Box::new(CallableFuncState {
            callback: jsvm::JSVM_CallbackStruct {
                callback: Some(callback_fn),
                data: ptr::null_mut(),
            },
            func_ref: ptr::null_mut(),
        }));

        let status = jsvm::OH_JSVM_CreateFunction(
            ctx.to_raw(),
            c"".as_ptr() as _,
            0,
            &mut (*state).callback as *mut _,
            &mut func_value,
        );

        if status != jsvm::JSVM_Status_JSVM_OK || func_value.is_null() {
            finalize_native_data(ctx.to_raw(), native_finalizer, data);
            let _ = Box::from_raw(state);
            return JSVMValue::create_undefined(ctx);
        }

        // Weak: a strong self-reference would keep the function, and so the
        // wrapper whose finalizer frees this state, alive forever. A function
        // is reachable while it runs, so the callback always resolves it.
        let mut func_ref: jsvm::JSVM_Ref = ptr::null_mut();
        if jsvm::OH_JSVM_CreateReference(ctx.to_raw(), func_value, 0, &mut func_ref)
            != jsvm::JSVM_Status_JSVM_OK
        {
            finalize_native_data(ctx.to_raw(), native_finalizer, data);
            let _ = Box::from_raw(state);
            return JSVMValue::create_undefined(ctx);
        }
        (*state).func_ref = func_ref;
        (*state).callback.data = state as *mut std::ffi::c_void;

        let mut wrapper_obj: jsvm::JSVM_Value = ptr::null_mut();
        if jsvm::OH_JSVM_CreateObject(ctx.to_raw(), &mut wrapper_obj) != jsvm::JSVM_Status_JSVM_OK {
            finalize_native_data(ctx.to_raw(), native_finalizer, data);
            jsvm::OH_JSVM_DeleteReference(ctx.to_raw(), func_ref);
            let _ = Box::from_raw(state);
            return JSVMValue::create_undefined(ctx);
        }

        let wrapper_data = Box::into_raw(Box::new(CallableWrapperData {
            native_data: data as *mut std::ffi::c_void,
            native_finalizer,
            state,
        }));
        let wrap_status = jsvm::OH_JSVM_Wrap(
            ctx.to_raw(),
            wrapper_obj,
            wrapper_data as *mut std::ffi::c_void,
            Some(callable_wrapper_finalizer),
            ptr::null_mut(),
            ptr::null_mut(),
        );
        if wrap_status != jsvm::JSVM_Status_JSVM_OK {
            finalize_native_data(ctx.to_raw(), native_finalizer, data);
            let _ = Box::from_raw(wrapper_data);
            jsvm::OH_JSVM_DeleteReference(ctx.to_raw(), func_ref);
            let _ = Box::from_raw(state);
            return JSVMValue::create_undefined(ctx);
        }
        let _ = jsvm::OH_JSVM_TypeTagObject(ctx.to_raw(), wrapper_obj, &meta.type_tag);
        let status =
            define_hidden_value_property(ctx.to_raw(), func_value, c"__rong_data", wrapper_obj);
        if status != jsvm::JSVM_Status_JSVM_OK {
            let mut stale: jsvm::JSVM_Value = ptr::null_mut();
            let _ = jsvm::OH_JSVM_GetAndClearLastException(ctx.to_raw(), &mut stale);
            return JSVMValue::create_undefined(ctx);
        }

        JSVMValue::from_owned_raw(ctx.to_raw(), func_value).with_object()
    }
}

unsafe extern "C" fn callable_wrapper_finalizer(
    env: jsvm::JSVM_Env,
    finalize_data: *mut std::ffi::c_void,
    _finalize_hint: *mut std::ffi::c_void,
) {
    if finalize_data.is_null() {
        return;
    }

    let wrapper = unsafe { Box::from_raw(finalize_data as *mut CallableWrapperData) };
    if !wrapper.state.is_null() {
        let state = unsafe { Box::from_raw(wrapper.state) };
        if !state.func_ref.is_null() {
            unsafe {
                jsvm::OH_JSVM_DeleteReference(env, state.func_ref);
            }
        }
    }

    if !wrapper.native_data.is_null()
        && let Some(finalizer) = wrapper.native_finalizer
    {
        unsafe {
            finalizer(env, wrapper.native_data, ptr::null_mut());
        }
    }
}

// Typed finalizer for specific class JC.
// `finalize_data` is the native data pointer passed to OH_JSVM_Wrap — free it directly.
// We must NOT use OH_JSVM_Unwrap or any V8 handle APIs here because
// the handle scope may already be closed during env destruction.
unsafe extern "C" fn finalizer<JC>(
    _env: jsvm::JSVM_Env,
    finalize_data: *mut std::ffi::c_void,
    _finalize_hint: *mut std::ffi::c_void,
) where
    JC: JSClass<JSVMValue>,
{
    if !finalize_data.is_null() {
        let _ = unsafe { Box::from_raw(finalize_data as *mut std::cell::RefCell<JC>) };
    }
}

/// Register a class.
/// Uses OH_JSVM_DefineClassWithPropertyHandler for callable classes (callAsFunctionCallback),
/// and OH_JSVM_DefineClass for regular data classes.
pub(crate) fn register_class_internal<JC>(ctx: &JSVMContext, class_name: &str) -> JSVMValue
where
    JC: JSClass<JSVMValue>,
{
    let _scope = crate::scope::OpScope::enter(ctx.to_raw());
    unsafe {
        let class_name_cstr = CString::new(class_name).unwrap();
        let mut constructor: jsvm::JSVM_Value = ptr::null_mut();

        let class_id = NEXT_CLASS_ID.get();
        NEXT_CLASS_ID.set(class_id.wrapping_add(1).max(1));

        // Heap-allocated because JSVM stores the pointer
        let constructor_callback = Box::into_raw(Box::new(jsvm::JSVM_CallbackStruct {
            callback: Some(generic_constructor::<JC>),
            data: class_id as usize as *mut std::ffi::c_void,
        }));

        // All classes use OH_JSVM_DefineClass. For CALLABLE classes (RustFunc),
        // instances are created as native functions via OH_JSVM_CreateFunction in
        // make_callable_instance, giving correct `this` semantics.
        let status = jsvm::OH_JSVM_DefineClass(
            ctx.to_raw(),
            class_name_cstr.as_ptr(),
            class_name.len(),
            constructor_callback as *mut _,
            0,           // propertyCount
            ptr::null(), // properties
            &mut constructor,
        );

        if status == jsvm::JSVM_Status_JSVM_OK {
            // Set constructor name property
            let mut name_value: jsvm::JSVM_Value = ptr::null_mut();
            jsvm::OH_JSVM_CreateStringUtf8(
                ctx.to_raw(),
                class_name_cstr.as_ptr(),
                class_name.len(),
                &mut name_value,
            );
            jsvm::OH_JSVM_SetNamedProperty(
                ctx.to_raw(),
                constructor,
                c"name".as_ptr() as _,
                name_value,
            );

            // Store constructor → class info mapping
            let finalizer_ptr = finalizer::<JC> as *const std::ffi::c_void as usize;
            let native_callback = if JC::CALLABLE {
                Some(native_func_callback::<JC> as unsafe extern "C" fn(_, _) -> _)
            } else {
                None
            };

            // From the class id, not a function address: the compiler folds
            // identical finalizers of same-layout classes into one address.
            let type_tag = jsvm::JSVM_TypeTag {
                lower: class_id as u64,
                upper: 0x524F4E47_524F4E47, // "RONGRONG" as magic
            };

            let mut id_value: jsvm::JSVM_Value = ptr::null_mut();
            let id_status = jsvm::OH_JSVM_CreateUint32(ctx.to_raw(), class_id, &mut id_value);
            if id_status != jsvm::JSVM_Status_JSVM_OK
                || define_hidden_value_property(ctx.to_raw(), constructor, CLASS_ID_KEY, id_value)
                    != jsvm::JSVM_Status_JSVM_OK
            {
                let mut stale: jsvm::JSVM_Value = ptr::null_mut();
                let _ = jsvm::OH_JSVM_GetAndClearLastException(ctx.to_raw(), &mut stale);
                let _ = Box::from_raw(constructor_callback);
                return JSVMValue::create_undefined(ctx);
            }

            CLASS.with(|map| {
                map.borrow_mut().insert(
                    class_id,
                    ClassInfo {
                        env: ctx.to_raw(),
                        finalizer_ptr,
                        callable: JC::CALLABLE,
                        constructor_callback,
                        native_callback,
                        type_tag,
                    },
                );
            });

            JSVMValue::from_owned_raw(ctx.to_raw(), constructor).with_object()
        } else {
            let _ = Box::from_raw(constructor_callback);
            JSVMValue::create_undefined(ctx)
        }
    }
}

/// Reads the class id from the constructor's own property; a subclass
/// constructor inherits the property but is not the registered class.
fn class_id(constructor: &JSVMValue) -> Option<u32> {
    unsafe {
        let env = *constructor.as_raw_context();
        let ctor = constructor.resolve_handle();
        if ctor.is_null() {
            return None;
        }
        let mut key: jsvm::JSVM_Value = ptr::null_mut();
        if jsvm::OH_JSVM_CreateStringUtf8(
            env,
            CLASS_ID_KEY.as_ptr(),
            CLASS_ID_KEY.to_bytes().len(),
            &mut key,
        ) != jsvm::JSVM_Status_JSVM_OK
        {
            return None;
        }
        let mut own = false;
        if jsvm::OH_JSVM_HasOwnProperty(env, ctor, key, &mut own) != jsvm::JSVM_Status_JSVM_OK
            || !own
        {
            let mut stale: jsvm::JSVM_Value = ptr::null_mut();
            let _ = jsvm::OH_JSVM_GetAndClearLastException(env, &mut stale);
            return None;
        }
        let mut id_value: jsvm::JSVM_Value = ptr::null_mut();
        let mut id = 0u32;
        if jsvm::OH_JSVM_GetProperty(env, ctor, key, &mut id_value) != jsvm::JSVM_Status_JSVM_OK
            || jsvm::OH_JSVM_GetValueUint32(env, id_value, &mut id) != jsvm::JSVM_Status_JSVM_OK
        {
            let mut stale: jsvm::JSVM_Value = ptr::null_mut();
            let _ = jsvm::OH_JSVM_GetAndClearLastException(env, &mut stale);
            return None;
        }
        Some(id)
    }
}

/// The registration of the class `constructor` was registered as, if any.
pub(crate) fn class_meta(constructor: &JSVMValue) -> Option<ClassMeta> {
    let _scope = crate::scope::OpScope::enter(*constructor.as_raw_context());
    let id = class_id(constructor)?;
    let env = *constructor.as_raw_context();
    CLASS.with(|map| {
        map.borrow()
            .get(&id)
            .filter(|info| info.env == env)
            .map(|info| ClassMeta {
                id,
                finalizer: Some(unsafe {
                    std::mem::transmute::<
                        usize,
                        unsafe extern "C" fn(
                            jsvm::JSVM_Env,
                            *mut std::ffi::c_void,
                            *mut std::ffi::c_void,
                        ),
                    >(info.finalizer_ptr)
                }),
                callable: info.callable,
                native_callback: info.native_callback,
                type_tag: info.type_tag,
            })
    })
}

/// Retrieves native data for a callable function instance.
/// Reads the `__rong_data` wrapper object property and calls OH_JSVM_Unwrap on it.
pub(crate) fn get_callable_func_data(value: &JSVMValue) -> *mut () {
    let _scope = crate::scope::OpScope::enter(*value.as_raw_context());
    unsafe {
        let env = *value.as_raw_context();
        let Some(wrapper) = get_callable_wrapper(value) else {
            return ptr::null_mut();
        };
        let mut data: *mut std::ffi::c_void = ptr::null_mut();
        let status = jsvm::OH_JSVM_Unwrap(env, wrapper, &mut data);
        if status == jsvm::JSVM_Status_JSVM_OK {
            let wrapper = &*(data as *const CallableWrapperData);
            wrapper.native_data as *mut ()
        } else {
            ptr::null_mut()
        }
    }
}

fn get_callable_wrapper(value: &JSVMValue) -> Option<jsvm::JSVM_Value> {
    unsafe {
        let env = *value.as_raw_context();
        let val = value.resolve_handle();
        if val.is_null() {
            return None;
        }

        let mut wrapper: jsvm::JSVM_Value = ptr::null_mut();
        let status =
            jsvm::OH_JSVM_GetNamedProperty(env, val, c"__rong_data".as_ptr() as _, &mut wrapper);
        if status != jsvm::JSVM_Status_JSVM_OK || wrapper.is_null() {
            return None;
        }

        let mut value_type: jsvm::JSVM_ValueType = jsvm::JSVM_ValueType_JSVM_UNDEFINED;
        jsvm::OH_JSVM_Typeof(env, wrapper, &mut value_type);
        if value_type == jsvm::JSVM_ValueType_JSVM_OBJECT {
            Some(wrapper)
        } else {
            None
        }
    }
}

pub(crate) fn callable_instance_of(value: &JSVMValue, meta: &ClassMeta) -> bool {
    let _scope = crate::scope::OpScope::enter(*value.as_raw_context());
    let tag = meta.type_tag;
    let Some(wrapper) = get_callable_wrapper(value) else {
        return false;
    };

    unsafe {
        let mut matches = false;
        jsvm::OH_JSVM_CheckObjectTypeTag(*value.as_raw_context(), wrapper, &tag, &mut matches)
            == jsvm::JSVM_Status_JSVM_OK
            && matches
    }
}

pub(crate) fn cleanup_class_cache(env: jsvm::JSVM_Env) {
    CLASS.with(|map| {
        let mut map = map.borrow_mut();
        let mut stale = Vec::new();
        for (constructor, info) in map.iter() {
            if info.env == env {
                stale.push(*constructor);
            }
        }

        for constructor in stale {
            if let Some(info) = map.remove(&constructor)
                && !info.constructor_callback.is_null()
            {
                unsafe {
                    let _ = Box::from_raw(info.constructor_callback);
                }
            }
        }
    });
}
