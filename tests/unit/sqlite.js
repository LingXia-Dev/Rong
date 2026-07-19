function tempDbPath(prefix = "rong_sqlite") {
  const rand = Math.random().toString(16).slice(2);
  return `target/${prefix}_${Date.now()}_${rand}.db`;
}

describe("SQLite — construction", () => {
  test("hides Statement from global scope", () => {
    assert.equal(typeof Statement, "undefined");
    assert.equal(globalThis.Statement, undefined);

    const db = new Rong.SQLite();
    const stmt = db.prepare("SELECT 1");
    let failed = false;
    try {
      new stmt.constructor();
    } catch (e) {
      failed = true;
    }
    assert(failed, "Statement should not be constructible via instance.constructor");
    db.close();
  });

  test("does not expose SQLite on globalThis", () => {
    assert.equal(typeof SQLite, "undefined");
    assert.equal(globalThis.SQLite, undefined);
    assert.equal(typeof Rong.SQLite, "function");
  });

  test("opens in-memory database by default", () => {
    const db = new Rong.SQLite();
    assert.equal(db.filename, ":memory:");
    assert.equal(db.inTransaction, false);
    db.close();
  });

  test("opens in-memory database with explicit :memory:", () => {
    const db = new Rong.SQLite(":memory:");
    assert.equal(db.filename, ":memory:");
    db.close();
  });

  test("is instance of SQLite", () => {
    const db = new Rong.SQLite();
    assert(db instanceof Rong.SQLite);
    db.close();
  });

  test("close is idempotent", () => {
    const db = new Rong.SQLite();
    db.close();
    db.close();
  });
});

describe("SQLite — file-backed", () => {
  test("persists data across reopen", () => {
    const filename = tempDbPath();

    let db = new Rong.SQLite(filename);
    db.exec("CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT)");
    db.run("INSERT INTO users (name) VALUES (?)", ["Alice"]);
    db.close();

    db = new Rong.SQLite(filename);
    const rows = db.query("SELECT * FROM users");
    assert.equal(rows.length, 1);
    assert.equal(rows[0].name, "Alice");
    db.close();
  });
});

describe("SQLite — exec", () => {
  let db;
  beforeEach(() => { db = new Rong.SQLite(); });
  afterEach(() => db.close());

  test("creates tables", () => {
    db.exec(`
      CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, age INTEGER, score REAL);
      CREATE TABLE logs (id INTEGER PRIMARY KEY AUTOINCREMENT, msg TEXT);
    `);
    // verify tables exist by querying
    const rows = db.query("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name");
    assert.equal(rows.length, 2);
    assert.equal(rows[0].name, "logs");
    assert.equal(rows[1].name, "users");
  });

  test("throws on invalid SQL", () => {
    let threw = false;
    try { db.exec("NOT VALID SQL"); } catch (e) { threw = true; }
    assert(threw, "should throw on invalid SQL");
  });

  test("prepare throws on invalid SQL", () => {
    let threw = false;
    try { db.prepare("SELECT FROM"); } catch (e) { threw = true; }
    assert(threw, "prepare should throw on invalid SQL");
  });
});

describe("SQLite — run & query", () => {
  let db;
  beforeEach(() => {
    db = new Rong.SQLite();
    db.exec("CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, age INTEGER, score REAL)");
  });
  afterEach(() => db.close());

  test("inserts and returns changes", () => {
    const r = db.run("INSERT INTO users (name, age, score) VALUES (?, ?, ?)", ["Alice", 30, 95.5]);
    assert.equal(r.changes, 1);
    assert.equal(r.lastInsertRowid, 1);
  });

  test("queries all rows as objects", () => {
    db.run("INSERT INTO users (name, age, score) VALUES (?, ?, ?)", ["Alice", 30, 95.5]);
    db.run("INSERT INTO users (name, age, score) VALUES (?, ?, ?)", ["Bob", 25, 88.0]);
    db.run("INSERT INTO users (name, age, score) VALUES (?, ?, ?)", ["Charlie", 35, 72.3]);

    const rows = db.query("SELECT * FROM users ORDER BY id");
    assert.equal(rows.length, 3);
    assert.equal(rows[0].name, "Alice");
    assert.equal(rows[0].age, 30);
    assert.equal(rows[0].score, 95.5);
    assert.equal(rows[1].name, "Bob");
    assert.equal(rows[2].name, "Charlie");
  });

  test("queries with parameters", () => {
    db.run("INSERT INTO users (name, age, score) VALUES (?, ?, ?)", ["Alice", 30, 95.5]);
    db.run("INSERT INTO users (name, age, score) VALUES (?, ?, ?)", ["Bob", 25, 88.0]);
    db.run("INSERT INTO users (name, age, score) VALUES (?, ?, ?)", ["Charlie", 35, 72.3]);

    const filtered = db.query("SELECT name FROM users WHERE age > ?", [28]);
    assert.equal(filtered.length, 2);
    assert.equal(filtered[0].name, "Alice");
    assert.equal(filtered[1].name, "Charlie");
  });

  test("returns empty array for no results", () => {
    const rows = db.query("SELECT * FROM users WHERE age > ?", [100]);
    assert.equal(rows.length, 0);
  });
});

