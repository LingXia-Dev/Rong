use crate::{ArkJSValue, arkjs};
use rong_core::engine::JSObjectOps;
use rong_core::{JSValueImpl, PropertyAttributes};

impl JSObjectOps for ArkJSValue {
    fn new_object(ctx: &Self::Context) -> Self {
        unsafe {
            let mut obj: arkjs::JSVM_Value = std::ptr::null_mut();
            let status = arkjs::OH_JSVM_CreateObject(ctx.to_raw(), &mut obj);
            if status == arkjs::JSVM_Status_JSVM_OK {
                ArkJSValue::from_owned_raw(ctx.to_raw(), obj).with_object()
            } else {
                Self::create_undefined(ctx)
            }
        }
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn make_instance(ctx: &Self::Context, constructor: Self, data: *mut ()) -> Self {
        // For callable classes (RustFunc), create a native JS function instead of
        // a class instance. This ensures `this_arg` in the callback is the correct
        // JS receiver (not the callee).
        if crate::class::is_callable_constructor(&constructor) {
            return crate::class::make_callable_instance(ctx, constructor, data);
        }

        unsafe {
            // If we're inside generic_constructor (JS `new Class(...)`), wrap data
            // onto the existing this_arg rather than creating a second instance.
            // JSVM ignores constructor return values — it always uses this_arg.
            let constructor_this = crate::class::CONSTRUCTOR_THIS.get();
            let instance = if !constructor_this.is_null() {
                crate::class::CONSTRUCTOR_THIS.set(std::ptr::null_mut());
                constructor_this
            } else {
                // Called from Rust (not from a JS constructor). Create a new instance.
                crate::class::MAKE_INSTANCE.set(true);

                let mut inst: arkjs::JSVM_Value = std::ptr::null_mut();
                let status = arkjs::OH_JSVM_NewInstance(
                    ctx.to_raw(),
                    constructor.resolve_handle(),
                    0,
                    std::ptr::null(),
                    &mut inst,
                );

                crate::class::MAKE_INSTANCE.set(false);

                if status != arkjs::JSVM_Status_JSVM_OK || inst.is_null() {
                    crate::class::finalize_native_data(
                        ctx.to_raw(),
                        crate::class::get_finalizer_by_constructor(&constructor),
                        data,
                    );
                    return Self::create_undefined(ctx);
                }
                inst
            };

            // Wrap native data for get_opaque / OH_JSVM_Unwrap
            if !data.is_null() {
                let finalizer_fn = crate::class::get_finalizer_by_constructor(&constructor);
                let status = arkjs::OH_JSVM_Wrap(
                    ctx.to_raw(),
                    instance,
                    data as *mut std::ffi::c_void,
                    finalizer_fn,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
                if status != arkjs::JSVM_Status_JSVM_OK {
                    crate::class::finalize_native_data(ctx.to_raw(), finalizer_fn, data);
                    let mut stale: arkjs::JSVM_Value = std::ptr::null_mut();
                    let _ = arkjs::OH_JSVM_GetAndClearLastException(ctx.to_raw(), &mut stale);
                    return Self::create_undefined(ctx);
                }

                // Tag the instance for type checking via instance_of
                if let Some(tag) = crate::class::get_type_tag_by_constructor(&constructor) {
                    arkjs::OH_JSVM_TypeTagObject(ctx.to_raw(), instance, &tag);
                }
            }

            ArkJSValue::from_owned_raw(ctx.to_raw(), instance).with_object()
        }
    }

    fn get_opaque(&self) -> *mut () {
        // For callable function instances, OH_JSVM_Unwrap doesn't work
        // (JSVM doesn't support wrapping function objects). Check the
        // side table first.
        let func_data = crate::class::get_callable_func_data(self);
        if !func_data.is_null() {
            return func_data;
        }
        unsafe {
            let obj = self.resolve_handle();
            let mut data: *mut std::ffi::c_void = std::ptr::null_mut();
            let status = arkjs::OH_JSVM_Unwrap(self.env, obj, &mut data);

            if status == arkjs::JSVM_Status_JSVM_OK {
                data as *mut ()
            } else {
                std::ptr::null_mut()
            }
        }
    }

    fn del_property(&self, key: Self) -> Result<bool, Self> {
        unsafe {
            let obj = self.resolve_handle();
            let mut result = false;
            let status =
                arkjs::OH_JSVM_DeleteProperty(self.env, obj, key.raw_value_for_api(), &mut result);
            if status == arkjs::JSVM_Status_JSVM_OK {
                Ok(result)
            } else {
                let mut exception: arkjs::JSVM_Value = std::ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(self.env, &mut exception);
                Err(ArkJSValue::from_owned_raw(self.env, exception)
                    .protect()
                    .with_exception())
            }
        }
    }

    fn has_property(&self, key: Self) -> Result<bool, Self> {
        unsafe {
            let obj = self.resolve_handle();
            let mut has_property = false;
            let status = arkjs::OH_JSVM_HasProperty(
                self.env,
                obj,
                key.raw_value_for_api(),
                &mut has_property,
            );
            if status == arkjs::JSVM_Status_JSVM_OK {
                Ok(has_property)
            } else {
                let mut exception: arkjs::JSVM_Value = std::ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(self.env, &mut exception);
                Err(ArkJSValue::from_owned_raw(self.env, exception)
                    .protect()
                    .with_exception())
            }
        }
    }

    fn set_property(&self, key: Self, value: Self) -> Result<(), Self> {
        let result = self.reflect_set(key.raw_value_for_api(), value.raw_value_for_api());
        if result._is_exception() {
            Err(result)
        } else {
            Ok(())
        }
    }

    fn set_prototype(&self, prototype: Self) -> bool {
        unsafe {
            let obj = self.resolve_handle();
            let status =
                arkjs::OH_JSVM_ObjectSetPrototypeOf(self.env, obj, prototype.raw_value_for_api());
            status == arkjs::JSVM_Status_JSVM_OK
        }
    }

    fn define_property(
        &self,
        key: Self,
        value: Self,
        getter: Self,
        setter: Self,
        attributes: PropertyAttributes,
    ) -> Result<(), Self> {
        unsafe {
            let has_getter = attributes.has_get();
            let has_setter = attributes.has_set();

            if has_getter || has_setter {
                // For accessor properties, use Object.defineProperty via JS
                // because JSVM_PropertyDescriptor expects JSVM_Callback (native fn ptr),
                // not JSVM_Value (JS function objects).
                let mut global: arkjs::JSVM_Value = std::ptr::null_mut();
                arkjs::OH_JSVM_GetGlobal(self.env, &mut global);

                let mut object_ctor: arkjs::JSVM_Value = std::ptr::null_mut();
                arkjs::OH_JSVM_GetNamedProperty(
                    self.env,
                    global,
                    c"Object".as_ptr() as _,
                    &mut object_ctor,
                );
                let mut define_prop_fn: arkjs::JSVM_Value = std::ptr::null_mut();
                arkjs::OH_JSVM_GetNamedProperty(
                    self.env,
                    object_ctor,
                    c"defineProperty".as_ptr() as _,
                    &mut define_prop_fn,
                );

                let mut desc_obj: arkjs::JSVM_Value = std::ptr::null_mut();
                arkjs::OH_JSVM_CreateObject(self.env, &mut desc_obj);

                if has_getter {
                    arkjs::OH_JSVM_SetNamedProperty(
                        self.env,
                        desc_obj,
                        c"get".as_ptr() as _,
                        getter.raw_value_for_api(),
                    );
                }
                if has_setter {
                    arkjs::OH_JSVM_SetNamedProperty(
                        self.env,
                        desc_obj,
                        c"set".as_ptr() as _,
                        setter.raw_value_for_api(),
                    );
                }
                if attributes.is_enumerable() {
                    let mut bool_val: arkjs::JSVM_Value = std::ptr::null_mut();
                    arkjs::OH_JSVM_GetBoolean(self.env, true, &mut bool_val);
                    arkjs::OH_JSVM_SetNamedProperty(
                        self.env,
                        desc_obj,
                        c"enumerable".as_ptr() as _,
                        bool_val,
                    );
                }
                if attributes.is_configurable() {
                    let mut bool_val: arkjs::JSVM_Value = std::ptr::null_mut();
                    arkjs::OH_JSVM_GetBoolean(self.env, true, &mut bool_val);
                    arkjs::OH_JSVM_SetNamedProperty(
                        self.env,
                        desc_obj,
                        c"configurable".as_ptr() as _,
                        bool_val,
                    );
                }

                let args = [self.resolve_handle(), key.raw_value_for_api(), desc_obj];
                let mut result: arkjs::JSVM_Value = std::ptr::null_mut();
                let status = arkjs::OH_JSVM_CallFunction(
                    self.env,
                    object_ctor,
                    define_prop_fn,
                    3,
                    args.as_ptr(),
                    &mut result,
                );
                if status == arkjs::JSVM_Status_JSVM_OK {
                    Ok(())
                } else {
                    let mut exception: arkjs::JSVM_Value = std::ptr::null_mut();
                    arkjs::OH_JSVM_GetAndClearLastException(self.env, &mut exception);
                    Err(ArkJSValue::from_owned_raw(self.env, exception)
                        .protect()
                        .with_exception())
                }
            } else {
                let descriptor = arkjs::JSVM_PropertyDescriptor {
                    utf8name: std::ptr::null(),
                    name: key.raw_value_for_api(),
                    method: std::ptr::null_mut(),
                    getter: std::ptr::null_mut(),
                    setter: std::ptr::null_mut(),
                    value: if attributes.has_value() {
                        value.raw_value_for_api()
                    } else {
                        std::ptr::null_mut()
                    },
                    attributes: to_arkjs_attributes(attributes),
                };

                let obj = self.resolve_handle();
                let status = arkjs::OH_JSVM_DefineProperties(self.env, obj, 1, &descriptor);
                if status == arkjs::JSVM_Status_JSVM_OK {
                    Ok(())
                } else {
                    let mut exception: arkjs::JSVM_Value = std::ptr::null_mut();
                    arkjs::OH_JSVM_GetAndClearLastException(self.env, &mut exception);
                    Err(ArkJSValue::from_owned_raw(self.env, exception)
                        .protect()
                        .with_exception())
                }
            }
        }
    }

    fn get_property(&self, key: Self) -> Result<Option<Self>, Self> {
        unsafe {
            let obj = self.resolve_handle();
            let mut result: arkjs::JSVM_Value = std::ptr::null_mut();
            let status =
                arkjs::OH_JSVM_GetProperty(self.env, obj, key.raw_value_for_api(), &mut result);

            if status == arkjs::JSVM_Status_JSVM_OK && !result.is_null() {
                Ok(Some(ArkJSValue::from_owned_raw(self.env, result).protect()))
            } else if status == arkjs::JSVM_Status_JSVM_OK {
                Ok(None)
            } else {
                let mut exception: arkjs::JSVM_Value = std::ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(self.env, &mut exception);
                Err(ArkJSValue::from_owned_raw(self.env, exception)
                    .protect()
                    .with_exception())
            }
        }
    }

    fn instance_of(&self, constructor: Self) -> bool {
        unsafe {
            let self_handle = self.resolve_handle();
            let ctor_handle = constructor.resolve_handle();
            let mut result = false;
            let status = arkjs::OH_JSVM_Instanceof(self.env, self_handle, ctor_handle, &mut result);
            if status == arkjs::JSVM_Status_JSVM_OK && result {
                return true;
            }
            // Fallback: check type tag for regular (non-callable) class instances
            if let Some(tag) = crate::class::get_type_tag_by_constructor(&constructor) {
                let mut matches = false;
                let status =
                    arkjs::OH_JSVM_CheckObjectTypeTag(self.env, self_handle, &tag, &mut matches);
                if status == arkjs::JSVM_Status_JSVM_OK && matches {
                    return true;
                }
            }
            // Fallback for callable instances: check if the object has native data
            // via the tagged wrapper object stored on the function.
            if crate::class::is_callable_constructor(&constructor) {
                return crate::class::callable_instance_of(self, &constructor);
            }
            false
        }
    }

    fn get_own_property_names(&self) -> Result<Vec<Self>, Self> {
        unsafe {
            let obj = self.resolve_handle();
            let mut property_names: arkjs::JSVM_Value = std::ptr::null_mut();
            let status = arkjs::OH_JSVM_GetAllPropertyNames(
                self.env,
                obj,
                arkjs::JSVM_KeyCollectionMode_JSVM_KEY_OWN_ONLY,
                arkjs::JSVM_KeyFilter_JSVM_KEY_ENUMERABLE
                    | arkjs::JSVM_KeyFilter_JSVM_KEY_SKIP_SYMBOLS,
                arkjs::JSVM_KeyConversion_JSVM_KEY_NUMBERS_TO_STRINGS,
                &mut property_names,
            );

            if status != arkjs::JSVM_Status_JSVM_OK {
                let mut exception: arkjs::JSVM_Value = std::ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(self.env, &mut exception);
                return Err(ArkJSValue::from_owned_raw(self.env, exception)
                    .protect()
                    .with_exception());
            }

            let mut length: u32 = 0;
            let status = arkjs::OH_JSVM_GetArrayLength(self.env, property_names, &mut length);

            if status != arkjs::JSVM_Status_JSVM_OK {
                let mut exception: arkjs::JSVM_Value = std::ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(self.env, &mut exception);
                return Err(ArkJSValue::from_owned_raw(self.env, exception)
                    .protect()
                    .with_exception());
            }

            let mut properties = Vec::with_capacity(length as usize);

            for i in 0..length {
                let mut element: arkjs::JSVM_Value = std::ptr::null_mut();
                let status = arkjs::OH_JSVM_GetElement(self.env, property_names, i, &mut element);

                if status == arkjs::JSVM_Status_JSVM_OK {
                    properties.push(ArkJSValue::from_owned_raw(self.env, element));
                }
            }

            Ok(properties)
        }
    }
}

fn to_arkjs_attributes(attr: PropertyAttributes) -> arkjs::JSVM_PropertyAttributes {
    let mut flags = arkjs::JSVM_PropertyAttributes_JSVM_DEFAULT;

    if attr.is_writable() {
        flags |= arkjs::JSVM_PropertyAttributes_JSVM_WRITABLE;
    }
    if attr.is_enumerable() {
        flags |= arkjs::JSVM_PropertyAttributes_JSVM_ENUMERABLE;
    }
    if attr.is_configurable() {
        flags |= arkjs::JSVM_PropertyAttributes_JSVM_CONFIGURABLE;
    }

    flags
}
