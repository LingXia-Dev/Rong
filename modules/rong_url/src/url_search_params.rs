use crate::url::SharedUrlData;
use rong::{function::*, *};
use std::cell::RefCell;
use std::rc::Rc;
use url::form_urlencoded;

/// URLSearchParams implementation following the Web spec
/// https://url.spec.whatwg.org/#interface-urlsearchparams
#[js_class]
pub struct URLSearchParams {
    // Use Vec instead of HashMap to maintain insertion order
    params: RefCell<Vec<(String, String)>>,
    // Reference to the shared URL data, if this URLSearchParams is from URL.searchParams
    shared_data: Option<Rc<SharedUrlData>>,
}

/// Ordered query-string parameters following the URLSearchParams API.
#[js_class]
impl URLSearchParams {
    /// Construct from a query string, pair array, or string record.
    #[js_method(
        constructor,
        ts_params = "init?: string | Array<[string, string]> | Record<string, string>"
    )]
    fn new(init: Optional<JSValue>) -> JSResult<Self> {
        let mut params = Vec::new();

        if let Some(init) = init.0 {
            if init.is_string() {
                // Initialize from query string
                let query: String = init.to_rust()?;
                let query = query.strip_prefix('?').unwrap_or(&query);
                if !query.is_empty() {
                    params.extend(
                        form_urlencoded::parse(query.as_bytes())
                            .map(|(key, value)| (key.into_owned(), value.into_owned())),
                    );
                }
            } else if init.is_object() {
                let obj: JSObject = init.into();
                if let Some(arr) = JSArray::from_object(obj.clone()) {
                    // Initialize from key-value pair array [[k1,v1], [k2,v2]]
                    for pair in arr.iter_present::<JSArray>()? {
                        let pair = pair?;
                        if pair.len()? >= 2
                            && let Some(key) = pair.get_opt::<String>(0)?
                            && let Some(value) = pair.get_opt::<String>(1)?
                        {
                            params.push((key, value));
                        }
                    }
                } else {
                    // Initialize from object {k1: v1, k2: v2}
                    for item in obj.entries_as()?.into_iter() {
                        params.push((item.0, item.1));
                    }
                }
            }
        }

        Ok(Self {
            params: RefCell::new(params),
            shared_data: None,
        })
    }

    // Create a URLSearchParams instance from shared URL data
    pub(crate) fn from_shared_data(shared_data: Rc<SharedUrlData>) -> Self {
        let params = {
            let url = shared_data.url.borrow();
            url.query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect()
        };

        Self {
            params: RefCell::new(params),
            shared_data: Some(shared_data),
        }
    }

    /// Append a value without removing existing values.
    #[js_method]
    fn append(&mut self, name: String, value: String) {
        {
            let mut params = self.params.borrow_mut();
            params.push((name, value));
        }
        self.sync_url();
    }

    /// Delete every value associated with a name.
    #[js_method]
    fn delete(&mut self, name: String) {
        {
            let mut params = self.params.borrow_mut();
            params.retain(|(k, _)| k != &name);
        }
        self.sync_url();
    }

    /// Return the first value for a name, or null.
    #[js_method]
    fn get(&self, name: String) -> Option<String> {
        self.params
            .borrow()
            .iter()
            .find(|(k, _)| k == &name)
            .map(|(_, v)| v.clone())
    }

    /// Return every value associated with a name.
    #[js_method(rename = "getAll")]
    fn get_all(&self, name: String) -> Vec<String> {
        self.params
            .borrow()
            .iter()
            .filter(|(k, _)| k == &name)
            .map(|(_, v)| v.clone())
            .collect()
    }

    /// Test whether a name is present.
    #[js_method]
    fn has(&self, name: String) -> bool {
        self.params.borrow().iter().any(|(k, _)| k == &name)
    }

    /// Replace existing values for a name.
    #[js_method]
    fn set(&mut self, name: String, value: String) {
        {
            let mut params = self.params.borrow_mut();
            let mut found = false;

            // Remove all entries with the same name except the first one
            let mut i = 0;
            while i < params.len() {
                if params[i].0 == name {
                    if !found {
                        // Keep the first occurrence and update its value
                        params[i].1 = value.clone();
                        found = true;
                        i += 1;
                    } else {
                        // Remove subsequent occurrences
                        params.remove(i);
                    }
                } else {
                    i += 1;
                }
            }

            // If no entry was found, add a new one
            if !found {
                params.push((name, value.clone()));
            }
        }
        self.sync_url();
    }

    /// Sort parameters by name.
    #[js_method]
    fn sort(&mut self) {
        {
            let mut params = self.params.borrow_mut();
            params.sort_by(|(a, _), (b, _)| a.cmp(b));
        }
        self.sync_url();
    }

    /// Number of stored name/value pairs.
    #[js_method(getter)]
    fn size(&self) -> u32 {
        self.params.borrow().len() as u32
    }

    /// Return all name/value pairs.
    #[js_method(ts_return = "Array<[string, string]>")]
    fn entries(&self, ctx: JSContext) -> JSResult<JSArray> {
        let array = JSArray::new(&ctx)?;
        let params = self.params.borrow();

        for (key, value) in params.iter() {
            let item = JSArray::new(&ctx)?;
            item.push(key.as_str())?;
            item.push(value.as_str())?;
            array.push(item)?;
        }
        Ok(array)
    }

    /// Return all parameter names.
    #[js_method]
    fn keys(&self) -> Vec<String> {
        let params = self.params.borrow();
        params.iter().map(|(k, _)| k.clone()).collect()
    }

    /// Return all parameter values.
    #[js_method]
    fn values(&self) -> Vec<String> {
        let params = self.params.borrow();
        params.iter().map(|(_, v)| v.clone()).collect()
    }

    /// Invoke a callback for each parameter.
    #[js_method(
        rename = "forEach",
        ts_params = "callback: (value: string, key: string) => void, thisArg?: any"
    )]
    fn for_each(&self, callback: JSFunc, this_arg: Optional<JSObject>) -> JSResult<()> {
        let params = self.params.borrow();

        for (key, value) in params.iter() {
            let key = key.as_str();
            let value = value.as_str();
            if let Some(ref this) = this_arg.0 {
                callback.call::<_, ()>(Some(this.clone()), (value, key))?;
            } else {
                callback.call::<_, ()>(None, (value, key))?;
            }
        }

        Ok(())
    }

    #[allow(clippy::inherent_to_string)]
    /// Serialize as a query string without a leading `?`.
    #[js_method(rename = "toString")]
    pub fn to_string(&self) -> String {
        let params = self.params.borrow();
        if params.is_empty() {
            return String::new();
        }
        form_urlencoded::Serializer::new(String::new())
            .extend_pairs(params.iter())
            .finish()
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

impl URLSearchParams {
    // Sync parameters to the associated URL
    pub(crate) fn sync_url(&self) {
        if let Some(shared_data) = &self.shared_data {
            let query_string = self.to_string();
            let mut url = shared_data.url.borrow_mut();
            url.set_query(if query_string.is_empty() {
                None
            } else {
                Some(&query_string)
            });
        }
    }
}
