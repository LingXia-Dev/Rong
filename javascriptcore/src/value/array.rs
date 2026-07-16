use crate::JSCValue;
use crate::jsc;
use rong_core::{JSArrayOps, JSValueImpl};

impl JSArrayOps for JSCValue {
    fn new_array(ctx: &Self::Context) -> Self {
        unsafe {
            let mut exception: jsc::JSValueRef = std::ptr::null_mut();
            let array = jsc::JSObjectMakeArray(ctx.to_raw(), 0, std::ptr::null(), &mut exception);
            if !exception.is_null() {
                return JSCValue::from_owned_raw(ctx.to_raw(), exception).with_exception();
            }

            let key = jsc::JSStringCreateWithUTF8CString(c"length".as_ptr());
            jsc::JSObjectSetProperty(
                ctx.to_raw(),
                array,
                key,
                jsc::JSValueMakeNumber(ctx.to_raw(), 0.0),
                jsc::attr(jsc::kJSPropertyAttributeDontEnum),
                &mut exception,
            );
            jsc::JSStringRelease(key);
            if !exception.is_null() {
                JSCValue::from_owned_raw(ctx.to_raw(), exception).with_exception()
            } else {
                JSCValue::from_owned_obj(ctx.to_raw(), array)
            }
        }
    }

    fn array_len(&self) -> Self {
        unsafe {
            let mut exception: jsc::JSValueRef = std::ptr::null_mut();
            let key = jsc::JSStringCreateWithUTF8CString(c"length".as_ptr());
            let length = jsc::JSObjectGetProperty(self.ctx, self.as_obj(), key, &mut exception);
            jsc::JSStringRelease(key);

            if !exception.is_null() {
                return JSCValue::from_owned_raw(self.ctx, exception).with_exception();
            }
            if jsc::JSValueIsNumber(self.ctx, length) {
                return JSCValue::from_owned_raw(self.ctx, length);
            }
            if !jsc::JSValueIsArray(self.ctx, self.as_value()) {
                return JSCValue::from_owned_raw(self.ctx, length);
            }

            let names = jsc::JSObjectCopyPropertyNames(self.ctx, self.as_obj());
            if names.is_null() {
                return JSCValue::from_owned_raw(self.ctx, jsc::JSValueMakeNumber(self.ctx, 0.0));
            }

            let mut len = 0u32;
            let count = jsc::JSPropertyNameArrayGetCount(names);
            for i in 0..count {
                let name = jsc::JSPropertyNameArrayGetNameAtIndex(names, i);
                let max_size = jsc::JSStringGetMaximumUTF8CStringSize(name);
                let mut buffer = vec![0u8; max_size];
                let actual_size = jsc::JSStringGetUTF8CString(
                    name,
                    buffer.as_mut_ptr().cast::<std::os::raw::c_char>(),
                    max_size,
                );
                if actual_size > 1 {
                    buffer.truncate(actual_size - 1);
                    if let Ok(s) = std::str::from_utf8(&buffer)
                        && let Ok(index) = s.parse::<u32>()
                        && index != u32::MAX
                    {
                        len = len.max(index + 1);
                    }
                }
            }
            jsc::JSPropertyNameArrayRelease(names);

            JSCValue::from_owned_raw(self.ctx, jsc::JSValueMakeNumber(self.ctx, len as f64))
        }
    }

    fn get_index(&self, index: u32) -> Self {
        unsafe {
            let mut exception: jsc::JSValueRef = std::ptr::null_mut();
            let value =
                jsc::JSObjectGetPropertyAtIndex(self.ctx, self.as_obj(), index, &mut exception);
            if !exception.is_null() {
                JSCValue::from_owned_raw(self.ctx, exception).with_exception()
            } else {
                JSCValue::from_owned_raw(self.ctx, value)
            }
        }
    }

    fn set_index(&self, index: u32, value: Self) -> Self {
        unsafe {
            self.reflect_set(
                jsc::JSValueMakeNumber(self.ctx, index as f64),
                value.as_value(),
            )
        }
    }
}
