//! `DOMException`-shaped errors for the Web Cryptography API.
//!
//! The Rong host-error convention (see `rong_http`) is to build a
//! [`HostError`] with a machine-readable `E_*` code and to set the JavaScript
//! `name` with [`HostError::with_name`]. The engine turns a non-built-in name
//! into an `Error` whose `.name` is that string, which is what the
//! [Web Cryptography API][spec] observes when it says "throw a
//! `NotSupportedError`".
//!
//! [spec]: https://w3c.github.io/webcrypto/

use rong::{HostError, RongJSError, error as codes};

macro_rules! crypto_errors {
    ($($(#[$doc:meta])* $fn_name:ident => ($js_name:literal, $code:expr)),* $(,)?) => {
        $(
            $(#[$doc])*
            pub(crate) fn $fn_name(message: impl Into<String>) -> RongJSError {
                HostError::new($code, message).with_name($js_name).into()
            }
        )*
    };
}

crypto_errors! {
    /// The algorithm is not one this build implements.
    not_supported => ("NotSupportedError", codes::E_NOT_SUPPORTED),
    /// The key is unusable for the requested operation (usage/extractable).
    invalid_access => ("InvalidAccessError", codes::E_INVALID_STATE),
    /// The operation itself failed (bad tag, bad padding, bad length).
    operation => ("OperationError", codes::E_ERROR),
    /// The supplied key data could not be interpreted.
    data => ("DataError", codes::E_INVALID_DATA),
    /// More random bytes were requested than the platform will produce at once.
    quota_exceeded => ("QuotaExceededError", codes::E_OUT_OF_RANGE),
    /// A `BufferSource` of the wrong element type was supplied.
    type_mismatch => ("TypeMismatchError", codes::E_TYPE),
    /// A required dictionary member was missing or structurally invalid.
    syntax => ("SyntaxError", codes::E_INVALID_ARG),
    /// A plain WebIDL type failure (wrong JavaScript type entirely).
    type_error => ("TypeError", codes::E_INVALID_ARG),
}
