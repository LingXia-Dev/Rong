use base64::{Engine as _, engine::general_purpose};
use rong::{Coerced, HostError, JSResult};

/// Decodes a string of data which has been encoded using base-64 encoding
pub fn atob(input: Coerced<String>) -> JSResult<String> {
    let input = input.0;
    let decoded = general_purpose::STANDARD.decode(input).map_err(|e| {
        HostError::new(
            rong::error::E_INVALID_ARG,
            format!("Failed to decode base64: {}", e),
        )
        .with_name("TypeError")
    })?;
    let decoded_str = String::from_utf8(decoded).map_err(|e| {
        HostError::new(
            rong::error::E_INVALID_ARG,
            format!("Invalid UTF-8 sequence: {}", e),
        )
        .with_name("TypeError")
    })?;
    Ok(decoded_str)
}

/// Creates a base-64 ASCII encoded string from the input string
pub fn btoa(input: Coerced<String>) -> JSResult<String> {
    let input = input.0;
    let encoded = general_purpose::STANDARD.encode(input);
    Ok(encoded)
}
