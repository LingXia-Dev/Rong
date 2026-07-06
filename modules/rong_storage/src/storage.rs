use super::*;
use redb::{Database, ReadableDatabase, ReadableTable};
use rong::{
    FromJSObj, HostError, IntoJSIteratorExt, JSContext, JSDate, JSObject, JSResult, JSValue,
    JsonToJSValue, function::Optional, js_class, js_export, js_method,
};
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(FromJSObj, Default)]
pub struct StorageOptionsInput {
    #[rename = "maxKeySize"]
    max_key_size: Option<u32>,
    #[rename = "maxValueSize"]
    max_value_size: Option<u32>,
    #[rename = "maxDataSize"]
    max_data_size: Option<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct StorageOptions {
    pub max_key_size: Option<u32>,
    pub max_value_size: Option<u32>,
    pub max_data_size: Option<u32>,
}

impl From<StorageOptionsInput> for StorageOptions {
    fn from(input: StorageOptionsInput) -> Self {
        Self {
            max_key_size: input.max_key_size,
            max_value_size: input.max_value_size,
            max_data_size: input.max_data_size,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct StorageConfig {
    max_key_size: usize,
    max_value_size: usize,
    max_user_data_size: usize,
}

impl StorageConfig {
    fn from_options(options: StorageOptions) -> JSResult<Self> {
        Ok(Self {
            max_key_size: Self::validate_limit(
                "maxKeySize",
                options.max_key_size,
                DEFAULT_MAX_KEY_SIZE,
            )?,
            max_value_size: Self::validate_limit(
                "maxValueSize",
                options.max_value_size,
                DEFAULT_MAX_VALUE_SIZE,
            )?,
            max_user_data_size: Self::validate_limit(
                "maxDataSize",
                options.max_data_size,
                DEFAULT_MAX_USER_DATA_SIZE,
            )?,
        })
    }

    fn validate_limit(field: &str, value: Option<u32>, default_value: usize) -> JSResult<usize> {
        let value = value.map(|v| v as usize).unwrap_or(default_value);
        if value == 0 {
            return Err(HostError::new(
                rong::error::E_INVALID_ARG,
                format!("{field} must be greater than 0"),
            )
            .with_name("TypeError")
            .into());
        }
        Ok(value)
    }

    fn limit_size_u32(&self) -> u32 {
        self.max_user_data_size.min(u32::MAX as usize) as u32
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            max_key_size: DEFAULT_MAX_KEY_SIZE,
            max_value_size: DEFAULT_MAX_VALUE_SIZE,
            max_user_data_size: DEFAULT_MAX_USER_DATA_SIZE,
        }
    }
}

#[js_export]
pub struct Storage {
    db: Rc<RefCell<Option<Database>>>,
    #[allow(dead_code)]
    db_path: PathBuf,
    config: StorageConfig,
}

impl Storage {
    /// Create a new `Storage` instance at the given path with custom options.
    ///
    /// This is the primary Rust-side API for creating `Storage` instances,
    /// used by both the JS constructor and environments that inject
    /// pre-configured instances via a platform namespace.
    ///
    /// Pass `StorageOptions::default()` for default limits.
    pub fn new<P: Into<PathBuf>>(path: P, options: StorageOptions) -> JSResult<Self> {
        Self::open_with_path(path.into(), options)
    }

    fn open_with_path(path: PathBuf, options: StorageOptions) -> JSResult<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                HostError::new(
                    rong::error::E_IO,
                    format!("Failed to create directory {:?}: {}", parent, e),
                )
            })?;
        }

        let db = Database::create(&path).map_err(|e| {
            HostError::new(
                rong::error::E_IO,
                format!("Failed to open database at {:?}: {}", path, e),
            )
        })?;

        Self::ensure_storage_table(&db)?;

        let config = StorageConfig::from_options(options)?;

