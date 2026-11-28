use crate::{JSVMValue, jsvm};
use rong_core::{JSTypedArrayKind, JSTypedArrayOps, JSValueImpl};
use std::ptr;

/// Raw info returned by OH_JSVM_GetTypedarrayInfo.
struct TypedArrayInfo {
    kind: jsvm::JSVM_TypedarrayType,
    length: usize,
    byte_offset: usize,
    array_buffer: jsvm::JSVM_Value,
}

impl JSVMValue {
    /// Single JSVM call that retrieves all typed-array metadata at once.
    fn typed_array_info(&self) -> Option<TypedArrayInfo> {
        unsafe {
            let mut kind: jsvm::JSVM_TypedarrayType = jsvm::JSVM_TypedarrayType_JSVM_INT8_ARRAY;
            let mut length: usize = 0;
            let mut data: *mut std::ffi::c_void = ptr::null_mut();
            let mut array_buffer: jsvm::JSVM_Value = ptr::null_mut();
            let mut byte_offset: usize = 0;

            let status = jsvm::OH_JSVM_GetTypedarrayInfo(
                self.env,
                self.resolve_handle(),
                &mut kind,
                &mut length,
                &mut data,
                &mut array_buffer,
                &mut byte_offset,
            );

            if status == jsvm::JSVM_Status_JSVM_OK {
                Some(TypedArrayInfo {
                    kind,
                    length,
                    byte_offset,
                    array_buffer,
                })
            } else {
                None
            }
        }
    }
}

fn ark_kind_to_kind(ark: jsvm::JSVM_TypedarrayType) -> Option<JSTypedArrayKind> {
    match ark {
        jsvm::JSVM_TypedarrayType_JSVM_INT8_ARRAY => Some(JSTypedArrayKind::Int8),
        jsvm::JSVM_TypedarrayType_JSVM_UINT8_ARRAY => Some(JSTypedArrayKind::Uint8),
        jsvm::JSVM_TypedarrayType_JSVM_UINT8_CLAMPED_ARRAY => Some(JSTypedArrayKind::Uint8Clamped),
        jsvm::JSVM_TypedarrayType_JSVM_INT16_ARRAY => Some(JSTypedArrayKind::Int16),
        jsvm::JSVM_TypedarrayType_JSVM_UINT16_ARRAY => Some(JSTypedArrayKind::Uint16),
        jsvm::JSVM_TypedarrayType_JSVM_INT32_ARRAY => Some(JSTypedArrayKind::Int32),
        jsvm::JSVM_TypedarrayType_JSVM_UINT32_ARRAY => Some(JSTypedArrayKind::Uint32),
        jsvm::JSVM_TypedarrayType_JSVM_FLOAT32_ARRAY => Some(JSTypedArrayKind::Float32),
        jsvm::JSVM_TypedarrayType_JSVM_FLOAT64_ARRAY => Some(JSTypedArrayKind::Float64),
        jsvm::JSVM_TypedarrayType_JSVM_BIGINT64_ARRAY => Some(JSTypedArrayKind::BigInt64),
        jsvm::JSVM_TypedarrayType_JSVM_BIGUINT64_ARRAY => Some(JSTypedArrayKind::BigUint64),
        _ => None,
    }
}

fn kind_to_ark_kind(kind: JSTypedArrayKind) -> jsvm::JSVM_TypedarrayType {
    match kind {
        JSTypedArrayKind::Int8 => jsvm::JSVM_TypedarrayType_JSVM_INT8_ARRAY,
        JSTypedArrayKind::Uint8 => jsvm::JSVM_TypedarrayType_JSVM_UINT8_ARRAY,
        JSTypedArrayKind::Uint8Clamped => jsvm::JSVM_TypedarrayType_JSVM_UINT8_CLAMPED_ARRAY,
        JSTypedArrayKind::Int16 => jsvm::JSVM_TypedarrayType_JSVM_INT16_ARRAY,
        JSTypedArrayKind::Uint16 => jsvm::JSVM_TypedarrayType_JSVM_UINT16_ARRAY,
        JSTypedArrayKind::Int32 => jsvm::JSVM_TypedarrayType_JSVM_INT32_ARRAY,
        JSTypedArrayKind::Uint32 => jsvm::JSVM_TypedarrayType_JSVM_UINT32_ARRAY,
        JSTypedArrayKind::Float32 => jsvm::JSVM_TypedarrayType_JSVM_FLOAT32_ARRAY,
        JSTypedArrayKind::Float64 => jsvm::JSVM_TypedarrayType_JSVM_FLOAT64_ARRAY,
        JSTypedArrayKind::BigInt64 => jsvm::JSVM_TypedarrayType_JSVM_BIGINT64_ARRAY,
        JSTypedArrayKind::BigUint64 => jsvm::JSVM_TypedarrayType_JSVM_BIGUINT64_ARRAY,
    }
}

impl JSTypedArrayOps for JSVMValue {
    fn from_array_buffer(
        ctx: &Self::Context,
        kind: JSTypedArrayKind,
        buffer: Self,
        byte_offset: usize,
        length: Option<usize>,
    ) -> Self {
        let _scope = crate::scope::OpScope::enter(ctx.to_raw());
        unsafe {
            let mut typed_array: jsvm::JSVM_Value = ptr::null_mut();
            let status = jsvm::OH_JSVM_CreateTypedarray(
                ctx.to_raw(),
                kind_to_ark_kind(kind),
                length.unwrap_or(0),
                buffer.resolve_handle(),
                byte_offset,
                &mut typed_array,
            );

            if status == jsvm::JSVM_Status_JSVM_OK {
                JSVMValue::from_owned_raw(ctx.to_raw(), typed_array).with_object()
            } else {
                Self::create_undefined(ctx)
            }
        }
    }

    fn get_kind(&self) -> Option<JSTypedArrayKind> {
        let _scope = crate::scope::OpScope::enter(self.env);
        self.typed_array_info()
            .and_then(|info| ark_kind_to_kind(info.kind))
    }

    fn get_array_buffer(&self) -> Option<Self> {
        let _scope = crate::scope::OpScope::enter(self.env);
        self.typed_array_info()
            .filter(|info| !info.array_buffer.is_null())
            .map(|info| JSVMValue::from_owned_raw(self.env, info.array_buffer))
    }

    fn get_byte_offset(&self) -> usize {
        let _scope = crate::scope::OpScope::enter(self.env);
        self.typed_array_info().map_or(0, |info| info.byte_offset)
    }

    fn get_length(&self) -> usize {
        let _scope = crate::scope::OpScope::enter(self.env);
        self.typed_array_info().map_or(0, |info| info.length)
    }

    fn get_byte_length(&self) -> usize {
        let _scope = crate::scope::OpScope::enter(self.env);
        let info = match self.typed_array_info() {
            Some(info) => info,
            None => return 0,
        };
        let element_size = match ark_kind_to_kind(info.kind) {
            Some(
                JSTypedArrayKind::Int8 | JSTypedArrayKind::Uint8 | JSTypedArrayKind::Uint8Clamped,
            ) => 1,
            Some(JSTypedArrayKind::Int16 | JSTypedArrayKind::Uint16) => 2,
            Some(
                JSTypedArrayKind::Int32 | JSTypedArrayKind::Uint32 | JSTypedArrayKind::Float32,
            ) => 4,
            Some(
                JSTypedArrayKind::BigInt64
                | JSTypedArrayKind::BigUint64
                | JSTypedArrayKind::Float64,
            ) => 8,
            None => 0,
        };
        info.length * element_size
    }
}
