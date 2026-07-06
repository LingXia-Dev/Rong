// Irreducible TS for the sqlite module: types with no backing Rust struct.
// These are authored here (their origin), not mirrored from Rust.

/** A value bindable as a SQLite statement parameter. */
export type SQLiteParam = null | boolean | number | bigint | string | ArrayBuffer | Uint8Array;
export type SQLiteParams = SQLiteParam[];

/** Result of a write (INSERT/UPDATE/DELETE). */
export interface RunResult {
  /** Number of rows changed. */
  changes: number;
  /** Row id of the last inserted row; large values come back as `bigint`. */
  lastInsertRowid: number | bigint;
}