        Ok(Self {
            db: Rc::new(RefCell::new(Some(db))),
            db_path: path,
            config,
        })
    }

    /// Close the underlying database for this Storage instance.
    ///
    /// This will drop the shared Database handle for all clones that share
    /// the same internal Rc. After calling close, any subsequent operation
    /// on this Storage (or its clones) will fail with a TypeError.
    pub fn close(&self) {
        let mut db_opt = self.db.borrow_mut();
        if db_opt.is_some() {
            *db_opt = None;
        }
    }

    fn with_db<F, T>(&self, f: F) -> JSResult<T>
    where
        F: FnOnce(&Database) -> JSResult<T>,
    {
        let db_opt = self.db.borrow();
        let db = db_opt.as_ref().ok_or_else(|| {
            HostError::new(rong::error::E_INVALID_STATE, "Storage database is closed")
                .with_name("TypeError")
        })?;
        f(db)
    }

    fn ensure_storage_table(db: &Database) -> JSResult<()> {
        let write_txn = db.begin_write().map_err(|e| {
            HostError::new(
                rong::error::E_IO,
                format!("Failed to begin write transaction: {}", e),
            )
        })?;

        {
            // This will create the table if it doesn't exist, or do nothing if it does
            write_txn.open_table(STORAGE_TABLE).map_err(|e| {
                HostError::new(
                    rong::error::E_IO,
                    format!("Failed to create/open storage table: {}", e),
                )
            })?;
        }

        write_txn.commit().map_err(|e| {
            HostError::new(
                rong::error::E_IO,
                format!("Failed to commit table creation: {}", e),
            )
        })?;

        Ok(())
    }
}

#[js_class]
impl Storage {
    #[js_method(constructor)]
    fn js_new(path: String, options: Optional<StorageOptionsInput>) -> JSResult<Self> {
        let opts = options.0.map(StorageOptions::from).unwrap_or_default();
        Self::new(PathBuf::from(path), opts)
    }

