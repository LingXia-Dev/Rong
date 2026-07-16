use crate::{JSCValue, jsc};
use rong_core::engine::JSObjectOps;
use rong_core::{JSValueImpl, PropertyAttributes};

impl JSObjectOps for JSCValue {
    fn new_object(ctx: &Self::Context) -> Self {
        unsafe {
            let obj = jsc::JSObjectMake(ctx.to_raw(), std::ptr::null_mut(), std::ptr::null_mut());
            JSCValue::from_owned_obj(ctx.to_raw(), obj)
        }
    }

    fn make_instance(ctx: &Self::Context, constructor: Self, data: *mut ()) -> Self {
        let obj = unsafe {
            let classref = crate::class::get_classref_by_constructor(constructor);
            jsc::JSObjectMake(ctx.to_raw(), classref as jsc::JSClassRef, data as _)
        };

        JSCValue::from_owned_obj(ctx.to_raw(), obj)
    }

    fn get_opaque(&self) -> *mut () {
        unsafe {
            let private_data = jsc::JSObjectGetPrivate(self.as_obj());
            private_data as _
        }
    }

    fn del_property(&self, key: Self) -> Result<bool, Self> {
        let mut exception: jsc::JSValueRef = std::ptr::null_mut();

        unsafe {
            if jsc::JSValueIsString(self.ctx, key.as_value()) {
                let key = jsc::JSValueToStringCopy(self.ctx, key.as_value(), &mut exception);
                if !exception.is_null() {
                    return Err(JSCValue::from_owned_raw(self.ctx, exception).with_exception());
                }
                let result =
                    jsc::JSObjectDeleteProperty(self.ctx, self.as_obj(), key, &mut exception);
                jsc::JSStringRelease(key);
                return if exception.is_null() {
                    Ok(result)
                } else {
                    Err(JSCValue::from_owned_raw(self.ctx, exception).with_exception())
                };
            }

            let result = jsc::JSObjectDeletePropertyForKey(
                self.ctx,
                self.as_obj(),
                key.as_value(),
                &mut exception,
            );
            if exception.is_null() {
                Ok(result)
            } else {
                Err(JSCValue::from_owned_raw(self.ctx, exception).with_exception())
            }
        }
    }

    fn has_property(&self, key: Self) -> Result<bool, Self> {
        let mut exception: jsc::JSValueRef = std::ptr::null_mut();

        unsafe {
            if jsc::JSValueIsString(self.ctx, key.as_value()) {
                let key = jsc::JSValueToStringCopy(self.ctx, key.as_value(), &mut exception);
                if !exception.is_null() {
                    return Err(JSCValue::from_owned_raw(self.ctx, exception).with_exception());
                }
                let result = jsc::JSObjectHasProperty(self.ctx, self.as_obj(), key);
                jsc::JSStringRelease(key);
                return Ok(result);
            }

            let result = jsc::JSObjectHasPropertyForKey(
                self.ctx,
                self.as_obj(),
                key.as_value(),
                &mut exception,
            );
            if exception.is_null() {
                Ok(result)
            } else {
                Err(JSCValue::from_owned_raw(self.ctx, exception).with_exception())
            }
        }
    }

    fn set_property(&self, key: Self, value: Self) -> Result<(), Self> {
        let result = self.reflect_set(key.as_value(), value.as_value());
        if result._is_exception() {
            Err(result)
        } else {
            Ok(())
        }
    }

