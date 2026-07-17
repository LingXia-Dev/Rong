#![allow(clippy::missing_const_for_thread_local)]

use crate::{ArkJSContext, ArkJSValue, arkjs};
use rong_core::{JSClass, JSClassExt, JSContextImpl, JSTypeOf, JSValueImpl};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::ptr;

/// Data stored alongside each callable function instance's callback.
/// Passed via the callback struct's `data` field.
struct CallableFuncState {
    callback: arkjs::JSVM_CallbackStruct,
    func_ref: arkjs::JSVM_Ref,
}

struct CallableWrapperData {
    native_data: *mut std::ffi::c_void,
    native_finalizer: arkjs::JSVM_Finalize,
    state: *mut CallableFuncState,
}

/// Per-class registration info stored by constructor pointer.
struct ClassInfo {
    env: arkjs::JSVM_Env,
    finalizer_ptr: usize,
    callable: bool,
    constructor_callback: *mut arkjs::JSVM_CallbackStruct,
    /// For callable classes: the monomorphized native_func_callback::<JC> pointer.
    native_callback: Option<
        unsafe extern "C" fn(arkjs::JSVM_Env, arkjs::JSVM_CallbackInfo) -> arkjs::JSVM_Value,
    >,
    /// Unique type tag for instance_of checks via OH_JSVM_CheckObjectTypeTag.
    type_tag: arkjs::JSVM_TypeTag,
}

thread_local! {
    /// Maps constructor pointer → class registration info.
    static CLASS: RefCell<HashMap<usize, ClassInfo>> = RefCell::new(HashMap::new());

    /// When true, the generic_constructor was triggered by make_instance
    /// (via OH_JSVM_NewInstance) and should skip data_constructor — just return `this`.
    /// make_instance will handle wrapping native data afterwards.
    pub(crate) static MAKE_INSTANCE: Cell<bool> = const { Cell::new(false) };

    /// When inside generic_constructor (JS `new Class()`), holds the current `this_arg`.
    /// make_instance checks this to wrap data onto the existing instance instead of
    /// creating a second one (JSVM ignores constructor return values).
    pub(crate) static CONSTRUCTOR_THIS: Cell<arkjs::JSVM_Value> = const { Cell::new(std::ptr::null_mut()) };

}

unsafe fn read_callback_info(
    env: arkjs::JSVM_Env,
    info: arkjs::JSVM_CallbackInfo,
) -> Option<(
    Vec<arkjs::JSVM_Value>,
    arkjs::JSVM_Value,
    *mut std::ffi::c_void,
)> {
    let mut argc: usize = 0;
    let mut this_arg: arkjs::JSVM_Value = ptr::null_mut();
    let mut data: *mut std::ffi::c_void = ptr::null_mut();
    let status = unsafe {
        arkjs::OH_JSVM_GetCbInfo(
            env,
            info,
            &mut argc,
            ptr::null_mut(),
            &mut this_arg,
            &mut data,
        )
    };
    if status != arkjs::JSVM_Status_JSVM_OK {
        return None;
    }

    let mut argv = vec![ptr::null_mut(); argc];
    let status = unsafe {
        arkjs::OH_JSVM_GetCbInfo(
            env,
            info,
            &mut argc,
            argv.as_mut_ptr(),
            &mut this_arg,
            &mut data,
        )
    };
    if status != arkjs::JSVM_Status_JSVM_OK {
        return None;
    }

    argv.truncate(argc);
    Some((argv, this_arg, data))
}