describe("SQLite — prepared statements", () => {
  let db;
  beforeEach(() => {
    db = new Rong.SQLite();
    db.exec("CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, age INTEGER)");
    db.run("INSERT INTO users (name, age) VALUES (?, ?)", ["Alice", 30]);
    db.run("INSERT INTO users (name, age) VALUES (?, ?)", ["Bob", 25]);
    db.run("INSERT INTO users (name, age) VALUES (?, ?)", ["Charlie", 35]);
  });
  afterEach(() => db.close());

  test("stmt.all returns all matching rows", () => {
    const stmt = db.prepare("SELECT * FROM users WHERE age >= ?");
    assert.equal(stmt.sql, "SELECT * FROM users WHERE age >= ?");
    const rows = stmt.all([30]);
    assert.equal(rows.length, 2);
    assert.equal(rows[0].name, "Alice");
    assert.equal(rows[1].name, "Charlie");
  });

  test("stmt.get returns first row or null", () => {
    const stmt = db.prepare("SELECT * FROM users WHERE age >= ?");
    const one = stmt.get([25]);
    assert.equal(one.name, "Alice");

    const none = stmt.get([100]);
    assert.equal(none, null);
  });

  test("stmt.values returns arrays of values", () => {
    const stmt = db.prepare("SELECT id, name FROM users ORDER BY id");
    const vals = stmt.values();
    assert.equal(vals.length, 3);
    assert.equal(vals[0][0], 1);
    assert.equal(vals[0][1], "Alice");
    assert.equal(vals[1][0], 2);
    assert.equal(vals[1][1], "Bob");
  });

  test("stmt.run executes and returns changes", () => {
    const stmt = db.prepare("INSERT INTO users (name, age) VALUES (?, ?)");
    const r = stmt.run(["Dave", 28]);
    assert.equal(r.changes, 1);
    assert.equal(db.query("SELECT * FROM users").length, 4);
  });

  test("throws after finalize", () => {
    const stmt = db.prepare("SELECT 1");
    stmt.finalize();
    let threw = false;
    try { stmt.all(); } catch (e) {
      threw = true;
      assert(e.message.includes("finalized"));
    }
    assert(threw, "should throw after finalize");
  });
});

describe("SQLite — transactions", () => {
  let db;
  beforeEach(() => {
    db = new Rong.SQLite();
    db.exec("CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT)");
  });
  afterEach(() => db.close());

  test("commits on success", () => {
    db.transaction(() => {
      db.run("INSERT INTO users (name) VALUES (?)", ["Alice"]);
      db.run("INSERT INTO users (name) VALUES (?)", ["Bob"]);
    });
    assert.equal(db.query("SELECT * FROM users").length, 2);
  });

  test("rolls back on error", () => {
    let threw = false;
    try {
      db.transaction(() => {
        db.run("INSERT INTO users (name) VALUES (?)", ["Fail"]);
        throw new Error("rollback please");
      });
    } catch (e) { threw = true; }
    assert(threw);
    assert.equal(db.query("SELECT * FROM users").length, 0, "rolled-back insert should not persist");
  });

  test("inTransaction reflects state", () => {
    assert.equal(db.inTransaction, false);
  });

  test("inTransaction is true inside the callback and resets after commit", () => {
    let seenInside = false;
    db.transaction(() => {
      seenInside = true;
      assert.equal(db.inTransaction, true);
      db.run("INSERT INTO users (name) VALUES (?)", ["Inside"]);
    });

    assert.equal(seenInside, true);
    assert.equal(db.inTransaction, false);
  });

  test("inTransaction resets after rollback", () => {
    try {
      db.transaction(() => {
        assert.equal(db.inTransaction, true);
        throw new Error("rollback");
      });
    } catch (e) {}

    assert.equal(db.inTransaction, false);
  });

});

