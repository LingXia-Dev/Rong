use rong::function::Optional;
use rong::*;
use rusqlite::types::Value;
use std::cell::RefCell;
use std::rc::Rc;

mod statement;

use statement::Statement;

rong::js_api! {
    fn register_sqlite_namespace(ctx) {
        namespace RongNamespace = ctx.host_namespace();

        /// A value bindable as a SQLite statement parameter.
        type SQLiteParam = "null | boolean | number | bigint | string | ArrayBuffer | Uint8Array";
        /// Ordered positional parameters bound to a SQLite statement.
        type SQLiteParams = "SQLiteParam[]";
        /// A value returned by SQLite.
        type SQLiteValue = "null | number | bigint | string | ArrayBuffer";
        /// A SQLite result row keyed by column name.
        type SQLiteRow = "Record<string, SQLiteValue>";

        class SQLite = SQLite;
    }
}

/// Result of a write (INSERT/UPDATE/DELETE).
#[derive(Clone, Debug, IntoJSObject)]
pub(crate) struct RunResult {
    /// Number of rows changed.
    changes: f64,
    /// Row id of the last inserted row; large values come back as `bigint`.
    #[js_name = "lastInsertRowid"]
    #[ts_type = "number | bigint"]
    last_insert_rowid: i64,
}

fn sqlite_error(msg: impl Into<String>) -> RongJSError {
    HostError::new("ERR_SQLITE", msg)
        .with_name("SQLiteError")
        .into()
}

/// Shared connection handle used by both SQLite and Statement.
pub(crate) type SharedConn = Rc<RefCell<Option<rusqlite::Connection>>>;

/// SQLite database connection.
#[js_class]
pub struct SQLite {
    pub(crate) conn: SharedConn,
    filename: String,
}

/// Synchronous SQLite database connection.
#[js_class]
impl SQLite {
    /// Open a database file, or an in-memory database when filename is omitted.
    #[js_method(constructor)]
    fn new(filename: Optional<String>) -> JSResult<Self> {
        let filename = filename.0.unwrap_or_else(|| ":memory:".to_string());
        let conn = if filename == ":memory:" {
            rusqlite::Connection::open_in_memory()
        } else {
            rusqlite::Connection::open(&filename)
        };
        let conn = conn.map_err(|e| sqlite_error(format!("Failed to open: {}", e)))?;
        // Prefer WAL for file-backed databases, but don't make otherwise valid opens fail if
        // the underlying database is read-only or the VFS does not support WAL.
        if filename != ":memory:" {
            let _ = conn.execute_batch("PRAGMA journal_mode=WAL;");
        }
        Ok(Self {
            conn: Rc::new(RefCell::new(Some(conn))),
            filename,
        })
    }

    /// Execute a SQL string that may contain multiple statements.
    /// Use for DDL / schema setup.
    #[js_method]
    fn exec(&self, sql: String) -> JSResult<()> {
        let borrow = self.conn.borrow();
        let conn = borrow
            .as_ref()
            .ok_or_else(|| sqlite_error("Database is closed"))?;
        conn.execute_batch(&sql)
            .map_err(|e| sqlite_error(e.to_string()))?;
        Ok(())
    }

    /// Execute a single statement with optional parameters.
    /// Returns `{ changes, lastInsertRowid }`.
    #[js_method(ts_params = "sql: string, params?: SQLiteParams")]
    fn run(&self, sql: String, params: Optional<JSArray>) -> JSResult<RunResult> {
        let borrow = self.conn.borrow();
        let conn = borrow
            .as_ref()
            .ok_or_else(|| sqlite_error("Database is closed"))?;
        let values = js_array_to_params(&params)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = values
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();

        let changes = conn
            .execute(&sql, param_refs.as_slice())
            .map_err(|e| sqlite_error(e.to_string()))?;

        Ok(RunResult {
            changes: changes as f64,
            last_insert_rowid: conn.last_insert_rowid(),
        })
    }

