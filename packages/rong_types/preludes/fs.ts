// Irreducible TS for the fs module: a pure-TS enum with no Rust struct behind
// it. Authored here (its origin), consumed by FileHandle.seek and Rong.SeekMode.

export enum SeekMode {
  /** Seek from start of file (absolute position). */
  Start = 0,
  /** Seek from current position (relative). */
  Current = 1,
  /** Seek from end of file (usually a negative offset). */
  End = 2,
}
