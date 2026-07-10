use rong::{function::*, *};
use rong_buffer::{Blob, File};
use std::collections::HashMap;
use url::form_urlencoded;

#[js_union]
#[derive(Clone)]
enum FormDataEntryValue {
    File(File),
    Blob(Blob),
    String(String),
}

#[js_class]
pub struct FormData {
    // Reasons for using Vec instead of HashMap:
    // 1. FormData specification requires maintaining insertion order, which Vec naturally supports
    // 2. FormData is mainly used for form data, where the data volume is usually small, so O(n) lookup performance impact is minimal
    // 3. The most common operation is iteration (entries/keys/values), for which Vec is more suitable
    // 4. Simplifies implementation by not needing to maintain multiple data structures to preserve order
    entries: Vec<(String, FormDataEntryValue, String)>,
}

// Iterator specifically for values() method
struct FormDataValuesIter {
    entries: Vec<(String, FormDataEntryValue, String)>,
    pos: usize,
}

impl Iterator for FormDataValuesIter {
    type Item = FormDataEntryValue;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos < self.entries.len() {
            let value = self.entries[self.pos].1.clone();
            self.pos += 1;
            Some(value)
        } else {
            None
        }
    }
}

// Iterator specifically for keys() method
struct FormDataKeysIter {
    entries: Vec<(String, FormDataEntryValue, String)>,
    pos: usize,
}

impl Iterator for FormDataKeysIter {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos < self.entries.len() {
            let key = self.entries[self.pos].0.clone();
            self.pos += 1;
            Some(key)
        } else {
            None
        }
    }
}

// Iterator for entries() method
struct FormDataEntriesIter {
    entries: Vec<(String, FormDataEntryValue, String)>,
    pos: usize,
    ctx: JSContext,
}

impl Iterator for FormDataEntriesIter {
    type Item = JSArray;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos < self.entries.len() {
            let (key, value, _) = self.entries[self.pos].clone();
            self.pos += 1;
            let array = JSArray::new(&self.ctx).ok()?;
            array.push(key).ok()?;
            array.push(value).ok()?;
            Some(array)
        } else {
            None
        }
    }
}

#[js_class]
impl FormData {
    #[js_method(constructor)]
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// appends a new value onto an existing key inside a FormData object, or adds
    /// the key if it does not already exist.
    #[js_method]
    fn append(&mut self, name: String, value: FormDataEntryValue, filename: Optional<String>) {
        let filename = match &value {
            FormDataEntryValue::File(file) => filename.0.unwrap_or_else(|| file.name()),
            FormDataEntryValue::Blob(_) => filename.0.unwrap_or_else(|| "blob".to_string()),
            FormDataEntryValue::String(_) => String::new(),
        };
        self.entries.push((name, value, filename));
    }

    /// deletes a key and its value(s) from a FormData object
    #[js_method]
    fn delete(&mut self, name: String) {
        self.entries.retain(|(key, _, _)| key != &name);
    }

    /// returns the first value associated with a given key from within a FormData object
    #[js_method]
    fn get(&self, name: String) -> Option<FormDataEntryValue> {
        self.entries
            .iter()
            .find(|(key, _, _)| key == &name)
            .map(|(_, value, _)| value.clone())
    }

    /// returns all the values associated with a given key from within a FormData object.
    #[js_method(rename = "getAll")]
    fn get_all(&self, name: String) -> Vec<FormDataEntryValue> {
        self.entries
            .iter()
            .filter(|(key, _, _)| key == &name)
            .map(|(_, value, _)| value.clone())
            .collect()
    }

    /// returns whether a FormData object contains a certain key.
    #[js_method]
    fn has(&self, name: String) -> bool {
        self.entries.iter().any(|(key, _, _)| key == &name)
    }

    /// sets a new value for an existing key inside a FormData object, or adds the
    /// key/value if it does not already exist.
    #[js_method]
    fn set(&mut self, name: String, value: FormDataEntryValue, filename: Optional<String>) {
        self.delete(name.clone());
        self.append(name, value, filename);
    }