    /// Set a key-value pair in storage
    #[js_method]
    async fn set(&self, key: String, value: JSValue) -> JSResult<()> {
        let cfg = &self.config;

        // Validate key size
        if key.len() > cfg.max_key_size {
            return Err(HostError::new(
                rong::error::E_OUT_OF_RANGE,
                format!(
                    "Key size exceeds maximum limit of {} bytes",
                    cfg.max_key_size
                ),
            )
            .with_name("RangeError")
            .into());
        }

        // Convert value to JSON string to preserve type information
        let value_str = if value.is_string() {
            // For strings, store as JSON string to preserve type
            let s: String = value.clone().to_rust().map_err(|_| {
                HostError::new(rong::error::E_INVALID_ARG, "Failed to convert string value")
                    .with_name("TypeError")
            })?;
            serde_json::to_string(&s).map_err(|e| {
                HostError::new(
                    rong::error::E_INTERNAL,
                    format!("Failed to serialize string: {}", e),
                )
            })?
        } else if value.is_number() {
            // First get as f64 to avoid truncation issues
            let f: f64 = value.clone().to_rust().map_err(|_| {
                HostError::new(rong::error::E_INVALID_ARG, "Failed to convert number value")
                    .with_name("TypeError")
            })?;

            // Check if it's actually an integer (no fractional part)
            if f.fract() == 0.0 {
                // It's an integer, try to fit in appropriate integer type
                if f >= i32::MIN as f64 && f <= i32::MAX as f64 {
                    // Fits in i32
                    serde_json::to_string(&(f as i32)).map_err(|e| {
                        HostError::new(
                            rong::error::E_INTERNAL,
                            format!("Failed to serialize i32: {}", e),
                        )
                    })?
                } else if f >= 0.0 && f <= u32::MAX as f64 {
                    // Fits in u32
                    serde_json::to_string(&(f as u32)).map_err(|e| {
                        HostError::new(
                            rong::error::E_INTERNAL,
                            format!("Failed to serialize u32: {}", e),
                        )
                    })?
                } else {
                    // Large integer, store as f64
                    serde_json::to_string(&f).map_err(|e| {
                        HostError::new(
                            rong::error::E_INTERNAL,
                            format!("Failed to serialize large integer as f64: {}", e),
                        )
                    })?
                }
            } else {
                // It's a floating point number
                serde_json::to_string(&f).map_err(|e| {
                    HostError::new(
                        rong::error::E_INTERNAL,
                        format!("Failed to serialize f64: {}", e),
                    )
                })?
            }
        } else if value.is_bigint() {
            // Handle BigInt values (i64/u64)
            if let Ok(i) = value.clone().to_rust::<i64>() {
                serde_json::to_string(&i).map_err(|e| {
                    HostError::new(
                        rong::error::E_INTERNAL,
                        format!("Failed to serialize bigint i64: {}", e),
                    )
                })?
            } else if let Ok(u) = value.clone().to_rust::<u64>() {
                serde_json::to_string(&u).map_err(|e| {
                    HostError::new(
                        rong::error::E_INTERNAL,
                        format!("Failed to serialize bigint u64: {}", e),
                    )
                })?
            } else {
                return Err(
                    HostError::new(rong::error::E_INVALID_ARG, "Invalid bigint value")
                        .with_name("TypeError")
                        .into(),
                );
            }
        } else if value.is_boolean() {
            let b: bool = value.clone().to_rust().map_err(|_| {
                HostError::new(
                    rong::error::E_INVALID_ARG,
                    "Failed to convert boolean value",
                )
                .with_name("TypeError")
            })?;
            serde_json::to_string(&b).map_err(|e| {
                HostError::new(
                    rong::error::E_INTERNAL,
                    format!("Failed to serialize boolean: {}", e),
                )
            })?
        } else if value.is_null() {
            "null".to_string()
        } else if value.is_undefined() {
            return Err(HostError::new(
                rong::error::E_INVALID_ARG,
                "Cannot store undefined values",
            )
            .with_name("TypeError")
            .into());
        } else if let Ok(date) = value.clone().to_rust::<JSDate>() {
            // Handle Date objects by storing timestamp with type marker
            let timestamp = date.get_time().map_err(|e| {
                HostError::new(
                    rong::error::E_INVALID_ARG,
                    format!("Failed to get Date timestamp: {}", e),
                )
                .with_name("TypeError")
            })?;
            serde_json::to_string(&serde_json::json!({
                "__type": "Date",
                "timestamp": timestamp
            }))
            .map_err(|e| {
                HostError::new(
                    rong::error::E_INTERNAL,
                    format!("Failed to serialize Date: {}", e),
                )
            })?
        } else if let Ok(obj) = value.clone().to_rust::<JSObject>() {
            // Handle objects by converting to JSON string
            obj.to_json_string().map_err(|e| {
                HostError::new(
                    rong::error::E_INVALID_ARG,
                    format!("Failed to stringify object: {}", e),
                )
                .with_name("TypeError")
            })?
        } else if let Ok(s) = value.clone().to_rust::<String>() {
            // Fallback: convert to string
            serde_json::to_string(&s).map_err(|e| {
                HostError::new(
                    rong::error::E_INTERNAL,
                    format!("Failed to serialize fallback string: {}", e),
                )
            })?
        } else {
            return Err(HostError::new(
                rong::error::E_INVALID_ARG,
                "Value cannot be converted to a storable type",
            )
            .with_name("TypeError")
            .into());
        };

        // Validate value size
        if value_str.len() > cfg.max_value_size {
            return Err(HostError::new(
                rong::error::E_OUT_OF_RANGE,
                format!(
                    "Value size exceeds maximum limit of {} bytes",
                    cfg.max_value_size
                ),
            )
            .with_name("RangeError")
            .into());
        }

        self.with_db(|db| {
            // Check total storage size before adding new data
            let read_txn = db.begin_read().map_err(|e| {
                HostError::new(
                    rong::error::E_IO,
                    format!("Failed to begin read transaction: {}", e),
                )
            })?;

            let table = read_txn
                .open_table(STORAGE_TABLE)
                .map_err(|e| {
                    HostError::new(rong::error::E_IO, format!("Failed to open table: {}", e))
                })?;

            let mut current_size = 0;
            let mut existing_key_size = 0;

            // Calculate current storage size and check if key already exists
            let iter = table
                .iter()
                .map_err(|e| {
                    HostError::new(rong::error::E_IO, format!("Failed to create iterator: {}", e))
                })?;

            for item in iter {
                let (existing_key, existing_value) = item
                    .map_err(|e| {
                        HostError::new(rong::error::E_IO, format!("Failed to read item: {}", e))
                    })?;

                let key_size = existing_key.value().len();
                let value_size = existing_value.value().len();

                if existing_key.value().as_bytes() == key.as_bytes() {
                    existing_key_size = key_size + value_size;
                }
                current_size += key_size + value_size;
            }

            drop(table);
            drop(read_txn);

            // Calculate new size after this operation
            let new_entry_size = key.len() + value_str.len();
            let new_total_size = current_size - existing_key_size + new_entry_size;

            if new_total_size > cfg.max_user_data_size {
                return Err(HostError::new(rong::error::E_OUT_OF_RANGE, format!(
                    "Storage size would exceed maximum limit of {} bytes (current: {}, new entry: {})",
                    cfg.max_user_data_size,
                    current_size - existing_key_size,
                    new_entry_size
                )).with_name("RangeError").into());
            }

            // Store in database
            let write_txn = db.begin_write().map_err(|e| {
                HostError::new(
                    rong::error::E_IO,
                    format!("Failed to begin write transaction: {}", e),
                )
            })?;

            {
                let mut table = write_txn
                    .open_table(STORAGE_TABLE)
                    .map_err(|e| {
                        HostError::new(rong::error::E_IO, format!("Failed to open table: {}", e))
                    })?;

                table
                    .insert(key.as_str(), value_str.as_bytes())
                    .map_err(|e| {
                        HostError::new(rong::error::E_IO, format!("Failed to insert value: {}", e))
                    })?;
            }

            write_txn.commit().map_err(|e| {
                HostError::new(
                    rong::error::E_IO,
                    format!("Failed to commit transaction: {}", e),
                )
            })?;

            Ok(())
        })
    }

