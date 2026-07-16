use crate::QJSValue;
use crate::qjs;
use rong_core::engine::JSObjectOps;
use rong_core::{JSValueImpl, PropertyAttributes};
use std::mem::MaybeUninit;

impl JSObjectOps for QJSValue {
    fn new_object(ctx: &Self::Context) -> Self {
        let ctx = ctx.to_raw();
        let v = unsafe { qjs::JS_NewObject(ctx) };
        QJSValue::from_owned_raw(ctx, v)
    }

    fn make_instance(ctx: &Self::Context, constructor: Self, data: *mut ()) -> Self {
        let ctx = ctx.to_raw();
        let constructor = constructor.value;
        let v = unsafe { qjs::QJS_ObjectMake(ctx, constructor, data.cast()) };
        QJSValue::from_owned_raw(ctx, v)
    }

    fn get_opaque(&self) -> *mut () {
        unsafe { qjs::QJS_ObjectGetPrivate(self.value) as _ }
    }

    fn del_property(&self, key: Self) -> Result<bool, Self> {
        let v = unsafe {
            let atom = qjs::JS_ValueToAtom(self.ctx, key.value);
            let v = qjs::JS_DeleteProperty(
                self.ctx, self.value, atom, 0, // flags is 0, OK ?
            );
            qjs::JS_FreeAtom(self.ctx, atom);
            v
        };

        if v < 0 {
            Err(
                QJSValue::from_owned_raw(self.ctx, unsafe { qjs::JS_GetException(self.ctx) })
                    .with_exception(),
            )
        } else {
            Ok(v != 0)
        }
    }

    fn has_property(&self, key: Self) -> Result<bool, Self> {
        let v = unsafe {
            let atom = qjs::JS_ValueToAtom(self.ctx, key.value);
            let v = qjs::JS_HasProperty(self.ctx, self.value, atom);
            qjs::JS_FreeAtom(self.ctx, atom);
            v
        };

        if v < 0 {
            Err(
                QJSValue::from_owned_raw(self.ctx, unsafe { qjs::JS_GetException(self.ctx) })
                    .with_exception(),
            )
        } else {
            Ok(v != 0)
        }
    }

    fn set_property(&self, key: Self, value: Self) -> Result<(), Self> {
        let kv = value.into_raw_value(); //necessary
        let v = unsafe {
            let atom = qjs::JS_ValueToAtom(self.ctx, key.value);
            let v = qjs::JS_SetProperty(self.ctx, self.value, atom, kv);
            qjs::JS_FreeAtom(self.ctx, atom);
            v
        };
        if v < 0 {
            Err(
                QJSValue::from_owned_raw(self.ctx, unsafe { qjs::JS_GetException(self.ctx) })
                    .with_exception(),
            )
        } else {
            Ok(())
        }
    }

    fn set_prototype(&self, prototype: Self) -> bool {
        let p = prototype.value;

        // JS_SetPrototype clone input prototype
        let v = unsafe { qjs::JS_SetPrototype(self.ctx, self.value, p) };
        if v < 0 {
            unsafe {
                let exception = qjs::JS_GetException(self.ctx);
                qjs::JS_FreeValue(self.ctx, exception);
            }
            false
        } else {
            v != 0
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
        let getter = getter.value;
        let setter = setter.value;
        let value = value.value;

        let v = unsafe {
            let atom = qjs::JS_ValueToAtom(self.ctx, key.value);
            // JS_DefineProperty clone value,getter,setter
            let v = qjs::JS_DefineProperty(
                self.ctx,
                self.value,
                atom,
                value,
                getter,
                setter,
                to_flags(attributes),
            );
            qjs::JS_FreeAtom(self.ctx, atom);
            v
        };
        if v < 0 {
            Err(
                QJSValue::from_owned_raw(self.ctx, unsafe { qjs::JS_GetException(self.ctx) })
                    .with_exception(),
            )
        } else {
            Ok(())
        }
    }

    fn get_property(&self, key: Self) -> Result<Option<Self>, Self> {
        let v = unsafe {
            let atom = qjs::JS_ValueToAtom(self.ctx, key.value);
            let v = qjs::JS_GetProperty(self.ctx, self.value, atom);
            qjs::JS_FreeAtom(self.ctx, atom);
            v
        };
        if unsafe { qjs::QJS_IsException(self.ctx, v) } {
            Err(
                QJSValue::from_owned_raw(self.ctx, unsafe { qjs::JS_GetException(self.ctx) })
                    .with_exception(),
            )
        } else {
            Ok(Some(QJSValue::from_owned_raw(self.ctx, v)))
        }
    }

    fn instance_of(&self, constructor: Self) -> bool {
        let constructor = constructor.value;
        let v = unsafe { qjs::JS_IsInstanceOf(self.ctx, self.value, constructor) };
        v != 0
    }

    fn get_own_property_names(&self) -> Result<Vec<Self>, Self> {
        unsafe {
            let ctx = self.ctx;
            let mut properties = Vec::new();

            let mut enums = MaybeUninit::uninit();
            let mut count = MaybeUninit::uninit();

            // Get property names
            let ret = qjs::JS_GetOwnPropertyNames(
                ctx,
                enums.as_mut_ptr(),
                count.as_mut_ptr(),
                self.value,
                qjs::JS_GPN_STRING_MASK as i32
                    | qjs::JS_GPN_ENUM_ONLY as i32
                    | qjs::JS_GPN_SET_ENUM as i32,
            );

            if ret != 0 {
                return Err(
                    QJSValue::from_owned_raw(ctx, qjs::JS_GetException(ctx)).with_exception()
                );
            }

            let enums = enums.assume_init();
            let count = count.assume_init();

            for i in 0..count {
                let atom = *enums.add(i as usize);
                let prop = qjs::JS_AtomToString(ctx, atom.atom);
                if !qjs::QJS_IsException(ctx, prop) {
                    properties.push(QJSValue::from_owned_raw(ctx, prop));
                }
            }

            qjs::JS_FreePropertyEnum(ctx, enums, count);

            Ok(properties)
        }
    }
}

fn to_flags(attr: PropertyAttributes) -> i32 {
    let mut flags = 0;

    if attr.has_value() {
        flags |= qjs::JS_PROP_HAS_VALUE;
    }

    if attr.has_get() {
        flags |= qjs::JS_PROP_HAS_GET;
    }
    if attr.has_set() {
        flags |= qjs::JS_PROP_HAS_SET;
    }

    if attr.has_writable() {
        flags |= qjs::JS_PROP_HAS_WRITABLE;
        if attr.is_writable() {
            flags |= qjs::JS_PROP_WRITABLE;
        }
    }

    if attr.has_enumerable() {
        flags |= qjs::JS_PROP_HAS_ENUMERABLE;
        if attr.is_enumerable() {
            flags |= qjs::JS_PROP_ENUMERABLE;
        }
    }

    if attr.has_configurable() {
        flags |= qjs::JS_PROP_HAS_CONFIGURABLE;
        if attr.is_configurable() {
            flags |= qjs::JS_PROP_CONFIGURABLE;
        }
    }
    flags as _
}