    /// Execute a query and return all matching rows as an array of objects.
    #[js_method(
        ts_return = "SQLiteRow[]",
        ts_params = "sql: string, params?: SQLiteParams"
    )]
    fn query(&self, ctx: JSContext, sql: String, params: Optional<JSArray>) -> JSResult<JSArray> {
        let borrow = self.conn.borrow();
        let conn = borrow
            .as_ref()
            .ok_or_else(|| sqlite_error("Database is closed"))?;
        query_rows(&ctx, conn, &sql, &params)
    }

    /// Create a prepared statement for repeated execution.
    #[js_method(ts_return = "Statement")]
    fn prepare(&self, ctx: JSContext, sql: String) -> JSResult<JSObject> {
        {
            let borrow = self.conn.borrow();
            let conn = borrow
                .as_ref()
                .ok_or_else(|| sqlite_error("Database is closed"))?;
            // Validate SQL
            conn.prepare(&sql)
                .map_err(|e| sqlite_error(e.to_string()))?;
        }
        let stmt = Statement::create(self.conn.clone(), sql);
        Ok(Class::lookup::<Statement>(&ctx)?.instance(stmt))
    }

    /// Run a function inside a transaction. Commits on success, rolls back on error.
    #[js_method(ts_params = "callback: () => void")]
    fn transaction(&self, callback: JSFunc) -> JSResult<()> {
        {
            let borrow = self.conn.borrow();
            let conn = borrow
                .as_ref()
                .ok_or_else(|| sqlite_error("Database is closed"))?;
            conn.execute_batch("BEGIN")
                .map_err(|e| sqlite_error(e.to_string()))?;
        }

        let result: JSResult<()> = callback.call(None, ());

        match result {
            Ok(_) => {
                let borrow = self.conn.borrow();
                let conn = borrow
                    .as_ref()
                    .ok_or_else(|| sqlite_error("Database is closed"))?;
                conn.execute_batch("COMMIT")
                    .map_err(|e| sqlite_error(e.to_string()))?;
                Ok(())
            }
            Err(e) => {
                let borrow = self.conn.borrow();
                if let Some(conn) = borrow.as_ref() {
                    let _ = conn.execute_batch("ROLLBACK");
                }
                Err(e)
            }
        }
    }

    /// Filename used to open the database.
    #[js_method(getter)]
    fn filename(&self) -> String {
        self.filename.clone()
    }

    /// Whether the database is currently inside a transaction.
    #[js_method(getter, rename = "inTransaction")]
    fn in_transaction(&self) -> JSResult<bool> {
        let borrow = self.conn.borrow();
        let conn = borrow
            .as_ref()
            .ok_or_else(|| sqlite_error("Database is closed"))?;
        Ok(!conn.is_autocommit())
    }

    /// Close the database. Further operations fail.
    #[js_method]
    fn close(&self) -> JSResult<()> {
        let conn = self.conn.borrow_mut().take();
        if let Some(conn) = conn
            && let Err((conn, e)) = conn.close()
        {
            *self.conn.borrow_mut() = Some(conn);
            return Err(sqlite_error(e.to_string()));
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

// ==================== Shared Helpers ====================

/// Convert a JS array of parameters to rusqlite Values.
pub(crate) fn js_array_to_params(params: &Optional<JSArray>) -> JSResult<Vec<Value>> {
    let Some(arr) = params.0.as_ref() else {
        return Ok(vec![]);
    };
    let len = arr.len()?;
    let mut out = Vec::with_capacity(len as usize);
    for i in 0..len {
        let val: JSValue = arr
            .get_opt::<JSValue>(i)?
            .ok_or_else(|| sqlite_error("missing param"))?;
        out.push(js_value_to_sqlite(&val)?);
    }
    Ok(out)
}

/// Convert a single JS value to a rusqlite Value.
fn js_value_to_sqlite(val: &JSValue) -> JSResult<Value> {
    if val.is_null() || val.is_undefined() {
        return Ok(Value::Null);
    }
    if val.is_boolean() {
        let b: bool = val.clone().to_rust()?;
        return Ok(Value::Integer(b as i64));
    }
    if val.is_bigint() {
        let s: String = val
            .clone()
            .to_rust()
            .map_err(|_| sqlite_error("Invalid bigint parameter"))?;
        let n: i64 = s
            .parse()
            .map_err(|_| sqlite_error("BigInt parameter is out of SQLite INTEGER range"))?;
        return Ok(Value::Integer(n));
    }
    if val.is_number() {
        let n: f64 = val.clone().to_rust()?;
        if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
            return Ok(Value::Integer(n as i64));
        }
        return Ok(Value::Real(n));
    }
    if val.is_string() {
        let s: String = val.clone().to_rust()?;
        return Ok(Value::Text(s));
    }
    if val.is_array_buffer() {
        let ab: JSArrayBuffer = val
            .clone()
            .to_rust()
            .map_err(|_| sqlite_error("invalid ArrayBuffer"))?;
        return Ok(Value::Blob(ab.as_bytes().to_vec()));
    }
    if let Some(obj) = val.clone().into_object()
        && let Some(ta) = AnyJSTypedArray::from_object(obj)
        && let Some(bytes) = ta.as_bytes()
    {
        return Ok(Value::Blob(bytes.to_vec()));
    }
    Err(sqlite_error(
        "Unsupported parameter type. Use null, boolean, number, bigint, string, ArrayBuffer, or Uint8Array.",
    ))
}

/// Set a SQLite value on a JS object property.
pub(crate) fn set_sqlite_value(
    ctx: &JSContext,
    obj: &JSObject,
    key: &str,
    val: &Value,
) -> JSResult<()> {
    match val {
        Value::Null => {
            obj.set(key, JSValue::null(ctx))?;
        }
        Value::Integer(n) => {
            obj.set(key, *n)?;
        }
        Value::Real(n) => {
            obj.set(key, *n)?;
        }
        Value::Text(s) => {
            obj.set(key, s.as_str())?;
        }
        Value::Blob(b) => {
            let ab = JSArrayBuffer::from_bytes(ctx, b)
                .map_err(|e| sqlite_error(format!("ArrayBuffer: {}", e)))?;
            obj.set(key, JSValue::from_rust(ctx, ab))?;
        }
    }
    Ok(())
}

/// Convert a SQLite value to a JS value.
pub(crate) fn sqlite_value_to_js(ctx: &JSContext, val: &Value) -> JSResult<JSValue> {
    match val {
        Value::Null => Ok(JSValue::null(ctx)),
        Value::Integer(n) => Ok(JSValue::from_rust(ctx, *n)),
        Value::Real(n) => Ok(JSValue::from_rust(ctx, *n)),
        Value::Text(s) => Ok(JSValue::from_rust(ctx, s.as_str())),
        Value::Blob(b) => {
            let ab = JSArrayBuffer::from_bytes(ctx, b)
                .map_err(|e| sqlite_error(format!("ArrayBuffer: {}", e)))?;
            Ok(JSValue::from_rust(ctx, ab))
        }
    }
}

/// Shared query helper — executes SQL and returns rows as array of objects.
pub(crate) fn query_rows(
    ctx: &JSContext,
    conn: &rusqlite::Connection,
    sql: &str,
    params: &Optional<JSArray>,
) -> JSResult<JSArray> {
    let values = js_array_to_params(params)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = values
        .iter()
        .map(|v| v as &dyn rusqlite::types::ToSql)
        .collect();

    let mut stmt = conn
        .prepare_cached(sql)
        .map_err(|e| sqlite_error(e.to_string()))?;

    let col_count = stmt.column_count();
    let col_names: Vec<String> = (0..col_count)
        .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
        .collect();

    let rows = stmt
        .query_map(param_refs.as_slice(), |row| {
            let mut values = Vec::with_capacity(col_count);
            for i in 0..col_count {
                values.push(row.get::<_, Value>(i)?);
            }
            Ok(values)
        })
        .map_err(|e| sqlite_error(e.to_string()))?;

    let result = JSArray::new(ctx)?;
    for row in rows {
        let row = row.map_err(|e| sqlite_error(e.to_string()))?;
        let obj = JSObject::new(ctx);
        for (i, val) in row.iter().enumerate() {
            set_sqlite_value(ctx, &obj, &col_names[i], val)?;
        }
        result.push(JSValue::from_rust(ctx, obj))?;
    }
    Ok(result)
}

pub fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_hidden_class::<SQLite>()?;
    ctx.register_hidden_class::<Statement>()?;
    register_sqlite_namespace(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;

    #[test]
    fn test_sqlite() {
        let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        std::env::set_current_dir(&workspace_root).expect("set cwd");

        async_run!(|ctx: JSContext| async move {
            rong_console::init(&ctx)?;
            rong_assert::init(&ctx)?;
            init(&ctx)?;

            let passed = UnitJSRunner::load_script(&ctx, "sqlite.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }
}
