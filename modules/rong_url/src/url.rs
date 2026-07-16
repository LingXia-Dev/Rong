use crate::URLSearchParams;
use rong::{function::*, *};
use std::cell::RefCell;
use std::rc::Rc;
use url::Url;

#[js_class]
pub struct URL {
    shared_data: Rc<SharedUrlData>,
    search_params: Option<URLSearchParams>,
}

// Shared data between URL and URLSearchParams
#[derive(Debug)]
pub(crate) struct SharedUrlData {
    pub url: RefCell<Url>,
}

/// WHATWG-style URL parser and mutable URL object.
#[js_class]
impl URL {
    /// Parse an absolute URL or resolve a relative URL against a base.
    #[js_method(constructor)]
    fn new(url: String, base: Optional<String>) -> JSResult<Self> {
        let inner = if let Some(base) = base.0 {
            let base = Url::parse(&base).map_err(|e| {
                HostError::new(
                    rong::error::E_INVALID_ARG,
                    format!("Invalid base URL: {}", e),
                )
                .with_name("TypeError")
            })?;
            base.join(&url).map_err(|e| {
                HostError::new(rong::error::E_INVALID_ARG, format!("Invalid URL: {}", e))
                    .with_name("TypeError")
            })?
        } else {
            Url::parse(&url).map_err(|e| {
                HostError::new(rong::error::E_INVALID_ARG, format!("Invalid URL: {}", e))
                    .with_name("TypeError")
            })?
        };

        let shared_data = Rc::new(SharedUrlData {
            url: RefCell::new(inner),
        });

        Ok(Self {
            shared_data,
            search_params: None,
        })
    }

    /// Fragment identifier including the leading `#`.
    #[js_method(getter, rename = "hash")]
    fn get_hash(&self) -> String {
        self.inner()
            .fragment()
            .map(|f| format!("#{}", f))
            .unwrap_or_default()
    }

    #[js_method(setter, rename = "hash")]
    fn set_hash(&mut self, value: String) {
        let mut url = self.inner_mut();
        url.set_fragment(if value.is_empty() {
            None
        } else {
            Some(value.trim_start_matches('#'))
        });
    }

    /// Hostname and port.
    #[js_method(getter, rename = "host")]
    fn get_host(&self) -> String {
        let url = self.inner();
        format!(
            "{}{}",
            url.host_str().unwrap_or_default(),
            url.port().map(|p| format!(":{}", p)).unwrap_or_default()
        )
    }

    #[js_method(setter, rename = "host")]
    fn set_host(&mut self, value: String) {
        let mut url = self.inner_mut();
        let _ = url.set_host(Some(&value));
    }

    /// Hostname without the port.
    #[js_method(getter)]
    fn hostname(&self) -> String {
        self.inner().host_str().unwrap_or_default().to_string()
    }

    #[js_method(setter, rename = "hostname")]
    fn set_hostname(&mut self, value: String) {
        let mut url = self.inner_mut();
        let _ = url.set_host(Some(&value));
    }

    /// Serialized URL.
    #[js_method(getter)]
    fn href(&self) -> String {
        self.inner().to_string()
    }

    #[js_method(setter, rename = "href")]
    fn set_href(&mut self, value: String) -> JSResult<()> {
        let new_url = Url::parse(&value).map_err(|e| {
            HostError::new(rong::error::E_INVALID_ARG, format!("Invalid URL: {}", e))
                .with_name("TypeError")
        })?;

        {
            let mut url = self.inner_mut();
            *url = new_url;
        }

        // Reset search_params because the URL has changed
        self.search_params = None;

        Ok(())
    }

    /// Origin consisting of scheme and host.
    #[js_method(getter)]
    fn origin(&self) -> String {
        let url = self.inner();
        format!("{}://{}", url.scheme(), self.get_host())
    }

    /// Password component.
    #[js_method(getter)]
    fn password(&self) -> String {
        self.inner().password().unwrap_or_default().to_string()
    }

