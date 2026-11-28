use crate::{JSVMValue, jsvm};
use rong_core::JSTypeOf;

impl JSTypeOf for JSVMValue {
    fn is_exception(&self) -> bool {
        // Limitation: can not verify exception as JSVMValue is from JS
        self._is_exception()
    }

    fn is_error(&self) -> bool {
        if self._is_err() {
            return true;
        }
        if self.primitive_type().is_some() {
            return false;
        }
        let _scope = crate::scope::OpScope::enter(self.env);

        let value = self.resolve_handle();
        unsafe {
            let mut result = false;
            let status = jsvm::OH_JSVM_IsError(self.env, value, &mut result);
            status == jsvm::JSVM_Status_JSVM_OK && result
        }
    }

    fn is_array(&self) -> bool {
        if self.primitive_type().is_some() {
            return false;
        }
        let _scope = crate::scope::OpScope::enter(self.env);
        if !self.is_object() {
            return false;
        }
        let value = self.resolve_handle();
        unsafe {
            let mut result = false;
            let status = jsvm::OH_JSVM_IsArray(self.env, value, &mut result);
            status == jsvm::JSVM_Status_JSVM_OK && result
        }
    }

    fn is_array_buffer(&self) -> bool {
        if self.primitive_type().is_some() {
            return false;
        }
        let _scope = crate::scope::OpScope::enter(self.env);
        if !self.is_object() {
            return false;
        }
        let value = self.resolve_handle();
        unsafe {
            let mut result = false;
            let status = jsvm::OH_JSVM_IsArraybuffer(self.env, value, &mut result);
            status == jsvm::JSVM_Status_JSVM_OK && result
        }
    }

    fn is_promise(&self) -> bool {
        if self.primitive_type().is_some() {
            return false;
        }
        let _scope = crate::scope::OpScope::enter(self.env);
        if !self.is_object() {
            return false;
        }

        let value = self.resolve_handle();
        unsafe {
            let mut result = false;
            let status = jsvm::OH_JSVM_IsPromise(self.env, value, &mut result);
            status == jsvm::JSVM_Status_JSVM_OK && result
        }
    }

    fn is_date(&self) -> bool {
        if self.primitive_type().is_some() {
            return false;
        }
        let _scope = crate::scope::OpScope::enter(self.env);
        if !self.is_object() {
            return false;
        }

        let value = self.resolve_handle();
        unsafe {
            let mut result = false;
            let status = jsvm::OH_JSVM_IsDate(self.env, value, &mut result);
            status == jsvm::JSVM_Status_JSVM_OK && result
        }
    }

    fn is_undefined(&self) -> bool {
        self.type_is(jsvm::JSVM_ValueType_JSVM_UNDEFINED)
    }

    fn is_null(&self) -> bool {
        self.type_is(jsvm::JSVM_ValueType_JSVM_NULL)
    }

    fn is_boolean(&self) -> bool {
        self.type_is(jsvm::JSVM_ValueType_JSVM_BOOLEAN)
    }

    fn is_number(&self) -> bool {
        self.type_is(jsvm::JSVM_ValueType_JSVM_NUMBER)
    }

    fn is_bigint(&self) -> bool {
        self.type_is(jsvm::JSVM_ValueType_JSVM_BIGINT)
    }

    fn is_string(&self) -> bool {
        self.type_is(jsvm::JSVM_ValueType_JSVM_STRING)
    }

    fn is_symbol(&self) -> bool {
        self.type_is(jsvm::JSVM_ValueType_JSVM_SYMBOL)
    }

    fn is_function(&self) -> bool {
        self.type_is(jsvm::JSVM_ValueType_JSVM_FUNCTION)
    }

    fn is_object(&self) -> bool {
        if self._is_object() {
            return true;
        }
        matches!(
            self.js_type(),
            Some(jsvm::JSVM_ValueType_JSVM_OBJECT | jsvm::JSVM_ValueType_JSVM_FUNCTION)
        )
    }

    fn is_constructor(&self) -> bool {
        if self.primitive_type().is_some() {
            return false;
        }
        let _scope = crate::scope::OpScope::enter(self.env);
        if !self.is_object() {
            return false;
        }

        let value = self.resolve_handle();
        unsafe {
            let mut result = false;
            let status = jsvm::OH_JSVM_IsConstructor(self.env, value, &mut result);
            status == jsvm::JSVM_Status_JSVM_OK && result
        }
    }

    fn is_proxy(&self) -> bool {
        if self.primitive_type().is_some() {
            return false;
        }
        let _scope = crate::scope::OpScope::enter(self.env);
        let value = self.resolve_handle();
        unsafe {
            let mut result = false;
            let status = jsvm::OH_JSVM_IsProxy(self.env, value, &mut result);
            status == jsvm::JSVM_Status_JSVM_OK && result
        }
    }
}

impl JSVMValue {
    /// `typeof` as JSVM reports it; a primitive kept by value answers without
    /// a JSVM call or a handle scope.
    fn js_type(&self) -> Option<jsvm::JSVM_ValueType> {
        if let Some(kind) = self.primitive_type() {
            return Some(kind);
        }
        let _scope = crate::scope::OpScope::enter(self.env);
        let value = self.resolve_handle();
        let mut value_type: jsvm::JSVM_ValueType = jsvm::JSVM_ValueType_JSVM_UNDEFINED;
        let status = unsafe { jsvm::OH_JSVM_Typeof(self.env, value, &mut value_type) };
        (status == jsvm::JSVM_Status_JSVM_OK).then_some(value_type)
    }

    fn type_is(&self, kind: jsvm::JSVM_ValueType) -> bool {
        self.js_type() == Some(kind)
    }
}