    /// Get a value from storage
    #[js_method]
    async fn get(&self, ctx: JSContext, key: String) -> JSResult<JSValue> {
        self.with_db(|db| {
            let read_txn = db.begin_read().map_err(|e| {
                HostError::new(
                    rong::error::E_IO,
                    format!("Failed to begin read transaction: {}", e),
                )
            })?;

            let table = read_txn.open_table(STORAGE_TABLE).map_err(|e| {
                HostError::new(rong::error::E_IO, format!("Failed to open table: {}", e))
            })?;

            match table.get(key.as_str()) {
                Ok(Some(value)) => {
                    let value_str = String::from_utf8(value.value().to_vec()).map_err(|e| {
                        HostError::new(
                            rong::error::E_INVALID_DATA,
                            format!("Failed to decode value as UTF-8: {}", e),
                        )
                    })?;

                    // Parse JSON back to appropriate JavaScript type
                    match serde_json::from_str::<serde_json::Value>(&value_str) {
                        Ok(json_value) => {
                            match json_value {
                                serde_json::Value::String(s) => Ok(JSValue::from_rust(&ctx, s)),
                                serde_json::Value::Number(n) => {
                                    // Let JSValue::from handle the intelligent number conversion
                                    if let Some(i) = n.as_i64() {
                                        Ok(JSValue::from_rust(&ctx, i))
                                    } else if let Some(u) = n.as_u64() {
                                        Ok(JSValue::from_rust(&ctx, u))
                                    } else if let Some(f) = n.as_f64() {
                                        Ok(JSValue::from_rust(&ctx, f))
                                    } else {
                                        Ok(JSValue::from_rust(&ctx, value_str))
                                    }
                                }
                                serde_json::Value::Bool(b) => Ok(JSValue::from_rust(&ctx, b)),
                                serde_json::Value::Null => Ok(JSValue::null(&ctx)),
                                serde_json::Value::Object(ref obj) => {
                                    // Check if this is a Date object
                                    if obj.get("__type")
                                        == Some(&serde_json::Value::String("Date".to_string()))
                                    {
                                        if let Some(timestamp) =
                                            obj.get("timestamp").and_then(|v| v.as_f64())
                                        {
                                            let date = JSDate::new(&ctx, timestamp);
                                            Ok(date.into_js_value())
                                        } else {
                                            Err(HostError::new(
                                                rong::error::E_INVALID_DATA,
                                                "Invalid Date object: missing timestamp",
                                            )
                                            .into())
                                        }
                                    } else {
                                        // Regular object, parse using JavaScript's JSON.parse
                                        value_str.as_str().json_to_js_value(&ctx)
                                    }
                                }
                                serde_json::Value::Array(_) => {
                                    // For arrays, parse them back using JavaScript's JSON.parse
                                    value_str.as_str().json_to_js_value(&ctx)
                                }
                            }
                        }
                        Err(_) => {
                            // If not valid JSON, return as string
                            Ok(JSValue::from_rust(&ctx, value_str))
                        }
                    }
                }
                Ok(None) => Ok(JSValue::undefined(&ctx)),
                Err(e) => Err(HostError::new(
                    rong::error::E_IO,
                    format!("Failed to get value: {}", e),
                )
                .into()),
            }
        })
    }

