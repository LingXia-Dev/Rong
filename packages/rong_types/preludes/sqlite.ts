// Irreducible TS for the sqlite module: parameter unions with no backing Rust
// struct. Authored here (their origin), not mirrored from Rust.

/** A value bindable as a SQLite statement parameter. */
export type SQLiteParam = null | boolean | number | bigint | string | ArrayBuffer | Uint8Array;
export type SQLiteParams = SQLiteParam[];
