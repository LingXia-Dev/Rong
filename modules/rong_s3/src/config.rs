use rong::*;
use std::rc::Rc;

pub(crate) trait S3ConfigOverlay {
    fn apply_to_config(&self, config: &mut S3Config);
    fn config_override_key(&self) -> Option<&'static str>;
}

/// S3 configuration: credentials + bucket + endpoint.
#[derive(Clone, Debug)]
pub struct S3Config {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
    pub region: String,
    pub endpoint: Option<String>,
    pub bucket: String,
    pub acl: Option<String>,
    pub virtual_hosted_style: bool,
}

impl Default for S3Config {
    fn default() -> Self {
        Self {
            access_key_id: String::new(),
            secret_access_key: String::new(),
            session_token: None,
            region: "us-east-1".to_string(),
            endpoint: None,
            bucket: String::new(),
            acl: None,
            virtual_hosted_style: false,
        }
    }
}

impl S3Config {
    /// Build config from typed JS options.
    pub(crate) fn from_options<T: S3ConfigOverlay>(options: Option<T>) -> Self {
        let mut config = Self::default();
        if let Some(options) = options {
            options.apply_to_config(&mut config);
        }
        config
    }

    /// Overlay typed JS options on top of this config. Only provided fields are overwritten.
    pub(crate) fn merge_options<T: S3ConfigOverlay>(&self, options: Option<&T>) -> Self {
        let mut config = self.clone();
        if let Some(options) = options {
            options.apply_to_config(&mut config);
        }
        config
    }

    /// Create an s3::Bucket from this config.
    pub fn create_bucket(&self) -> JSResult<s3::Bucket> {
        if self.access_key_id.is_empty() || self.secret_access_key.is_empty() {
            return Err(HostError::new(
                "ERR_S3_MISSING_CREDENTIALS",
                "S3 credentials are required. Pass accessKeyId and secretAccessKey in the options object.",
            )
            .into());
        }
        if self.bucket.is_empty() {
            return Err(HostError::new(
                "ERR_S3_MISSING_CREDENTIALS",
                "S3 bucket name is required. Pass bucket in the options object.",
            )
            .into());
        }

        let region = match &self.endpoint {
            Some(endpoint) => s3::Region::Custom {
                region: self.region.clone(),
                endpoint: endpoint.clone(),
            },
            None => s3::Region::Custom {
                region: self.region.clone(),
                endpoint: format!("https://s3.{}.amazonaws.com", self.region),
            },
        };

        let credentials = s3::creds::Credentials::new(
            Some(&self.access_key_id),
            Some(&self.secret_access_key),
            self.session_token.as_deref(),
            None,
            None,
        )
        .map_err(|e| {
            HostError::new(
                "ERR_S3_MISSING_CREDENTIALS",
                format!("Failed to create S3 credentials: {}", e),
            )
        })?;

        let bucket = s3::Bucket::new(&self.bucket, region, credentials)
            .map_err(|e| HostError::new("ERR_S3", format!("Failed to create S3 bucket: {}", e)))?;

        let bucket = if self.virtual_hosted_style {
            bucket
        } else {
            bucket.with_path_style()
        };

        Ok(*bucket)
    }

    pub fn into_rc(self) -> Rc<S3Config> {
        Rc::new(self)
    }
}
