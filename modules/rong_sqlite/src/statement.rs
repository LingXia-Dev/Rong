use crate::{
    SharedConn, js_array_to_params, query_rows, set_sqlite_value, sqlite_error, sqlite_value_to_js,
};
use rong::function::Optional;
use rong::*;
use rusqlite::types::Value;

/// Prepared statement. Shares the SQLite connection via Rc.
#[js_export]
pub struct Statement {
    conn: SharedConn,
    sql: String,
    finalized: bool,
}

#[js_class]
impl Statement {
    #[js_method(constructor)]
    fn new() -> JSResult<Self> {
        rong::illegal_constructor("Not allowed 'new Statement()'. Use db.prepare(sql) instead.")
    }

    pub(crate) fn create(conn: SharedConn, sql: String) -> Self {
        Self {
            conn,
            sql,
            finalized: false,
        }
    }

    /// Execute the statement. Returns `{ changes, lastInsertRowid }`.
    #[js_method(ts_return = "RunResult", ts_args = "params?: SQLiteParams")]
    fn run(&self, ctx: JSContext, params: Optional<JSArray>) -> JSResult<JSObject> {
        self.check_finalized()?;
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
            .execute(&self.sql, param_refs.as_slice())
            .map_err(|e| sqlite_error(e.to_string()))?;

        let result = JSObject::new(&ctx);
        result.set("changes", changes as f64)?;
        result.set("lastInsertRowid", conn.last_insert_rowid())?;
        Ok(result)
    }

    /// Execute and return all rows as array of objects.
    #[js_method(ts_return = "Record<string, any>[]", ts_args = "params?: SQLiteParams")]
    fn all(&self, ctx: JSContext, params: Optional<JSArray>) -> JSResult<JSArray> {
        self.check_finalized()?;
        let borrow = self.conn.borrow();
        let conn = borrow
            .as_ref()
            .ok_or_else(|| sqlite_error("Database is closed"))?;
        query_rows(&ctx, conn, &self.sql, &params)
    }

    /// Execute and return the first row, or null if no match.
    #[js_method(
        ts_return = "Record<string, any> | null",
        ts_args = "params?: SQLiteParams"
    )]
    fn get(&self, ctx: JSContext, params: Optional<JSArray>) -> JSResult<JSValue> {
        self.check_finalized()?;
        let borrow = self.conn.borrow();
        let conn = borrow
            .as_ref()
            .ok_or_else(|| sqlite_error("Database is closed"))?;

        let values = js_array_to_params(&params)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = values
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();

        let mut stmt = conn
            .prepare_cached(&self.sql)
            .map_err(|e| sqlite_error(e.to_string()))?;

        let col_count = stmt.column_count();
        let col_names: Vec<String> = (0..col_count)
            .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
            .collect();

        let result = stmt.query_row(param_refs.as_slice(), |row| {
            let mut values = Vec::with_capacity(col_count);
            for i in 0..col_count {
                values.push(row.get::<_, Value>(i)?);
            }
            Ok(values)
        });

        match result {
            Ok(row) => {
                let obj = JSObject::new(&ctx);
                for (i, val) in row.iter().enumerate() {
                    set_sqlite_value(&ctx, &obj, &col_names[i], val)?;
                }
                Ok(JSValue::from_rust(&ctx, obj))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(JSValue::null(&ctx)),
            Err(e) => Err(sqlite_error(e.to_string())),
        }
    }

    /// Execute and return all rows as arrays of values (column-order tuples).
    #[js_method(ts_return = "any[][]", ts_args = "params?: SQLiteParams")]
    fn values(&self, ctx: JSContext, params: Optional<JSArray>) -> JSResult<JSArray> {
        self.check_finalized()?;
        let borrow = self.conn.borrow();
        let conn = borrow
            .as_ref()
            .ok_or_else(|| sqlite_error("Database is closed"))?;

        let param_values = js_array_to_params(&params)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();

        let mut stmt = conn
            .prepare_cached(&self.sql)
            .map_err(|e| sqlite_error(e.to_string()))?;

        let col_count = stmt.column_count();

        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                let mut values = Vec::with_capacity(col_count);
                for i in 0..col_count {
                    values.push(row.get::<_, Value>(i)?);
                }
                Ok(values)
            })
            .map_err(|e| sqlite_error(e.to_string()))?;

        let result = JSArray::new(&ctx)?;
        for row in rows {
            let row = row.map_err(|e| sqlite_error(e.to_string()))?;
            let arr = JSArray::new(&ctx)?;
            for val in &row {
                arr.push(sqlite_value_to_js(&ctx, val)?)?;
            }
            result.push(arr)?;
        }
        Ok(result)
    }

    /// Mark the statement as finalized. Further calls will throw.
    #[js_method]
    fn finalize(&mut self) -> JSResult<()> {
        self.finalized = true;
        Ok(())
    }

    /// The SQL text of this statement.
    #[js_method(getter)]
    fn sql(&self) -> String {
        self.sql.clone()
    }

    fn check_finalized(&self) -> JSResult<()> {
        if self.finalized {
            return Err(sqlite_error("Statement has been finalized"));
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
