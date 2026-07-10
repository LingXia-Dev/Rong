//! DOMException implementation
//!
//! This module provides a DOMException implementation that follows the Node.js error handling patterns.
//! It includes all standard error names used in Node.js.
//!
//! # Example
//! ```javascript
//! // Create a DOMException with a specific error type
//! const ex = new DOMException("Operation failed", "AbortError");
//! console.log(ex.name);    // "AbortError"
//! console.log(ex.message); // "Operation failed"
//! ```
//!
//! # Features
//! - Standard Node.js error names (INDEX_SIZE_ERR, ABORT_ERR, etc.)
//! - Full compatibility with Node.js error handling patterns
//!
//! # Error Categories
//! - DOM Hierarchy: INDEX_SIZE_ERR, HIERARCHY_REQUEST_ERR
//! - Data Handling: DOMSTRING_SIZE_ERR, DATA_CLONE_ERR
//! - State Management: INVALID_STATE_ERR, INVALID_ACCESS_ERR
//! - Network Operations: NETWORK_ERR, ABORT_ERR
//! - Resource Management: QUOTA_EXCEEDED_ERR, TIMEOUT_ERR
//!
//! //! # Notes
//!
//! The `code` property is **not implemented** in this module, as it has been
//! deprecated in the DOM specification and is no longer recommended for use.
//! Instead, use the `name` property to identify the type of error.

use rong::{function::*, *};

/// Macro to define error names
#[allow(clippy::upper_case_acronyms)]
macro_rules! define_error_names {
    ($($name:ident => $display:literal),* $(,)?) => {
        #[allow(clippy::upper_case_acronyms)]
        #[allow(non_camel_case_types)]
        #[derive(Debug, Clone, Copy)]
        pub enum DOMExceptionName {
            $($name,)*
        }

        impl DOMExceptionName {
            /// Get the name as string with automatic conversion
            #[inline]
            pub fn as_str(&self) -> &'static str {
                match self {
                    $(DOMExceptionName::$name => $display,)*
                }
            }

            /// Iterate over all error names efficiently
            #[inline]
            pub fn iter() -> impl Iterator<Item = &'static str> {
                [$($display),*].into_iter()
            }

            /// Convert string to corresponding DOMExceptionName variant
            /// Default to ERROR variant if no match found
            #[inline]
            pub fn find_or_default(s: &str) -> Self {
                match s {
                    $(stringify!($name) | $display => DOMExceptionName::$name,)*
                    _ => DOMExceptionName::ERROR,
                }
            }
        }
    }
}

// This enum DOMExceptionName represents all standard DOM exception names supported by Node.js
// https://webidl.spec.whatwg.org/#idl-DOMException-error-names
define_error_names! {
    INDEX_SIZE_ERR => "IndexSizeError",
    DOMSTRING_SIZE_ERR => "DOMStringSizeError",
    HIERARCHY_REQUEST_ERR => "HierarchyRequestError",
    INVALID_CHARACTER_ERR => "InvalidCharacterError",
    NO_DATA_ALLOWED_ERR => "NoDataAllowedError",
    NO_MODIFICATION_ALLOWED_ERR => "NoModificationAllowedError",
    NOT_FOUND_ERR => "NotFoundError",
    NOT_SUPPORTED_ERR => "NotSupportedError",
    INUSE_ATTRIBUTE_ERR => "InUseAttributeError",
    INVALID_STATE_ERR => "InvalidStateError",
    SYNTAX_ERR => "SyntaxError",
    INVALID_MODIFICATION_ERR => "InvalidModificationError",
    NAMESPACE_ERR => "NamespaceError",
    INVALID_ACCESS_ERR => "InvalidAccessError",
    VALIDATION_ERR => "ValidationError",
    TYPE_MISMATCH_ERR => "TypeMismatchError",
    SECURITY_ERR => "SecurityError",
    NETWORK_ERR => "NetworkError",
    ABORT_ERR => "AbortError",
    URL_MISMATCH_ERR => "URLMismatchError",
    QUOTA_EXCEEDED_ERR => "QuotaExceededError",
    TIMEOUT_ERR => "TimeoutError",
    INVALID_NODE_TYPE_ERR => "InvalidNodeTypeError",
    DATA_CLONE_ERR => "DataCloneError",
    ERROR => "Error",
}

// Implement From<&str> to replace from_str
impl From<&str> for DOMExceptionName {
    fn from(s: &str) -> Self {
        DOMExceptionName::find_or_default(s)
    }
}

// Implement From<Option<String>> using From<&str>
impl From<Option<String>> for DOMExceptionName {
    fn from(value: Option<String>) -> Self {
        match value {
            Some(s) => s.as_str().into(),
            None => DOMExceptionName::ERROR,
        }
    }
}

/// DOMException implementation following Node.js error types
#[js_class]
pub struct DOMException {
    name: DOMExceptionName,
    message: String,
}

#[js_class]
impl DOMException {
    #[js_method(constructor)]
    fn new(message: Optional<String>, name: Optional<String>) -> Self {
        Self {
            message: message.0.unwrap_or_default(),
            name: name.0.into(),
        }
    }

    #[js_method(getter)]
    fn name(&self) -> String {
        self.name.as_str().to_string()
    }

    #[js_method(getter)]
    fn message(&self) -> String {
        self.message.clone()
    }

    #[js_method(getter)]
    fn stack(&self) -> String {
        "NotImplemented".to_string()
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }

    /// Create a new DOMException instance
    ///
    /// # Arguments
    /// * `message` - Error message
    /// * `name` - DOMExceptionName
    ///
    /// # Returns
    /// Returns a JSObject containing the new DOMException instance
    pub fn create(ctx: &JSContext, message: &str, name: DOMExceptionName) -> JSResult<JSObject> {
        let dom = DOMException {
            message: message.to_string(),
            name,
        };
        Ok(Class::lookup::<DOMException>(ctx)?.instance(dom))
    }
}

/// Register exception-related classes with the JavaScript engine
pub fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_class::<DOMException>()?;
    let constructor = Class::lookup::<DOMException>(ctx)?;

    // Add all error names as static properties
    for name in DOMExceptionName::iter() {
        constructor.define_property(
            name,
            PropertyDescriptor::from_rust(ctx, name)
                .enumerable()
                .readonly()
                .non_configurable(),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;

    #[test]
    fn test_dom_exception() {
        async_run!(|ctx: JSContext| async move {
            init(&ctx)?;

            // Test constructor with name and message
            let result = ctx.eval::<JSObject>(Source::from_bytes(
                r#"
                const ex = new DOMException("Operation failed", "ABORT_ERR");
                ({
                    name: ex.name,
                    message: ex.message
                })
                "#,
            ))?;

            assert_eq!(result.get::<_, String>("name")?, "AbortError");
            assert_eq!(result.get::<_, String>("message")?, "Operation failed");

            Ok(())
        });
    }
}
