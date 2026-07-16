use crate::QJSValue;
use crate::qjs;
use rong_core::{JSArrayOps, JSValueImpl};

impl JSArrayOps for QJSValue {
    fn new_array(ctx: &Self::Context) -> Self {
        let ctx = ctx.to_raw();
        unsafe {
            let v = qjs::JS_NewArray(ctx);

            if qjs::QJS_IsException(ctx, v) {
                QJSValue::from_owned_raw(ctx, v).with_exception()
            } else {
                QJSValue::from_owned_raw(ctx, v)
            }
        }
    }

    fn array_len(&self) -> Self {
        let ctx = self.ctx;
        unsafe {
            let v = qjs::JS_GetPropertyStr(ctx, self.value, c"length".as_ptr());

            if qjs::QJS_IsException(ctx, v) {
                QJSValue::from_owned_raw(ctx, v).with_exception()
            } else {
                QJSValue::from_owned_raw(ctx, v)
            }
        }
    }

    fn get_index(&self, index: u32) -> Self {
        let ctx = self.ctx;
        unsafe {
            let v = qjs::JS_GetPropertyUint32(ctx, self.value, index);

            if qjs::QJS_IsException(ctx, v) {
                QJSValue::from_owned_raw(ctx, v).with_exception()
            } else {
                QJSValue::from_owned_raw(ctx, v)
            }
        }
    }

    fn set_index(&self, index: u32, value: Self) -> Self {
        let ctx = self.ctx;
        unsafe {
            let status = qjs::JS_SetPropertyUint32(ctx, self.value, index, value.into_raw_value());
            if status >= 0 {
                let raw = qjs::QJS_NewUndefined(ctx);
                QJSValue::from_owned_raw(ctx, raw)
            } else {
                let exception = qjs::JS_GetException(ctx);
                QJSValue::from_owned_raw(ctx, exception).with_exception()
            }
        }
    }
}
