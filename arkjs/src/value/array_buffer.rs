use crate::{ArkJSValue, arkjs};
use rong_core::{JSArrayBufferOps, JSTypeOf, JSValueImpl};
use std::ptr;
use std::slice;

impl JSArrayBufferOps for ArkJSValue {
    /// Create an ArrayBuffer by copying existing data
    fn from_bytes(ctx: &Self::Context, bytes: &[u8]) -> Self {
        let _scope = crate::scope::OpScope::enter(ctx.to_raw());
        unsafe {
            let mut buffer_data: *mut std::ffi::c_void = ptr::null_mut();
            let mut array_buffer: arkjs::JSVM_Value = ptr::null_mut();

            let status = arkjs::OH_JSVM_CreateArraybuffer(
                ctx.to_raw(),
                bytes.len(),
                &mut buffer_data,
                &mut array_buffer,
            );

            if status == arkjs::JSVM_Status_JSVM_OK {
                // Copy data into the buffer (skip for empty buffers where buffer_data may be null)
                if !bytes.is_empty() && !buffer_data.is_null() {
                    std::ptr::copy_nonoverlapping(
                        bytes.as_ptr(),
                        buffer_data as *mut u8,
                        bytes.len(),
                    );
                }
                ArkJSValue::from_owned_raw(ctx.to_raw(), array_buffer).with_object()
            } else {
                Self::create_undefined(ctx)
            }
        }
    }

    /// Create an ArrayBuffer from an existing Vec without copying (zero-copy)
    fn from_vec(ctx: &Self::Context, vec: Vec<u8>) -> Self {
        // ArkJS doesn't provide a direct zero-copy API like JavaScriptCore
        // For now, we'll copy the data using the standard API
        // In a real implementation, we might need to use external backing store
        Self::from_bytes(ctx, &vec)
    }

    /// Get the byte length of the ArrayBuffer
    fn length(&self) -> usize {
        let _scope = crate::scope::OpScope::enter(self.env);
        if !self.is_array_buffer() {
            return 0;
        }

        unsafe {
            let value = self.resolve_handle();
            let mut data: *mut std::ffi::c_void = ptr::null_mut();
            let mut byte_length: usize = 0;

            let status =
                arkjs::OH_JSVM_GetArraybufferInfo(self.env, value, &mut data, &mut byte_length);

            if status == arkjs::JSVM_Status_JSVM_OK {
                byte_length
            } else {
                0
            }
        }
    }

    /// Get a safe slice view of the ArrayBuffer's data
    fn as_slice(&self) -> &[u8] {
        let _scope = crate::scope::OpScope::enter(self.env);
        if !self.is_array_buffer() {
            return &[];
        }

        unsafe {
            let value = self.resolve_handle();
            let mut data: *mut std::ffi::c_void = ptr::null_mut();
            let mut byte_length: usize = 0;

            let status =
                arkjs::OH_JSVM_GetArraybufferInfo(self.env, value, &mut data, &mut byte_length);

            if status == arkjs::JSVM_Status_JSVM_OK && !data.is_null() {
                slice::from_raw_parts(data as *const u8, byte_length)
            } else {
                &[]
            }
        }
    }

    /// Get a mutable slice view of the ArrayBuffer's data
    fn as_mut_slice(&mut self) -> &mut [u8] {
        let _scope = crate::scope::OpScope::enter(self.env);
        if !self.is_array_buffer() {
            return &mut [];
        }

        unsafe {
            let value = self.resolve_handle();
            let mut data: *mut std::ffi::c_void = ptr::null_mut();
            let mut byte_length: usize = 0;

            let status =
                arkjs::OH_JSVM_GetArraybufferInfo(self.env, value, &mut data, &mut byte_length);

            if status == arkjs::JSVM_Status_JSVM_OK && !data.is_null() {
                slice::from_raw_parts_mut(data as *mut u8, byte_length)
            } else {
                &mut []
            }
        }
    }
}
