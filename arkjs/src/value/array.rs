use crate::ArkJSValue;
use crate::arkjs;
use rong_core::{JSArrayOps, JSValueImpl};

impl JSArrayOps for ArkJSValue {
    fn new_array(ctx: &Self::Context) -> Self {
        unsafe {
            let mut array: arkjs::JSVM_Value = std::ptr::null_mut();
            let status = arkjs::OH_JSVM_CreateArray(ctx.to_raw(), &mut array);
            if status == arkjs::JSVM_Status_JSVM_OK {
                ArkJSValue::from_owned_raw(ctx.to_raw(), array).with_object()
            } else {
                Self::create_undefined(ctx)
            }
        }
    }

    fn array_len(&self) -> Self {
        unsafe {
            let mut length = 0u32;
            let status =
                arkjs::OH_JSVM_GetArrayLength(self.env, self.resolve_handle(), &mut length);
            if status != arkjs::JSVM_Status_JSVM_OK {
                let mut exception: arkjs::JSVM_Value = std::ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(self.env, &mut exception);
                return ArkJSValue::from_owned_raw(self.env, exception)
                    .protect()
                    .with_exception();
            }

            let mut value: arkjs::JSVM_Value = std::ptr::null_mut();
            let status = arkjs::OH_JSVM_CreateUint32(self.env, length, &mut value);
            if status == arkjs::JSVM_Status_JSVM_OK {
                ArkJSValue::from_owned_raw(self.env, value)
            } else {
                // Surface the pending exception (consistent with get_index /
                // set_index) instead of returning undefined, which would mask
                // the OOM/internal-error root cause as a type mismatch.
                let mut exception: arkjs::JSVM_Value = std::ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(self.env, &mut exception);
                ArkJSValue::from_owned_raw(self.env, exception)
                    .protect()
                    .with_exception()
            }
        }
    }

    fn get_index(&self, index: u32) -> Self {
        unsafe {
            let mut result: arkjs::JSVM_Value = std::ptr::null_mut();
            let status =
                arkjs::OH_JSVM_GetElement(self.env, self.resolve_handle(), index, &mut result);
            if status == arkjs::JSVM_Status_JSVM_OK {
                ArkJSValue::from_owned_raw(self.env, result)
            } else {
                // Return exception
                let mut exception: arkjs::JSVM_Value = std::ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(self.env, &mut exception);
                ArkJSValue::from_owned_raw(self.env, exception)
                    .protect()
                    .with_exception()
            }
        }
    }

    fn set_index(&self, index: u32, value: Self) -> Self {
        unsafe {
            let mut key: arkjs::JSVM_Value = std::ptr::null_mut();
            let status = arkjs::OH_JSVM_CreateUint32(self.env, index, &mut key);
            if status != arkjs::JSVM_Status_JSVM_OK {
                let mut exception: arkjs::JSVM_Value = std::ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(self.env, &mut exception);
                return ArkJSValue::from_owned_raw(self.env, exception)
                    .protect()
                    .with_exception();
            }

            self.reflect_set(key, value.resolve_handle())
        }
    }
}
