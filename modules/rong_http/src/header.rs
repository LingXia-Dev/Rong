use http::header::{self, HeaderMap, HeaderName, HeaderValue};
use rong::{
    function::{Optional, This},
    js_class, js_method, *,
};

#[js_class]
#[derive(Default)]
pub struct Headers {
    headers: HeaderMap<HeaderValue>,
}

impl Headers {
    /// Create `Headers` from an existing `HeaderMap`.
    pub(crate) fn from_header_map(headers: HeaderMap<HeaderValue>) -> Self {
        Self { headers }
    }

    // Get a reference to the inner HeaderMap
    pub(crate) fn as_header_map(&self) -> &HeaderMap<HeaderValue> {
        &self.headers
    }
}

#[js_class]
impl Headers {
    #[js_method(constructor)]
    pub(crate) fn new(init: Optional<JSValue>) -> JSResult<Self> {
        let mut headers = HeaderMap::new();

        if let Some(init) = init.0 {
            if let Some(obj) = init.into_object() {
                // instance of Headers
                if let Ok(other_headers) = obj.borrow::<Headers>() {
                    headers.extend(
                        other_headers
                            .headers
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone())),
                    );
                } else if let Some(array) = JSArray::from_object(obj.clone()) {
                    for item in array.iter_values()? {
                        let item = item?;
                        if let Some(pair) = item.into_object().and_then(JSArray::from_object) {
                            if pair.len()? != 2 {
                                return Err(HostError::new(
                                    rong::error::E_INVALID_ARG,
                                    "Each header must be an array of [name, value]",
                                )
                                .with_name("TypeError")
                                .into());
                            }

                            let key: String = pair.get_opt(0)?.ok_or_else(|| {
                                HostError::new(
                                    rong::error::E_INVALID_ARG,
                                    "Header name is required",
                                )
                                .with_name("TypeError")
                            })?;
                            let value: String = pair.get_opt(1)?.ok_or_else(|| {
                                HostError::new(
                                    rong::error::E_INVALID_ARG,
                                    "Header value is required",
                                )
                                .with_name("TypeError")
                            })?;

                            match (
                                HeaderName::try_from(key.as_str()),
                                HeaderValue::try_from(value.as_str()),
                            ) {
                                (Ok(name), Ok(value)) => {
                                    headers.append(name, value);
                                }
                                (Err(_), _) => {
                                    return Err(HostError::new(
                                        rong::error::E_INVALID_ARG,
                                        format!("Invalid header name: {}", key),
                                    )
                                    .with_name("TypeError")
                                    .into());
                                }
                                (_, Err(_)) => {
                                    return Err(HostError::new(
                                        rong::error::E_INVALID_ARG,
                                        "Invalid header value",
                                    )
                                    .with_name("TypeError")
                                    .into());
                                }
                            }
                        } else {
                            return Err(HostError::new(
                                rong::error::E_INVALID_ARG,
                                "Each header must be an array of [name, value]",
                            )
                            .with_name("TypeError")
                            .into());
                        }
                    }
                } else {
                    for entry in obj.entries()? {
                        let (key, value) = entry.try_into::<String, String>()?;

                        match (
                            HeaderName::try_from(key.as_str()),
                            HeaderValue::try_from(value.as_str()),
                        ) {
                            (Ok(name), Ok(value)) => {
                                headers.append(name, value);
                            }
                            (Err(_), _) => {
                                return Err(HostError::new(
                                    rong::error::E_INVALID_ARG,
                                    format!("Invalid header name: {}", key),
                                )
                                .with_name("TypeError")
                                .into());
                            }
                            (_, Err(_)) => {
                                return Err(HostError::new(
                                    rong::error::E_INVALID_ARG,
                                    "Invalid header value",
                                )
                                .with_name("TypeError")
                                .into());
                            }
                        }
                    }
                }
            } else {
                return Err(
                    HostError::new(rong::error::E_INVALID_ARG, "Invalid Headers init")
                        .with_name("TypeError")
                        .into(),
                );
            }
        }

        Ok(Self { headers })
    }

    /// The append() method of the Headers interface appends a new value onto an
    /// existing header inside a Headers object, or adds the header if it does not
    /// already exist.
    #[js_method]
    pub(crate) fn append(&mut self, name: String, value: String) {
        if let (Ok(name), Ok(value)) = (
            HeaderName::try_from(name.as_str()),
            HeaderValue::try_from(value.as_str()),
        ) {
            self.headers.append(name, value);
        }
    }

    /// The delete() method of the Headers interface deletes a header from the current Headers object.
    #[js_method]
    pub(crate) fn delete(&mut self, name: String) {
        if let Ok(name) = HeaderName::try_from(name.as_str()) {
            self.headers.remove(&name);
        }
    }

    /// The get() method of the Headers interface returns a byte string of all the
    /// values of a header within a Headers object with a given name. If the requested
    /// header doesn't exist in the Headers object, it returns null.
    ///
    /// The name of the HTTP header whose values you want to retrieve from the Headers
    /// object. If the given name is not the name of an HTTP header, this method throws
    /// a TypeError. The name is case-insensitive.
    #[js_method]
    pub(crate) fn get(&self, name: String) -> JSResult<Option<String>> {
        match HeaderName::try_from(name.as_str()) {
            Ok(name) => {
                let values: Vec<&str> = self
                    .headers
                    .get_all(&name)
                    .into_iter()
                    .filter_map(|v| v.to_str().ok())
                    .collect();

                if values.is_empty() {
                    return Ok(None);
                }

                Ok(Some(values.join(", ")))
            }
            Err(_) => Err(HostError::new(
                rong::error::E_INVALID_ARG,
                format!("Invalid header name: {}", name),
            )
            .with_name("TypeError")
            .into()),
        }
    }

    /// The has() method  returns a boolean stating whether a Headers object contains
    /// a certain header.
    ///
    /// The name of the HTTP header you want to test for. If the given name is not a
    /// valid HTTP header name, this method throws a TypeError.
    #[js_method]
    pub(crate) fn has(&self, name: String) -> JSResult<bool> {
        match HeaderName::try_from(name.as_str()) {
            Ok(name) => Ok(self.headers.contains_key(&name)),
            Err(_) => Err(HostError::new(
                rong::error::E_INVALID_ARG,
                format!("Invalid header name: {}", name),
            )
            .with_name("TypeError")
            .into()),
        }
    }

    /// The set() method sets a new value for an existing header inside a Headers
    /// object, or adds the header if it does not already exist.
    ///
    /// The name of the HTTP header you want to set to a new value. If the given
    /// name is not the name of an HTTP header, this method throws a TypeError.
    #[js_method]
    pub(crate) fn set(&mut self, name: String, value: String) -> JSResult<()> {
        // Check for null characters in value
        if value.contains('\0') {
            return Err(HostError::new(
                rong::error::E_INVALID_ARG,
                "Header value must not contain null characters",
            )
            .with_name("TypeError")
            .into());
        }

        match (
            HeaderName::try_from(name.as_str()),
            HeaderValue::try_from(value.as_str()),
        ) {
            (Ok(name), Ok(value)) => {
                self.headers.insert(name, value);
                Ok(())
            }
            (Err(_), _) => Err(HostError::new(
                rong::error::E_INVALID_ARG,
                format!("Invalid header name: {}", name),
            )
            .with_name("TypeError")
            .into()),
            (_, Err(_)) => Err(
                HostError::new(rong::error::E_INVALID_ARG, "Invalid header value")
                    .with_name("TypeError")
                    .into(),
            ),
        }
    }

    /// The Headers.entries() method returns an iterator allowing to go through all
    /// key/value pairs contained in this object. Both the key and value of each pair are String objects
    #[js_method]
    fn entries(&self, ctx: JSContext) -> JSResult<JSObject> {
        let entries = self
            .headers
            .iter()
            .map(|(name, value)| {
                vec![
                    name.as_str().to_lowercase(),
                    value.to_str().unwrap_or_default().to_string(),
                ]
            })
            .collect::<Vec<_>>();

        // Use new simplified API
        entries.to_js_iter(&ctx)
    }

    /// The Headers.keys() method returns an iterator allowing to go through all
    /// keys contained in this object. The keys are String objects.
    #[js_method]
    fn keys(&self, ctx: JSContext) -> JSResult<JSObject> {
        let keys = self
            .headers
            .keys()
            .map(|name| name.as_str().to_lowercase())
            .collect::<Vec<_>>();

        // Use new simplified API
        keys.to_js_iter(&ctx)
    }

    /// The Headers.values() method returns an iterator allowing to go through all
    /// values contained in this object. The values are String objects
    #[js_method]
    fn values(&self, ctx: JSContext) -> JSResult<JSObject> {
        let values = self
            .headers
            .values()
            .filter_map(|value| value.to_str().ok().map(|s| s.to_string()))
            .collect::<Vec<_>>();

        // Use new simplified API
        values.to_js_iter(&ctx)
    }

    /// getSetCookie() returns an array containing the values of all Set-Cookie
    /// headers associated with a response.
    ///
    /// If no Set-Cookie headers are set, the method will return an empty array
    #[js_method(rename = "getSetCookie")]
    fn get_set_cookie(&self) -> Vec<String> {
        let mut cookies = Vec::new();

        // HeaderMap natively supports multi-value headers
        for cookie in self.headers.get_all(header::SET_COOKIE) {
            if let Ok(cookie_str) = cookie.to_str() {
                cookies.push(cookie_str.to_string());
            }
        }
        cookies
    }

    /// forEach() method executes a callback function once per each key/value pair
    /// in the Headers object
    #[js_method(rename = "forEach")]
    fn for_each(
        &self,
        this: This<JSObject>, // Header Object
        callback: JSFunc,
        this_arg: Optional<JSObject>,
    ) -> JSResult<()> {
        // Value to use as this when executing callback. It's optional
        let this_arg = this_arg.0;

        for (name, value) in &self.headers {
            let value_str = value.to_str().unwrap_or_default();
            let name_str = name.as_str();

            callback.call::<_, ()>(this_arg.clone(), (value_str, name_str, this.0.clone()))?;
        }
        Ok(())
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

pub(crate) fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_class::<Headers>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;

    #[test]
    fn test_headers() {
        async_run!(|ctx: JSContext| async move {
            rong_assert::init(&ctx)?;
            rong_console::init(&ctx)?;
            rong_encoding::init(&ctx)?;
            crate::header::init(&ctx)?; // Initialize Headers before running tests

            let passed = UnitJSRunner::load_script(&ctx, "header.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }
}