    fn set_prototype(&self, prototype: Self) -> bool {
        unsafe {
            let obj = self.as_obj();
            jsc::JSObjectSetPrototype(self.ctx, obj, prototype.as_value());
            let actual = jsc::JSObjectGetPrototype(self.ctx, obj);
            jsc::JSValueIsStrictEqual(self.ctx, actual, prototype.as_value())
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
        let mut exception: jsc::JSValueRef = std::ptr::null_mut();

        // Get the Object constructor
        let object_ctor = crate::class::get_constructor(self.ctx, c"Object".as_ptr());

        unsafe {
            // Create property descriptor
            let descriptor =
                jsc::JSObjectMake(self.ctx, std::ptr::null_mut(), std::ptr::null_mut());

            if attributes.has_value() {
                let value_str = jsc::JSStringCreateWithUTF8CString(c"value".as_ptr());
                jsc::JSObjectSetProperty(
                    self.ctx,
                    descriptor,
                    value_str,
                    value.as_value(),
                    jsc::attr(jsc::kJSPropertyAttributeNone),
                    &mut exception,
                );
                jsc::JSStringRelease(value_str);
                if !exception.is_null() {
                    return Err(JSCValue::from_owned_raw(self.ctx, exception).with_exception());
                }
            }

            if attributes.has_get() {
                let get_str = jsc::JSStringCreateWithUTF8CString(c"get".as_ptr());
                jsc::JSObjectSetProperty(
                    self.ctx,
                    descriptor,
                    get_str,
                    getter.as_value(),
                    jsc::attr(jsc::kJSPropertyAttributeNone),
                    &mut exception,
                );
                jsc::JSStringRelease(get_str);
                if !exception.is_null() {
                    return Err(JSCValue::from_owned_raw(self.ctx, exception).with_exception());
                }
            }

            if attributes.has_set() {
                let set_str = jsc::JSStringCreateWithUTF8CString(c"set".as_ptr());
                jsc::JSObjectSetProperty(
                    self.ctx,
                    descriptor,
                    set_str,
                    setter.as_value(),
                    jsc::attr(jsc::kJSPropertyAttributeNone),
                    &mut exception,
                );
                jsc::JSStringRelease(set_str);
                if !exception.is_null() {
                    return Err(JSCValue::from_owned_raw(self.ctx, exception).with_exception());
                }
            }

            // Add enumerable/configurable/writable flags
            let flags = to_jsc_attributes(attributes);
            if attributes.has_enumerable() {
                let enumerable_str = jsc::JSStringCreateWithUTF8CString(c"enumerable".as_ptr());
                jsc::JSObjectSetProperty(
                    self.ctx,
                    descriptor,
                    enumerable_str,
                    jsc::JSValueMakeBoolean(
                        self.ctx,
                        (flags & jsc::attr(jsc::kJSPropertyAttributeDontEnum)) == 0,
                    ),
                    jsc::attr(jsc::kJSPropertyAttributeNone),
                    &mut exception,
                );
                jsc::JSStringRelease(enumerable_str);
                if !exception.is_null() {
                    return Err(JSCValue::from_owned_raw(self.ctx, exception).with_exception());
                }
            }

            if attributes.has_configurable() {
                let configurable_str = jsc::JSStringCreateWithUTF8CString(c"configurable".as_ptr());
                jsc::JSObjectSetProperty(
                    self.ctx,
                    descriptor,
                    configurable_str,
                    jsc::JSValueMakeBoolean(
                        self.ctx,
                        (flags & jsc::attr(jsc::kJSPropertyAttributeDontDelete)) == 0,
                    ),
                    jsc::attr(jsc::kJSPropertyAttributeNone),
                    &mut exception,
                );
                jsc::JSStringRelease(configurable_str);
                if !exception.is_null() {
                    return Err(JSCValue::from_owned_raw(self.ctx, exception).with_exception());
                }
            }

            if attributes.has_value() && attributes.has_writable() {
                let writable_str = jsc::JSStringCreateWithUTF8CString(c"writable".as_ptr());
                jsc::JSObjectSetProperty(
                    self.ctx,
                    descriptor,
                    writable_str,
                    jsc::JSValueMakeBoolean(
                        self.ctx,
                        (flags & jsc::attr(jsc::kJSPropertyAttributeReadOnly)) == 0,
                    ),
                    jsc::attr(jsc::kJSPropertyAttributeNone),
                    &mut exception,
                );
                jsc::JSStringRelease(writable_str);
                if !exception.is_null() {
                    return Err(JSCValue::from_owned_raw(self.ctx, exception).with_exception());
                }
            }

            // Get defineProperty function
            let define_prop_str = jsc::JSStringCreateWithUTF8CString(c"defineProperty".as_ptr());
            let define_prop_fn =
                jsc::JSObjectGetProperty(self.ctx, object_ctor, define_prop_str, &mut exception);
            jsc::JSStringRelease(define_prop_str);

            if !exception.is_null() || jsc::JSValueIsUndefined(self.ctx, define_prop_fn) {
                return Err(JSCValue::from_owned_raw(self.ctx, exception).with_exception());
            }

            // Call defineProperty
            let args = [self.as_value(), key.as_value(), descriptor];

            jsc::JSObjectCallAsFunction(
                self.ctx,
                define_prop_fn as jsc::JSObjectRef,
                std::ptr::null_mut(),
                args.len(),
                args.as_ptr(),
                &mut exception,
            );

            if exception.is_null() {
                Ok(())
            } else {
                Err(JSCValue::from_owned_raw(self.ctx, exception).with_exception())
            }
        }
    }

    fn get_property(&self, key: Self) -> Result<Option<Self>, Self> {
        let mut exception: jsc::JSValueRef = std::ptr::null_mut();

        unsafe {
            if jsc::JSValueIsString(self.ctx, key.as_value()) {
                let key = jsc::JSValueToStringCopy(self.ctx, key.as_value(), &mut exception);
                if !exception.is_null() {
                    return Err(JSCValue::from_owned_raw(self.ctx, exception).with_exception());
                }
                let value = jsc::JSObjectGetProperty(self.ctx, self.as_obj(), key, &mut exception);
                jsc::JSStringRelease(key);
                return if exception.is_null() {
                    Ok(Some(JSCValue::from_owned_raw(self.ctx, value)))
                } else {
                    Err(JSCValue::from_owned_raw(self.ctx, exception).with_exception())
                };
            }

            let value = jsc::JSObjectGetPropertyForKey(
                self.ctx,
                self.as_obj(),
                key.as_value(),
                &mut exception,
            );
            if !exception.is_null() {
                Err(JSCValue::from_owned_raw(self.ctx, exception).with_exception())
            } else {
                Ok(Some(JSCValue::from_owned_raw(self.ctx, value)))
            }
        }
    }

    fn instance_of(&self, constructor: Self) -> bool {
        let mut exception: jsc::JSValueRef = std::ptr::null_mut();
        unsafe {
            let result = jsc::JSValueIsInstanceOfConstructor(
                self.ctx,
                self.as_value(),
                constructor.as_obj(),
                &mut exception,
            );
            exception.is_null() && result
        }
    }

    fn get_own_property_names(&self) -> Result<Vec<Self>, Self> {
        unsafe {
            let mut exception: jsc::JSValueRef = std::ptr::null_mut();

            // Get the property names array
            let names = jsc::JSObjectCopyPropertyNames(self.ctx, self.as_obj());
            if names.is_null() {
                return Ok(Vec::new());
            }

            let count = jsc::JSPropertyNameArrayGetCount(names);
            let mut properties = Vec::with_capacity(count as usize);

            // Get the Object constructor
            let object_ctor = crate::class::get_constructor(self.ctx, c"Object".as_ptr());

            // Get the prototype of the Object constructor
            let prototype = jsc::JSObjectGetPrototype(self.ctx, object_ctor);
            let prototype = jsc::JSValueToObject(self.ctx, prototype, &mut exception);

            // Get the hasOwnProperty function
            let has_own_str = jsc::JSStringCreateWithUTF8CString(c"hasOwnProperty".as_ptr());
            let has_own =
                jsc::JSObjectGetProperty(self.ctx, prototype as _, has_own_str, &mut exception);
            jsc::JSStringRelease(has_own_str);

            if !exception.is_null() {
                return Err(JSCValue::from_owned_raw(self.ctx, exception).with_exception());
            }

            // Collect all property names
            for i in 0..count {
                let name = jsc::JSPropertyNameArrayGetNameAtIndex(names, i);
                let value = jsc::JSValueMakeString(self.ctx, name);

                // Call hasOwnProperty on the current object
                let args = [value];
                let result = jsc::JSObjectCallAsFunction(
                    self.ctx,
                    has_own as jsc::JSObjectRef,
                    self.as_obj(),
                    args.len(),
                    args.as_ptr(),
                    &mut exception,
                );

                if !exception.is_null() {
                    jsc::JSPropertyNameArrayRelease(names);
                    return Err(JSCValue::from_owned_raw(self.ctx, exception).with_exception());
                }

                let is_own = jsc::JSValueToBoolean(self.ctx, result);

                if is_own {
                    // Property names are borrowed from JSC; protect them because we store
                    // and use them after this call (e.g. JSObject::entries).
                    properties.push(JSCValue::from_borrowed_raw(self.ctx, value));
                }
            }

            jsc::JSPropertyNameArrayRelease(names);
            Ok(properties)
        }
    }
}

fn to_jsc_attributes(attr: PropertyAttributes) -> u32 {
    let mut flags = jsc::attr(jsc::kJSPropertyAttributeNone);

    if !attr.is_writable() {
        flags |= jsc::attr(jsc::kJSPropertyAttributeReadOnly);
    }
    if !attr.is_enumerable() {
        flags |= jsc::attr(jsc::kJSPropertyAttributeDontEnum)
    }
    if !attr.is_configurable() {
        flags |= jsc::attr(jsc::kJSPropertyAttributeDontDelete)
    }

    flags
}
