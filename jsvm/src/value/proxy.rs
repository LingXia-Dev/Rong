use crate::{JSVMValue, jsvm};
use rong_core::JSValueImpl;
use rong_core::engine::JSProxyOps;

impl JSProxyOps for JSVMValue {
    fn new_proxy(ctx: &Self::Context, target: Self, handler: Self) -> Result<Self, Self> {
        let _scope = crate::scope::OpScope::enter(ctx.to_raw());
        unsafe {
            let env = ctx.to_raw();
            let mut proxy: jsvm::JSVM_Value = std::ptr::null_mut();
            let status = jsvm::OH_JSVM_CreateProxy(
                env,
                target.raw_value_for_api(),
                handler.raw_value_for_api(),
                &mut proxy,
            );
            if status == jsvm::JSVM_Status_JSVM_OK {
                Ok(JSVMValue::from_owned_raw(env, proxy))
            } else {
                let mut exception: jsvm::JSVM_Value = std::ptr::null_mut();
                jsvm::OH_JSVM_GetAndClearLastException(env, &mut exception);
                Err(JSVMValue::from_owned_raw(env, exception).with_exception())
            }
        }
    }

    fn proxy_target(&self) -> Result<Self, Self> {
        let _scope = crate::scope::OpScope::enter(self.env);
        unsafe {
            let mut target: jsvm::JSVM_Value = std::ptr::null_mut();
            let status =
                jsvm::OH_JSVM_ProxyGetTarget(self.env, self.raw_value_for_api(), &mut target);
            if status == jsvm::JSVM_Status_JSVM_OK {
                Ok(JSVMValue::from_owned_raw(self.env, target))
            } else {
                let mut exception: jsvm::JSVM_Value = std::ptr::null_mut();
                jsvm::OH_JSVM_GetAndClearLastException(self.env, &mut exception);
                Err(JSVMValue::from_owned_raw(self.env, exception).with_exception())
            }
        }
    }
}
