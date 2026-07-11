use crate::config::{S3Config, S3ConfigOverlay};
use rong::*;

/// Options for constructing or overriding an S3 client.
#[derive(Clone, Debug, Default, FromJSObject)]
pub struct S3ClientOptions {
    /// AWS access key ID.
    #[js_name = "accessKeyId"]
    pub access_key_id: Option<String>,
    /// AWS secret access key.
    #[js_name = "secretAccessKey"]
    pub secret_access_key: Option<String>,
    /// AWS session token (STS).
    #[js_name = "sessionToken"]
    pub session_token: Option<String>,
    /// AWS region. Defaults to `us-east-1`.
    pub region: Option<String>,
    /// Custom endpoint URL (for S3-compatible services).
    pub endpoint: Option<String>,
    /// Bucket name.
    pub bucket: Option<String>,
    /// Default ACL for uploads (e.g. "public-read").
    pub acl: Option<String>,
    /// Use virtual-hosted-style URLs instead of path-style. Defaults to false.
    #[js_name = "virtualHostedStyle"]
    pub virtual_hosted_style: Option<bool>,
}

/// Options for write operations.
#[derive(Clone, Debug, Default, FromJSObject)]
pub struct S3WriteOptions {
    /// Content-Type header. Defaults to `application/octet-stream` when it cannot be inferred.
    #[js_name = "type"]
    pub content_type: Option<String>,
}

#[derive(Clone, Debug, Default, FromJSObject)]
#[ts_skip]
pub(crate) struct S3ClientWriteOptions {
    #[js_name = "accessKeyId"]
    pub access_key_id: Option<String>,
    #[js_name = "secretAccessKey"]
    pub secret_access_key: Option<String>,
    #[js_name = "sessionToken"]
    pub session_token: Option<String>,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub acl: Option<String>,
    #[js_name = "virtualHostedStyle"]
    pub virtual_hosted_style: Option<bool>,
    #[js_name = "type"]
    pub content_type: Option<String>,
}

/// Options for presigning URLs.
#[derive(Clone, Debug, Default, FromJSObject)]
pub struct S3PresignOptions {
    /// Expiration in seconds. Defaults to 86400 (24 hours).
    #[js_name = "expiresIn"]
    pub expires_in: Option<f64>,
    /// HTTP method. Defaults to `GET`.
    #[ts_type = "\"GET\" | \"PUT\" | \"DELETE\""]
    pub method: Option<String>,
}

#[derive(Clone, Debug, Default, FromJSObject)]
#[ts_skip]
pub(crate) struct S3ClientPresignOptions {
    #[js_name = "accessKeyId"]
    pub access_key_id: Option<String>,
    #[js_name = "secretAccessKey"]
    pub secret_access_key: Option<String>,
    #[js_name = "sessionToken"]
    pub session_token: Option<String>,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub acl: Option<String>,
    #[js_name = "virtualHostedStyle"]
    pub virtual_hosted_style: Option<bool>,
    #[js_name = "expiresIn"]
    pub expires_in: Option<f64>,
    #[ts_type = "\"GET\" | \"PUT\" | \"DELETE\""]
    pub method: Option<String>,
}

/// Options for list operations.
#[derive(Clone, Debug, Default, FromJSObject)]
pub struct S3ListOptions {
    /// Filter objects by key prefix.
    pub prefix: Option<String>,
    /// Maximum number of keys to return.
    #[js_name = "maxKeys"]
    pub max_keys: Option<f64>,
    /// Start listing after this key (for pagination).
    #[js_name = "startAfter"]
    pub start_after: Option<String>,
}

#[derive(Clone, Debug, Default, FromJSObject)]
#[ts_skip]
pub(crate) struct S3ClientListOptions {
    #[js_name = "accessKeyId"]
    pub access_key_id: Option<String>,
    #[js_name = "secretAccessKey"]
    pub secret_access_key: Option<String>,
    #[js_name = "sessionToken"]
    pub session_token: Option<String>,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub acl: Option<String>,
    #[js_name = "virtualHostedStyle"]
    pub virtual_hosted_style: Option<bool>,
    pub prefix: Option<String>,
    #[js_name = "maxKeys"]
    pub max_keys: Option<f64>,
    #[js_name = "startAfter"]
    pub start_after: Option<String>,
}

/// Object metadata returned by `stat()`.
#[derive(Clone, Debug, IntoJSObject)]
pub struct S3StatResult {
    /// ETag of the object.
    pub etag: Option<String>,
    /// Last modified timestamp (ISO 8601 string).
    #[js_name = "lastModified"]
    pub last_modified: Option<String>,
    /// Object size in bytes.
    pub size: f64,
    /// Content-Type of the object.
    #[js_name = "type"]
    pub content_type: Option<String>,
}

/// Single object entry in a list result.
#[derive(Clone, Debug, IntoJSObject)]
pub struct S3ListEntry {
    /// Object key.
    pub key: String,
    /// Object size in bytes.
    pub size: f64,
    /// Last modified timestamp (ISO 8601 string).
    #[js_name = "lastModified"]
    pub last_modified: String,
    /// ETag of the object.
    pub etag: Option<String>,
}

/// Result of a list operation.
#[derive(Clone, Debug, IntoJSObject)]
pub struct S3ListResult {
    /// List of matching objects.
    pub contents: Vec<S3ListEntry>,
    /// Whether there are more results (use `startAfter` to paginate).
    #[js_name = "isTruncated"]
    pub is_truncated: bool,
}

macro_rules! impl_s3_overlay {
    ($ty:ty) => {
        impl S3ConfigOverlay for $ty {
            fn apply_to_config(&self, config: &mut S3Config) {
                if let Some(v) = &self.access_key_id {
                    config.access_key_id = v.clone();
                }
                if let Some(v) = &self.secret_access_key {
                    config.secret_access_key = v.clone();
                }
                if let Some(v) = &self.session_token {
                    config.session_token = Some(v.clone());
                }
                if let Some(v) = &self.region {
                    config.region = v.clone();
                }
                if let Some(v) = &self.endpoint {
                    config.endpoint = Some(v.clone());
                }
                if let Some(v) = &self.bucket {
                    config.bucket = v.clone();
                }
                if let Some(v) = &self.acl {
                    config.acl = Some(v.clone());
                }
                if let Some(v) = self.virtual_hosted_style {
                    config.virtual_hosted_style = v;
                }
            }
        }
    };
}

impl_s3_overlay!(S3ClientOptions);
impl_s3_overlay!(S3ClientWriteOptions);
impl_s3_overlay!(S3ClientPresignOptions);
impl_s3_overlay!(S3ClientListOptions);
