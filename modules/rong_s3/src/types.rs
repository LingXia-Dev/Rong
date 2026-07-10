use crate::config::{S3Config, S3ConfigOverlay};
use rong::*;

/// Options for constructing or overriding an S3 client.
#[derive(Clone, Debug, Default, FromJSObj)]
pub struct S3ClientOptions {
    /// AWS access key ID.
    #[rename = "accessKeyId"]
    pub access_key_id: Option<String>,
    /// AWS secret access key.
    #[rename = "secretAccessKey"]
    pub secret_access_key: Option<String>,
    /// AWS session token (STS).
    #[rename = "sessionToken"]
    pub session_token: Option<String>,
    /// AWS region.
    pub region: Option<String>,
    /// Custom endpoint URL (for S3-compatible services).
    pub endpoint: Option<String>,
    /// Bucket name.
    pub bucket: Option<String>,
    /// Default ACL for uploads (e.g. "public-read").
    pub acl: Option<String>,
    /// Use virtual-hosted-style URLs instead of path-style.
    #[rename = "virtualHostedStyle"]
    pub virtual_hosted_style: Option<bool>,
}

/// Options for write operations.
#[derive(Clone, Debug, Default, FromJSObj)]
pub struct S3WriteOptions {
    /// Content-Type header.
    #[rename = "type"]
    pub content_type: Option<String>,
}

#[derive(Clone, Debug, Default, FromJSObj)]
#[ts_skip]
pub(crate) struct S3ClientWriteOptions {
    #[rename = "accessKeyId"]
    pub access_key_id: Option<String>,
    #[rename = "secretAccessKey"]
    pub secret_access_key: Option<String>,
    #[rename = "sessionToken"]
    pub session_token: Option<String>,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub acl: Option<String>,
    #[rename = "virtualHostedStyle"]
    pub virtual_hosted_style: Option<bool>,
    #[rename = "type"]
    pub content_type: Option<String>,
}

/// Options for presigning URLs.
#[derive(Clone, Debug, Default, FromJSObj)]
pub struct S3PresignOptions {
    /// Expiration in seconds.
    #[rename = "expiresIn"]
    pub expires_in: Option<f64>,
    /// HTTP method.
    #[ts_type = "\"GET\" | \"PUT\" | \"DELETE\""]
    pub method: Option<String>,
}

#[derive(Clone, Debug, Default, FromJSObj)]
#[ts_skip]
pub(crate) struct S3ClientPresignOptions {
    #[rename = "accessKeyId"]
    pub access_key_id: Option<String>,
    #[rename = "secretAccessKey"]
    pub secret_access_key: Option<String>,
    #[rename = "sessionToken"]
    pub session_token: Option<String>,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub acl: Option<String>,
    #[rename = "virtualHostedStyle"]
    pub virtual_hosted_style: Option<bool>,
    #[rename = "expiresIn"]
    pub expires_in: Option<f64>,
    #[ts_type = "\"GET\" | \"PUT\" | \"DELETE\""]
    pub method: Option<String>,
}

/// Options for list operations.
#[derive(Clone, Debug, Default, FromJSObj)]
pub struct S3ListOptions {
    /// Filter objects by key prefix.
    pub prefix: Option<String>,
    /// Maximum number of keys to return.
    #[rename = "maxKeys"]
    pub max_keys: Option<f64>,
    /// Start listing after this key (for pagination).
    #[rename = "startAfter"]
    pub start_after: Option<String>,
}

#[derive(Clone, Debug, Default, FromJSObj)]
#[ts_skip]
pub(crate) struct S3ClientListOptions {
    #[rename = "accessKeyId"]
    pub access_key_id: Option<String>,
    #[rename = "secretAccessKey"]
    pub secret_access_key: Option<String>,
    #[rename = "sessionToken"]
    pub session_token: Option<String>,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub acl: Option<String>,
    #[rename = "virtualHostedStyle"]
    pub virtual_hosted_style: Option<bool>,
    pub prefix: Option<String>,
    #[rename = "maxKeys"]
    pub max_keys: Option<f64>,
    #[rename = "startAfter"]
    pub start_after: Option<String>,
}

/// Object metadata returned by `stat()`.
#[derive(Clone, Debug, IntoJSObj)]
pub struct S3StatResult {
    /// ETag of the object.
    pub etag: Option<String>,
    /// Last modified timestamp (ISO 8601 string).
    #[rename = "lastModified"]
    pub last_modified: Option<String>,
    /// Object size in bytes.
    pub size: f64,
    /// Content-Type of the object.
    #[rename = "type"]
    pub content_type: Option<String>,
}

/// Single object entry in a list result.
#[derive(Clone, Debug, IntoJSObj)]
pub struct S3ListEntry {
    /// Object key.
    pub key: String,
    /// Object size in bytes.
    pub size: f64,
    /// Last modified timestamp (ISO 8601 string).
    #[rename = "lastModified"]
    pub last_modified: String,
    /// ETag of the object.
    pub etag: Option<String>,
}

/// Result of a list operation.
#[derive(Clone, Debug, IntoJSObj)]
pub struct S3ListResult {
    /// List of matching objects.
    pub contents: Vec<S3ListEntry>,
    /// Whether there are more results (use `startAfter` to paginate).
    #[rename = "isTruncated"]
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