unsafe fn sync_instance_prototype_from_new_target(
    env: arkjs::JSVM_Env,
    info: arkjs::JSVM_CallbackInfo,
    this_arg: arkjs::JSVM_Value,
) -> bool {
    let mut new_target: arkjs::JSVM_Value = ptr::null_mut();
    if unsafe { arkjs::OH_JSVM_GetNewTarget(env, info, &mut new_target) }
        != arkjs::JSVM_Status_JSVM_OK
        || new_target.is_null()
    {
        return true;
    }

    let mut proto: arkjs::JSVM_Value = ptr::null_mut();
    if unsafe {
        arkjs::OH_JSVM_GetNamedProperty(env, new_target, c"prototype".as_ptr() as _, &mut proto)
    } != arkjs::JSVM_Status_JSVM_OK
    {
        return false;
    }

    (unsafe { arkjs::OH_JSVM_ObjectSetPrototypeOf(env, this_arg, proto) })
        == arkjs::JSVM_Status_JSVM_OK
}

pub(crate) unsafe fn define_hidden_value_property(
    env: arkjs::JSVM_Env,
    object: arkjs::JSVM_Value,
    name: &CStr,
    value: arkjs::JSVM_Value,
) -> arkjs::JSVM_Status {
    let descriptor = arkjs::JSVM_PropertyDescriptor {
        utf8name: name.as_ptr(),
        name: ptr::null_mut(),
        method: ptr::null_mut(),
        getter: ptr::null_mut(),
        setter: ptr::null_mut(),
        value,
        // Hidden sidecar: not writable, enumerable, or configurable.
        attributes: arkjs::JSVM_PropertyAttributes_JSVM_DEFAULT,
    };

    unsafe { arkjs::OH_JSVM_DefineProperties(env, object, 1, &descriptor) }
}