describe("SQLite — type handling", () => {
  let db;
  beforeEach(() => { db = new Rong.SQLite(); });
  afterEach(() => db.close());

  test("handles NULL values", () => {
    db.exec("CREATE TABLE t (id INTEGER PRIMARY KEY, val INTEGER)");
    db.run("INSERT INTO t (val) VALUES (?)", [null]);
    const row = db.query("SELECT * FROM t")[0];
    assert.equal(row.val, null);
  });

  test("handles boolean as integer", () => {
    db.exec("CREATE TABLE flags (id INTEGER PRIMARY KEY, active INTEGER)");
    db.run("INSERT INTO flags (active) VALUES (?)", [true]);
    db.run("INSERT INTO flags (active) VALUES (?)", [false]);
    const rows = db.query("SELECT * FROM flags ORDER BY id");
    assert.equal(rows[0].active, 1);
    assert.equal(rows[1].active, 0);
  });

  test("handles BLOB with Uint8Array", () => {
    db.exec("CREATE TABLE blobs (id INTEGER PRIMARY KEY, data BLOB)");
    const input = new Uint8Array([0xDE, 0xAD, 0xBE, 0xEF]);
    db.run("INSERT INTO blobs (data) VALUES (?)", [input]);
    const row = db.query("SELECT * FROM blobs")[0];
    assert(row.data instanceof ArrayBuffer, "BLOB should return ArrayBuffer");
    const view = new Uint8Array(row.data);
    assert.equal(view[0], 0xDE);
    assert.equal(view[3], 0xEF);
  });

  test("handles BLOB with ArrayBuffer", () => {
    db.exec("CREATE TABLE blobs (id INTEGER PRIMARY KEY, data BLOB)");
    const input = new Uint8Array([1, 2, 3, 4]).buffer;
    db.run("INSERT INTO blobs (data) VALUES (?)", [input]);
    const row = db.query("SELECT * FROM blobs")[0];
    const view = new Uint8Array(row.data);
    assert.equal(view[0], 1);
    assert.equal(view[3], 4);
  });

  test("preserves large INTEGER values as bigint", () => {
    db.exec("CREATE TABLE ids (id INTEGER PRIMARY KEY, label TEXT)");
    const bigId = 9007199254740993n;

    const result = db.run("INSERT INTO ids (id, label) VALUES (?, ?)", [bigId, "large"]);
    assert.equal(typeof result.lastInsertRowid, "bigint");
    assert.equal(result.lastInsertRowid, bigId);

    const row = db.query("SELECT id FROM ids")[0];
    assert.equal(typeof row.id, "bigint");
    assert.equal(row.id, bigId);

    const stmt = db.prepare("SELECT id FROM ids WHERE id = ?");
    const selected = stmt.get([bigId]);
    assert.equal(typeof selected.id, "bigint");
    assert.equal(selected.id, bigId);
  });

  test("rejects bigint values outside SQLite INTEGER range", () => {
    db.exec("CREATE TABLE ids (id INTEGER PRIMARY KEY, label TEXT)");

    let threw = false;
    try {
      db.run("INSERT INTO ids (id, label) VALUES (?, ?)", [9223372036854775808n, "too-large"]);
    } catch (e) {
      threw = true;
      assert(e.message.includes("out of SQLite INTEGER range"));
    }

    assert(threw, "should reject bigint values outside SQLite's INTEGER range");
  });

  test("stores numeric values outside INTEGER range as REAL", () => {
    db.exec("CREATE TABLE values_test (value)");
    const value = 2 ** 63;
    db.run("INSERT INTO values_test (value) VALUES (?)", [value]);
    const row = db.query(
      "SELECT value, typeof(value) AS storage_type FROM values_test",
    )[0];
    assert.equal(row.storage_type, "real");
    assert.equal(typeof row.value, "number");
    assert.equal(row.value, value);
  });
});

describe("SQLite — error handling", () => {
  test("throws on closed database", () => {
    const db = new Rong.SQLite();
    db.close();
    let threw = false;
    try { db.exec("SELECT 1"); } catch (e) {
      threw = true;
      assert(e.message.includes("closed"));
    }
    assert(threw, "should throw on closed db");
  });

  test("throws on unsupported parameter types", () => {
    const db = new Rong.SQLite();
    db.exec("CREATE TABLE t (id INTEGER PRIMARY KEY, val TEXT)");
    let threw = false;
    try {
      db.run("INSERT INTO t (val) VALUES (?)", [{}]);
    } catch (e) {
      threw = true;
      assert(e.message.includes("Unsupported parameter type"));
    } finally {
      db.close();
    }
    assert(threw, "should throw on unsupported param type");
  });

  test("prepared statements throw after database close", () => {
    const db = new Rong.SQLite();
    const stmt = db.prepare("SELECT 1 AS n");
    db.close();

    let threw = false;
    try {
      stmt.all();
    } catch (e) {
      threw = true;
      assert(e.message.includes("closed"));
    }
    assert(threw, "statement should throw after db close");
  });
});