    /// returns an iterator which iterates through all values contained in the FormData
    #[js_method]
    fn values(&self, ctx: JSContext) -> JSResult<JSObject> {
        FormDataValuesIter {
            entries: self.entries.clone(),
            pos: 0,
        }
        .to_js_iter(&ctx)
    }

    /// returns an iterator which iterates through all key/value pairs contained
    /// in the FormData. The key of each pair is a string, and the value is either
    /// a string or a Blob
    #[js_method]
    fn entries(&self, ctx: JSContext) -> JSResult<JSObject> {
        FormDataEntriesIter {
            entries: self.entries.clone(),
            pos: 0,
            ctx: ctx.clone(),
        }
        .to_js_iter(&ctx)
    }

    /// returns an iterator which iterates through all keys contained in the FormData
    #[js_method]
    fn keys(&self, ctx: JSContext) -> JSResult<JSObject> {
        FormDataKeysIter {
            entries: self.entries.clone(),
            pos: 0,
        }
        .to_js_iter(&ctx)
    }

    /// forEach() executes a provided function once for each key/value pair in the FormData
    #[js_method(rename = "forEach")]
    fn for_each(
        &self,
        this: This<JSObject>,
        callback: JSFunc,
        this_arg: Optional<JSObject>,
    ) -> JSResult<()> {
        let this_arg = this_arg.0;

        for (name, value, _) in &self.entries {
            callback.call::<_, ()>(
                this_arg.clone(),
                (value.clone(), name.clone(), this.0.clone()),
            )?;
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

impl FormData {
    // Add new methods for serialization
    pub(crate) async fn serialize(&self, _ctx: JSContext) -> JSResult<(Vec<u8>, String)> {
        let boundary = uuid::Uuid::new_v4().to_string();
        let mut body = Vec::new();

        for (name, value, filename) in &self.entries {
            body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());

            match value {
                FormDataEntryValue::File(file) => {
                    body.extend_from_slice(
                        format!(
                            "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                            name, filename
                        )
                        .as_bytes(),
                    );
                    body.extend_from_slice(
                        format!("Content-Type: {}\r\n\r\n", file.mime_type()).as_bytes(),
                    );
                    body.extend_from_slice(file.bytes_ref().as_ref());
                }
                FormDataEntryValue::Blob(blob) => {
                    body.extend_from_slice(
                        format!(
                            "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                            name, filename
                        )
                        .as_bytes(),
                    );
                    body.extend_from_slice(
                        format!("Content-Type: {}\r\n\r\n", blob.mime_type()).as_bytes(),
                    );
                    body.extend_from_slice(blob.bytes_ref().as_ref());
                }
                FormDataEntryValue::String(value) => {
                    body.extend_from_slice(
                        format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name)
                            .as_bytes(),
                    );
                    body.extend_from_slice(value.as_bytes());
                }
            }
            body.extend_from_slice(b"\r\n");
        }

        body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
        Ok((body, boundary))
    }

    pub(crate) fn content_type(boundary: &str) -> String {
        format!("multipart/form-data; boundary={}", boundary)
    }

    pub(crate) fn from_bytes(body: &[u8], content_type: &str) -> JSResult<Self> {
        if let Some(boundary) = Self::parse_boundary(content_type) {
            return Self::from_multipart(body, &boundary);
        }

        let lower = content_type.to_ascii_lowercase();
        if lower.starts_with("application/x-www-form-urlencoded") {
            let text = String::from_utf8_lossy(body).into_owned();
            return Ok(Self::from_urlencoded(&text));
        }

        Err(HostError::new(
            rong::error::E_INVALID_ARG,
            "Unsupported Content-Type for formData()",
        )
        .with_name("TypeError")
        .into())
    }

    pub(crate) fn parse_boundary(content_type: &str) -> Option<String> {
        let lower = content_type.to_ascii_lowercase();
        if !lower.starts_with("multipart/form-data") {
            return None;
        }
        for part in content_type.split(';').skip(1) {
            let part = part.trim();
            if let Some((key, val)) = part.split_once('=')
                && key.trim().eq_ignore_ascii_case("boundary")
            {
                return Some(val.trim().trim_matches('"').to_string());
            }
        }
        None
    }

    pub(crate) fn from_urlencoded(body: &str) -> Self {
        let mut entries = Vec::new();
        for (key, value) in form_urlencoded::parse(body.as_bytes()) {
            entries.push((
                key.into_owned(),
                FormDataEntryValue::String(value.into_owned()),
                String::new(),
            ));
        }
        Self { entries }
    }

    pub(crate) fn from_multipart(body: &[u8], boundary: &str) -> JSResult<Self> {
        let delimiter = format!("--{}", boundary).into_bytes();
        let delimiter_with_crlf = format!("\r\n--{}", boundary).into_bytes();

        if !body.starts_with(&delimiter) {
            return Err(HostError::new(
                rong::error::E_INVALID_ARG,
                "Invalid multipart body: missing starting boundary",
            )
            .with_name("TypeError")
            .into());
        }

        let mut entries = Vec::new();
        let mut pos = delimiter.len();

        // End without parts
        if body.get(pos..pos + 2) == Some(b"--") {
            return Ok(Self { entries });
        }

        if body.get(pos..pos + 2) == Some(b"\r\n") {
            pos += 2;
        }

        loop {
            let header_end = find_subslice(body, b"\r\n\r\n", pos).ok_or_else(|| {
                HostError::new(
                    rong::error::E_INVALID_ARG,
                    "Invalid multipart body: missing header terminator",
                )
                .with_name("TypeError")
            })?;
            let headers = parse_headers(&body[pos..header_end]);
            let content_start = header_end + 4;

            let next_boundary = find_subslice(body, &delimiter_with_crlf, content_start)
                .ok_or_else(|| {
                    HostError::new(
                        rong::error::E_INVALID_ARG,
                        "Invalid multipart body: missing boundary",
                    )
                    .with_name("TypeError")
                })?;

            let content = &body[content_start..next_boundary];

            let disposition = headers
                .get("content-disposition")
                .ok_or_else(|| {
                    HostError::new(
                        rong::error::E_INVALID_ARG,
                        "Missing Content-Disposition header",
                    )
                    .with_name("TypeError")
                })?
                .to_string();
            let (name, filename) = parse_content_disposition(&disposition);
            let name = name.ok_or_else(|| {
                HostError::new(
                    rong::error::E_INVALID_ARG,
                    "Content-Disposition missing name",
                )
                .with_name("TypeError")
            })?;

            let content_type = headers
                .get("content-type")
                .map(|s| s.to_string())
                .unwrap_or_default();

            if let Some(filename) = filename {
                let file =
                    File::from_parts(content_type, content.to_vec(), filename.clone(), None)?;
                entries.push((name, FormDataEntryValue::File(file), filename));
            } else {
                let value = String::from_utf8_lossy(content).into_owned();
                entries.push((name, FormDataEntryValue::String(value), String::new()));
            }

            pos = next_boundary + delimiter_with_crlf.len();

            if body.get(pos..pos + 2) == Some(b"--") {
                break;
            }

            if body.get(pos..pos + 2) == Some(b"\r\n") {
                pos += 2;
            }
        }

        Ok(Self { entries })
    }
}

fn parse_headers(raw: &[u8]) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    let text = String::from_utf8_lossy(raw);
    for line in text.split("\r\n") {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    headers
}

fn parse_content_disposition(value: &str) -> (Option<String>, Option<String>) {
    let mut name = None;
    let mut filename = None;

    for part in value.split(';').skip(1) {
        let part = part.trim();
        if let Some((key, raw)) = part.split_once('=') {
            let key = key.trim().to_ascii_lowercase();
            let val = raw.trim().trim_matches('"').to_string();
            if key == "name" {
                name = Some(val);
            } else if key == "filename" {
                filename = Some(val);
            }
        }
    }

    (name, filename)
}

fn find_subslice(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() || start >= haystack.len() {
        return None;
    }
    haystack[start..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|idx| idx + start)
}

pub(crate) fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_class::<FormData>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;

    #[test]
    fn test_formdata() {
        async_run!(|ctx: JSContext| async move {
            rong_assert::init(&ctx)?;
            rong_console::init(&ctx)?;
            rong_encoding::init(&ctx)?;
            rong_buffer::init(&ctx)?;
            init(&ctx)?;

            let passed = UnitJSRunner::load_script(&ctx, "formdata.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }
}