pub(crate) unsafe fn finalize_native_data(
    env: arkjs::JSVM_Env,
    finalizer: arkjs::JSVM_Finalize,
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
    env: arkjs::JSVM_Env,
    info: arkjs::JSVM_CallbackInfo,
) -> arkjs::JSVM_Value
where
    JC: JSClass<ArkJSValue>,
{
    unsafe {
        let Some((argv, this_arg, _data)) = read_callback_info(env, info) else {
            let mut undefined: arkjs::JSVM_Value = ptr::null_mut();
            arkjs::OH_JSVM_GetUndefined(env, &mut undefined);
            return undefined;
        };

        // If called from make_instance, skip data_constructor — just return this.
        // make_instance will wrap native data via OH_JSVM_Wrap afterwards.
        if MAKE_INSTANCE.get() {
            return this_arg;
        }

        let ctx = ArkJSContext::from_borrowed_raw(env);
        let this = ArkJSValue::from_borrowed_raw(env, this_arg);
        let args: Vec<ArkJSValue> = argv
            .into_iter()
            .map(|value| ArkJSValue::from_borrowed_raw(env, value))
            .collect();

        // Store this_arg so make_instance wraps data onto it instead of
        // creating a second instance (JSVM ignores constructor return values).
        CONSTRUCTOR_THIS.set(this_arg);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            <JC as JSClassExt<ArkJSValue>>::construct_value(&ctx, this, args)
        }));
        CONSTRUCTOR_THIS.set(ptr::null_mut());

        match result {
            Ok(Ok(value)) => {
                let this = ArkJSValue::from_borrowed_raw(env, this_arg);
                if this.is_undefined() {
                    value.into_raw_value()
                } else {
                    if sync_instance_prototype_from_new_target(env, info, this_arg) {
                        this_arg
                    } else {
                        let mut undefined: arkjs::JSVM_Value = ptr::null_mut();
                        arkjs::OH_JSVM_GetUndefined(env, &mut undefined);
                        undefined
                    }
                }
            }
            Ok(Err(_)) | Err(_) => {
                let mut undefined: arkjs::JSVM_Value = ptr::null_mut();
                arkjs::OH_JSVM_GetUndefined(env, &mut undefined);
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
    env: arkjs::JSVM_Env,
    info: arkjs::JSVM_CallbackInfo,
) -> arkjs::JSVM_Value
where
    JC: JSClass<ArkJSValue>,
{
    unsafe {
        let Some((argv, this_arg, data)) = read_callback_info(env, info) else {
            let mut undefined: arkjs::JSVM_Value = ptr::null_mut();
            arkjs::OH_JSVM_GetUndefined(env, &mut undefined);
            return undefined;
        };

        let ctx = ArkJSContext::from_borrowed_raw(env);
        let func_data = &*(data as *const CallableFuncState);
        let mut func_value: arkjs::JSVM_Value = ptr::null_mut();
        arkjs::OH_JSVM_GetReferenceValue(env, func_data.func_ref, &mut func_value);

        let function = ArkJSValue::from_borrowed_raw(env, func_value);
        let this = ArkJSValue::from_borrowed_raw(env, this_arg);
        let args: Vec<ArkJSValue> = argv
            .into_iter()
            .map(|value| ArkJSValue::from_borrowed_raw(env, value))
            .collect();

        // Delegate to JSClassExt::call which unwraps the RustFunc from the function object
        let value = <JC as JSClassExt<ArkJSValue>>::call(&ctx, function, this, args);
        if value.is_exception() {
            let mut undefined: arkjs::JSVM_Value = ptr::null_mut();
            arkjs::OH_JSVM_GetUndefined(env, &mut undefined);
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
    ctx: &ArkJSContext,
    constructor: ArkJSValue,
    data: *mut (),
) -> ArkJSValue {
    let native_finalizer = get_finalizer_by_constructor(&constructor);
    let constructor_ptr = *constructor.as_raw_value() as usize;
    let native_callback = CLASS.with(|map| {
        map.borrow()
            .get(&constructor_ptr)
            .and_then(|info| info.native_callback)
    });

    let Some(callback_fn) = native_callback else {
        return ArkJSValue::create_undefined(ctx);
    };

    unsafe {
        let mut func_value: arkjs::JSVM_Value = ptr::null_mut();
        let state = Box::into_raw(Box::new(CallableFuncState {
            callback: arkjs::JSVM_CallbackStruct {
                callback: Some(callback_fn),
                data: ptr::null_mut(),
            },
            func_ref: ptr::null_mut(),
        }));

        let status = arkjs::OH_JSVM_CreateFunction(
            ctx.to_raw(),
            c"".as_ptr() as _,
            0,
            &mut (*state).callback as *mut _,
            &mut func_value,
        );

        if status != arkjs::JSVM_Status_JSVM_OK || func_value.is_null() {
            finalize_native_data(ctx.to_raw(), native_finalizer, data);
            let _ = Box::from_raw(state);
            return ArkJSValue::create_undefined(ctx);
        }

        let mut func_ref: arkjs::JSVM_Ref = ptr::null_mut();
        if arkjs::OH_JSVM_CreateReference(ctx.to_raw(), func_value, 1, &mut func_ref)
            != arkjs::JSVM_Status_JSVM_OK
        {
            finalize_native_data(ctx.to_raw(), native_finalizer, data);
            let _ = Box::from_raw(state);
            return ArkJSValue::create_undefined(ctx);
        }
        (*state).func_ref = func_ref;
        (*state).callback.data = state as *mut std::ffi::c_void;

        let mut wrapper_obj: arkjs::JSVM_Value = ptr::null_mut();
        if arkjs::OH_JSVM_CreateObject(ctx.to_raw(), &mut wrapper_obj) != arkjs::JSVM_Status_JSVM_OK
        {
            finalize_native_data(ctx.to_raw(), native_finalizer, data);
            arkjs::OH_JSVM_DeleteReference(ctx.to_raw(), func_ref);
            let _ = Box::from_raw(state);
            return ArkJSValue::create_undefined(ctx);
        }

        let wrapper_data = Box::into_raw(Box::new(CallableWrapperData {
            native_data: data as *mut std::ffi::c_void,
            native_finalizer,
            state,
        }));
        let wrap_status = arkjs::OH_JSVM_Wrap(
            ctx.to_raw(),
            wrapper_obj,
            wrapper_data as *mut std::ffi::c_void,
            Some(callable_wrapper_finalizer),
            ptr::null_mut(),
            ptr::null_mut(),
        );
        if wrap_status != arkjs::JSVM_Status_JSVM_OK {
            finalize_native_data(ctx.to_raw(), native_finalizer, data);
            let _ = Box::from_raw(wrapper_data);
            arkjs::OH_JSVM_DeleteReference(ctx.to_raw(), func_ref);
            let _ = Box::from_raw(state);
            return ArkJSValue::create_undefined(ctx);
        }
        if let Some(tag) = get_type_tag_by_constructor(&constructor) {
            let _ = arkjs::OH_JSVM_TypeTagObject(ctx.to_raw(), wrapper_obj, &tag);
        }
        let status =
            define_hidden_value_property(ctx.to_raw(), func_value, c"__rong_data", wrapper_obj);
        if status != arkjs::JSVM_Status_JSVM_OK {
            let mut stale: arkjs::JSVM_Value = ptr::null_mut();
            let _ = arkjs::OH_JSVM_GetAndClearLastException(ctx.to_raw(), &mut stale);
            return ArkJSValue::create_undefined(ctx);
        }

        ArkJSValue::from_owned_raw(ctx.to_raw(), func_value).with_object()
    }
}

unsafe extern "C" fn callable_wrapper_finalizer(
    env: arkjs::JSVM_Env,
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
                arkjs::OH_JSVM_DeleteReference(env, state.func_ref);
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
    _env: arkjs::JSVM_Env,
    finalize_data: *mut std::ffi::c_void,
    _finalize_hint: *mut std::ffi::c_void,
) where
    JC: JSClass<ArkJSValue>,
{
    if !finalize_data.is_null() {
        let _ = unsafe { Box::from_raw(finalize_data as *mut std::cell::RefCell<JC>) };
    }
}

/// Register a class.
/// Uses OH_JSVM_DefineClassWithPropertyHandler for callable classes (callAsFunctionCallback),
/// and OH_JSVM_DefineClass for regular data classes.
pub(crate) fn register_class_internal<JC>(ctx: &ArkJSContext, class_name: &str) -> ArkJSValue
where
    JC: JSClass<ArkJSValue>,
{
    unsafe {
        let class_name_cstr = CString::new(class_name).unwrap();
        let mut constructor: arkjs::JSVM_Value = ptr::null_mut();

        // Heap-allocated because JSVM stores the pointer
        let constructor_callback = Box::into_raw(Box::new(arkjs::JSVM_CallbackStruct {
            callback: Some(generic_constructor::<JC>),
            data: ptr::null_mut(),
        }));

        // All classes use OH_JSVM_DefineClass. For CALLABLE classes (RustFunc),
        // instances are created as native functions via OH_JSVM_CreateFunction in
        // make_callable_instance, giving correct `this` semantics.
        let status = arkjs::OH_JSVM_DefineClass(
            ctx.to_raw(),
            class_name_cstr.as_ptr(),
            class_name.len(),
            constructor_callback as *mut _,
            0,           // propertyCount
            ptr::null(), // properties
            &mut constructor,
        );

        if status == arkjs::JSVM_Status_JSVM_OK {
            // Set constructor name property
            let mut name_value: arkjs::JSVM_Value = ptr::null_mut();
            arkjs::OH_JSVM_CreateStringUtf8(
                ctx.to_raw(),
                class_name_cstr.as_ptr(),
                class_name.len(),
                &mut name_value,
            );
            arkjs::OH_JSVM_SetNamedProperty(
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

            // Generate a unique type tag from the finalizer pointer and a magic constant
            let type_tag = arkjs::JSVM_TypeTag {
                lower: finalizer_ptr as u64,
                upper: 0x524F4E47_524F4E47, // "RONGRONG" as magic
            };

            CLASS.with(|map| {
                map.borrow_mut().insert(
                    constructor as usize,
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

            ArkJSValue::from_owned_raw(ctx.to_raw(), constructor).with_object()
        } else {
            let _ = Box::from_raw(constructor_callback);
            ArkJSValue::create_undefined(ctx)
        }
    }
}

/// Retrieves the finalizer function pointer associated with a given constructor
pub(crate) fn get_finalizer_by_constructor(constructor: &ArkJSValue) -> arkjs::JSVM_Finalize {
    let constructor_ptr = *constructor.as_raw_value() as usize;
    CLASS.with(|map| {
        map.borrow().get(&constructor_ptr).map(|info| unsafe {
            std::mem::transmute::<
                usize,
                unsafe extern "C" fn(arkjs::JSVM_Env, *mut std::ffi::c_void, *mut std::ffi::c_void),
            >(info.finalizer_ptr)
        })
    })
}

/// Retrieves native data for a callable function instance.
/// Reads the `__rong_data` wrapper object property and calls OH_JSVM_Unwrap on it.
pub(crate) fn get_callable_func_data(value: &ArkJSValue) -> *mut () {
    unsafe {
        let env = *value.as_raw_context();
        let Some(wrapper) = get_callable_wrapper(value) else {
            return ptr::null_mut();
        };
        let mut data: *mut std::ffi::c_void = ptr::null_mut();
        let status = arkjs::OH_JSVM_Unwrap(env, wrapper, &mut data);
        if status == arkjs::JSVM_Status_JSVM_OK {
            let wrapper = &*(data as *const CallableWrapperData);
            wrapper.native_data as *mut ()
        } else {
            ptr::null_mut()
        }
    }
}

fn get_callable_wrapper(value: &ArkJSValue) -> Option<arkjs::JSVM_Value> {
    unsafe {
        let env = *value.as_raw_context();
        let val = value.resolve_handle();
        if val.is_null() {
            return None;
        }

        let mut wrapper: arkjs::JSVM_Value = ptr::null_mut();
        let status =
            arkjs::OH_JSVM_GetNamedProperty(env, val, c"__rong_data".as_ptr() as _, &mut wrapper);
        if status != arkjs::JSVM_Status_JSVM_OK || wrapper.is_null() {
            return None;
        }

        let mut value_type: arkjs::JSVM_ValueType = arkjs::JSVM_ValueType_JSVM_UNDEFINED;
        arkjs::OH_JSVM_Typeof(env, wrapper, &mut value_type);
        if value_type == arkjs::JSVM_ValueType_JSVM_OBJECT {
            Some(wrapper)
        } else {
            None
        }
    }
}

pub(crate) fn callable_instance_of(value: &ArkJSValue, constructor: &ArkJSValue) -> bool {
    let Some(tag) = get_type_tag_by_constructor(constructor) else {
        return false;
    };
    let Some(wrapper) = get_callable_wrapper(value) else {
        return false;
    };

    unsafe {
        let mut matches = false;
        arkjs::OH_JSVM_CheckObjectTypeTag(*value.as_raw_context(), wrapper, &tag, &mut matches)
            == arkjs::JSVM_Status_JSVM_OK
            && matches
    }
}

pub(crate) fn cleanup_class_cache(env: arkjs::JSVM_Env) {
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

/// Retrieves the type tag associated with a given constructor for instance_of checks.
pub(crate) fn get_type_tag_by_constructor(constructor: &ArkJSValue) -> Option<arkjs::JSVM_TypeTag> {
    let constructor_ptr = *constructor.as_raw_value() as usize;
    CLASS.with(|map| map.borrow().get(&constructor_ptr).map(|info| info.type_tag))
}

/// Checks if a constructor was registered as callable (CALLABLE=true)
pub(crate) fn is_callable_constructor(constructor: &ArkJSValue) -> bool {
    let constructor_ptr = *constructor.as_raw_value() as usize;
    CLASS.with(|map| {
        map.borrow()
            .get(&constructor_ptr)
            .is_some_and(|info| info.callable)
    })
}