    /// Delete a key from storage
    #[js_method]
    async fn delete(&self, key: String) -> JSResult<()> {
        self.with_db(|db| {
            let write_txn = db.begin_write().map_err(|e| {
                HostError::new(
                    rong::error::E_IO,
                    format!("Failed to begin write transaction: {}", e),
                )
            })?;

            {
                let mut table = write_txn.open_table(STORAGE_TABLE).map_err(|e| {
                    HostError::new(rong::error::E_IO, format!("Failed to open table: {}", e))
                })?;

                table.remove(key.as_str()).map_err(|e| {
                    HostError::new(rong::error::E_IO, format!("Failed to remove key: {}", e))
                })?;
            }

            write_txn.commit().map_err(|e| {
                HostError::new(
                    rong::error::E_IO,
                    format!("Failed to commit transaction: {}", e),
                )
            })?;

            Ok(())
        })
    }

    /// Clear all data from storage
    #[js_method]
    async fn clear(&self) -> JSResult<()> {
        self.with_db(|db| {
            let write_txn = db.begin_write().map_err(|e| {
                HostError::new(
                    rong::error::E_IO,
                    format!("Failed to begin write transaction: {}", e),
                )
            })?;

            {
                let mut table = write_txn.open_table(STORAGE_TABLE).map_err(|e| {
                    HostError::new(rong::error::E_IO, format!("Failed to open table: {}", e))
                })?;

                // Remove all entries
                let keys: Vec<String> = table
                    .iter()
                    .map_err(|e| {
                        HostError::new(rong::error::E_IO, format!("Failed to iterate table: {}", e))
                    })?
                    .map(|item| {
                        item.map(|(key, _)| key.value().to_string()).map_err(|e| {
                            HostError::new(rong::error::E_IO, format!("Failed to read key: {}", e))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                for key in keys {
                    table.remove(key.as_str()).map_err(|e| {
                        HostError::new(rong::error::E_IO, format!("Failed to remove key: {}", e))
                    })?;
                }
            }

            write_txn.commit().map_err(|e| {
                HostError::new(
                    rong::error::E_IO,
                    format!("Failed to commit transaction: {}", e),
                )
            })?;

            Ok(())
        })
    }

    /// Storage list function that returns an iterator
    #[js_method(ts_return = "IterableIterator<string>")]
    async fn list(&self, ctx: JSContext, prefix: Optional<String>) -> JSResult<JSValue> {
        self.with_db(|db| {
            let read_txn = db.begin_read().map_err(|e| {
                HostError::new(
                    rong::error::E_IO,
                    format!("Failed to begin read transaction: {}", e),
                )
            })?;

            let table = read_txn.open_table(STORAGE_TABLE).map_err(|e| {
                HostError::new(rong::error::E_IO, format!("Failed to open table: {}", e))
            })?;

            let iter = table.iter().map_err(|e| {
                HostError::new(
                    rong::error::E_IO,
                    format!("Failed to create iterator: {}", e),
                )
            })?;

            let mut keys = Vec::new();
            for item in iter {
                let (key, _) = item.map_err(|e| {
                    HostError::new(rong::error::E_IO, format!("Failed to read item: {}", e))
                })?;
                let key_str = key.value().to_string();

                // Apply prefix filter if provided
                if let Some(ref prefix_str) = prefix.0 {
                    if key_str.starts_with(prefix_str) {
                        keys.push(key_str);
                    }
                } else {
                    keys.push(key_str);
                }
            }

            // Convert to JS iterator and then to JSValue
            let iter = keys.to_js_iter(&ctx)?;
            Ok(JSValue::from_rust(&ctx, iter))
        })
    }

    /// Storage info function
    #[js_method]
    async fn info(&self) -> JSResult<StorageInfo> {
        self.with_db(|db| {
            let read_txn = db.begin_read().map_err(|e| {
                HostError::new(
                    rong::error::E_IO,
                    format!("Failed to begin read transaction: {}", e),
                )
            })?;

            let table = read_txn.open_table(STORAGE_TABLE).map_err(|e| {
                HostError::new(rong::error::E_IO, format!("Failed to open table: {}", e))
            })?;

            let mut current_size = 0;
            let mut key_count = 0;
            let iter = table.iter().map_err(|e| {
                HostError::new(
                    rong::error::E_IO,
                    format!("Failed to create iterator: {}", e),
                )
            })?;

            for item in iter {
                let (key, value) = item.map_err(|e| {
                    HostError::new(rong::error::E_IO, format!("Failed to read item: {}", e))
                })?;

                current_size += key.value().len() + value.value().len();
                key_count += 1;
            }

            Ok(StorageInfo {
                current_size: current_size as u32,
                limit_size: self.config.limit_size_u32(),
                key_count: key_count as u32,
            })
        })
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}
