use crate::{ArkJSValue, arkjs};
use rong_core::JSValueImpl;
use rong_core::engine::JSProxyOps;

impl JSProxyOps for ArkJSValue {
    fn new_proxy(ctx: &Self::Context, target: Self, handler: Self) -> Result<Self, Self> {
        let _scope = crate::scope::OpScope::enter(ctx.to_raw());
        unsafe {
            let env = ctx.to_raw();
            let mut proxy: arkjs::JSVM_Value = std::ptr::null_mut();
            let status = arkjs::OH_JSVM_CreateProxy(
                env,
                target.raw_value_for_api(),
                handler.raw_value_for_api(),
                &mut proxy,
            );
            if status == arkjs::JSVM_Status_JSVM_OK {
                Ok(ArkJSValue::from_owned_raw(env, proxy))
            } else {
                let mut exception: arkjs::JSVM_Value = std::ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(env, &mut exception);
                Err(ArkJSValue::from_owned_raw(env, exception).with_exception())
            }
        }
    }

    fn proxy_target(&self) -> Result<Self, Self> {
        let _scope = crate::scope::OpScope::enter(self.env);
        unsafe {
            let mut target: arkjs::JSVM_Value = std::ptr::null_mut();
            let status =
                arkjs::OH_JSVM_ProxyGetTarget(self.env, self.raw_value_for_api(), &mut target);
            if status == arkjs::JSVM_Status_JSVM_OK {
                Ok(ArkJSValue::from_owned_raw(self.env, target))
            } else {
                let mut exception: arkjs::JSVM_Value = std::ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(self.env, &mut exception);
                Err(ArkJSValue::from_owned_raw(self.env, exception).with_exception())
            }
        }
    }
}
