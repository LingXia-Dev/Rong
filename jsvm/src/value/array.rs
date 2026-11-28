use crate::JSVMValue;
use crate::jsvm;
use rong_core::{JSArrayOps, JSValueImpl};

impl JSArrayOps for JSVMValue {
    fn new_array(ctx: &Self::Context) -> Self {
        let _scope = crate::scope::OpScope::enter(ctx.to_raw());
        unsafe {
            let mut array: jsvm::JSVM_Value = std::ptr::null_mut();
            let status = jsvm::OH_JSVM_CreateArray(ctx.to_raw(), &mut array);
            if status == jsvm::JSVM_Status_JSVM_OK {
                JSVMValue::from_owned_raw(ctx.to_raw(), array).with_object()
            } else {
                Self::create_undefined(ctx)
            }
        }
    }

    fn array_len(&self) -> Self {
        let _scope = crate::scope::OpScope::enter(self.env);
        unsafe {
            let mut length = 0u32;
            let status = jsvm::OH_JSVM_GetArrayLength(self.env, self.resolve_handle(), &mut length);
            if status != jsvm::JSVM_Status_JSVM_OK {
                let mut exception: jsvm::JSVM_Value = std::ptr::null_mut();
                jsvm::OH_JSVM_GetAndClearLastException(self.env, &mut exception);
                return JSVMValue::from_owned_raw(self.env, exception).with_exception();
            }

            let mut value: jsvm::JSVM_Value = std::ptr::null_mut();
            let status = jsvm::OH_JSVM_CreateUint32(self.env, length, &mut value);
            if status == jsvm::JSVM_Status_JSVM_OK {
                JSVMValue::from_owned_raw(self.env, value)
            } else {
                // Surface the pending exception (consistent with get_index /
                // set_index) instead of returning undefined, which would mask
                // the OOM/internal-error root cause as a type mismatch.
                let mut exception: jsvm::JSVM_Value = std::ptr::null_mut();
                jsvm::OH_JSVM_GetAndClearLastException(self.env, &mut exception);
                JSVMValue::from_owned_raw(self.env, exception).with_exception()
            }
        }
    }

    fn get_index(&self, index: u32) -> Self {
        let _scope = crate::scope::OpScope::enter(self.env);
        unsafe {
            let mut result: jsvm::JSVM_Value = std::ptr::null_mut();
            let status =
                jsvm::OH_JSVM_GetElement(self.env, self.resolve_handle(), index, &mut result);
            if status == jsvm::JSVM_Status_JSVM_OK {
                JSVMValue::from_owned_raw(self.env, result)
            } else {
                // Return exception
                let mut exception: jsvm::JSVM_Value = std::ptr::null_mut();
                jsvm::OH_JSVM_GetAndClearLastException(self.env, &mut exception);
                JSVMValue::from_owned_raw(self.env, exception).with_exception()
            }
        }
    }

    fn set_index(&self, index: u32, value: Self) -> Self {
        let _scope = crate::scope::OpScope::enter(self.env);
        unsafe {
            let mut key: jsvm::JSVM_Value = std::ptr::null_mut();
            let status = jsvm::OH_JSVM_CreateUint32(self.env, index, &mut key);
            if status != jsvm::JSVM_Status_JSVM_OK {
                let mut exception: jsvm::JSVM_Value = std::ptr::null_mut();
                jsvm::OH_JSVM_GetAndClearLastException(self.env, &mut exception);
                return JSVMValue::from_owned_raw(self.env, exception).with_exception();
            }

            self.reflect_set(key, value.resolve_handle())
        }
    }
}