    #[js_method(setter, rename = "password")]
    fn set_password(&mut self, value: String) {
        let mut url = self.inner_mut();
        let _ = url.set_password(Some(&value));
    }

    /// Path component.
    #[js_method(getter, rename = "pathname")]
    fn pathname(&self) -> String {
        self.inner().path().to_string()
    }

    #[js_method(setter, rename = "pathname")]
    fn set_pathname(&mut self, value: String) {
        let path = if !value.starts_with('/') {
            format!("/{}", value)
        } else {
            value
        };

        let mut url = self.inner_mut();
        url.set_path(&path);
    }

    /// Port as a string.
    #[js_method(getter, rename = "port")]
    fn port(&self) -> String {
        self.inner()
            .port()
            .map(|p| p.to_string())
            .unwrap_or_default()
    }

    #[js_method(setter, rename = "port")]
    fn set_port(&mut self, value: String) {
        let port = if value.is_empty() {
            None
        } else if let Ok(port) = value.parse() {
            Some(port)
        } else {
            return; // Invalid port, ignore
        };

        let mut url = self.inner_mut();
        let _ = url.set_port(port);
    }

    /// Protocol including the trailing `:`.
    #[js_method(getter, rename = "protocol")]
    fn protocol(&self) -> String {
        format!("{}:", self.inner().scheme())
    }

    #[js_method(setter, rename = "protocol")]
    fn set_protocol(&mut self, value: String) {
        let protocol = value.trim_end_matches(':');
        let mut url = self.inner_mut();
        if url.set_scheme(protocol).is_err() {
            // If setting scheme fails, try to create a new URL with the new protocol
            let new_url_str = url
                .as_str()
                .replace(&format!("{}:", url.scheme()), &format!("{}:", protocol));
            if let Ok(new_url) = Url::parse(&new_url_str) {
                *url = new_url;
            }
        }
    }

    /// Query string including the leading `?`.
    #[js_method(getter, rename = "search")]
    fn search(&self) -> String {
        if let Some(query) = self.inner().query() {
            format!("?{}", query)
        } else {
            String::new()
        }
    }

    #[js_method(setter, rename = "search")]
    fn set_search(&mut self, value: String) {
        {
            let mut url = self.inner_mut();
            let query = value.strip_prefix('?').unwrap_or(&value);
            url.set_query(if query.is_empty() { None } else { Some(query) });
        }

        // Reset search_params because the query string has changed
        self.search_params = None;
    }

    /// Username component.
    #[js_method(getter, rename = "username")]
    fn username(&self) -> String {
        self.inner().username().to_string()
    }

    #[js_method(setter, rename = "username")]
    fn set_username(&mut self, value: String) {
        let mut url = self.inner_mut();
        let _ = url.set_username(&value);
    }

    /// Live search-parameter view backed by this URL.
    #[js_method(getter, rename = "searchParams")]
    fn search_params(&mut self) -> URLSearchParams {
        self.search_params
            .get_or_insert_with(|| URLSearchParams::from_shared_data(self.shared_data.clone()))
            .clone()
    }

    #[allow(clippy::inherent_to_string)]
    /// Serialize this URL.
    #[js_method(rename = "toString")]
    pub fn to_string(&self) -> String {
        self.inner().to_string()
    }

    /// Serialize this URL for JSON conversion.
    #[js_method(rename = "toJSON")]
    fn to_json(&self) -> String {
        self.inner().to_string()
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

impl URL {
    // Helper method to get a reference to the inner URL
    pub(crate) fn inner(&self) -> std::cell::Ref<'_, Url> {
        self.shared_data.url.borrow()
    }

    // Helper method to get a mutable reference to the inner URL
    pub(crate) fn inner_mut(&self) -> std::cell::RefMut<'_, Url> {
        self.shared_data.url.borrow_mut()
    }
}
