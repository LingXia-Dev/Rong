//! End-to-end: build a descriptor by hand, render it, and round-trip it.

use rong_typedef::model::*;
use rong_typedef::render_module;

fn method(name: &str, docs: &[&str], params: Vec<Param>, ret: &str) -> Member {
    Member {
        kind: MemberKind::Method,
        name: name.into(),
        docs: docs.iter().map(|s| s.to_string()).collect(),
        sig: FnSig {
            params,
            ret: ret.into(),
            ..Default::default()
        },
    }
}

fn param(name: &str, ts_type: &str, optional: bool) -> Param {
    Param {
        name: name.into(),
        ts_type: ts_type.into(),
        optional,
        rest: false,
    }
}

fn sqlite_module() -> ModuleTypeDef {
    ModuleTypeDef {
        module: "sqlite".into(),
        items: vec![
            Item::TypeAlias(TypeAliasDef {
                name: "SQLiteParams".into(),
                docs: vec!["Supported parameter types.".into()],
                value: "SQLiteParam[]".into(),
            }),
            Item::Interface(InterfaceDef {
                name: "RunResult".into(),
                docs: vec!["Result of a write operation.".into()],
                fields: vec![
                    Field {
                        name: "changes".into(),
                        ts_type: "number".into(),
                        optional: false,
                        readonly: false,
                        docs: vec!["Number of rows changed.".into()],
                    },
                    Field {
                        name: "lastInsertRowid".into(),
                        ts_type: "number | bigint".into(),
                        optional: false,
                        readonly: false,
                        docs: vec![],
                    },
                ],
            }),
            Item::Class(ClassDef {
                name: "SQLite".into(),
                docs: vec!["SQLite database connection.".into()],
                constructor: Some(FnSig {
                    params: vec![param("filename", "string", true)],
                    ret: String::new(),
                    ..Default::default()
                }),
                private_constructor: false,
                members: vec![
                    Member {
                        kind: MemberKind::Getter,
                        name: "filename".into(),
                        docs: vec![],
                        sig: FnSig {
                            params: vec![],
                            ret: "string".into(),
                            ..Default::default()
                        },
                    },
                    method(
                        "exec",
                        &["Execute SQL."],
                        vec![param("sql", "string", false)],
                        "void",
                    ),
                    method(
                        "query",
                        &[],
                        vec![
                            param("sql", "string", false),
                            param("params", "SQLiteParams", true),
                        ],
                        "Record<string, any>[]",
                    ),
                ],
            }),
        ],
    }
}

#[test]
fn renders_expected_declarations() {
    let out = render_module(&sqlite_module());
    let expected = "\
/** Supported parameter types. */
export type SQLiteParams = SQLiteParam[];

/** Result of a write operation. */
export interface RunResult {
  /** Number of rows changed. */
  changes: number;
  lastInsertRowid: number | bigint;
}

/** SQLite database connection. */
export declare class SQLite {
  constructor(filename?: string);
  readonly filename: string;
  /** Execute SQL. */
  exec(sql: string): void;
  query(sql: string, params?: SQLiteParams): Record<string, any>[];
}

";
    assert_eq!(out, expected);
}

#[test]
fn descriptor_round_trips_through_json() {
    let def = sqlite_module();
    let json = serde_json::to_string(&def).unwrap();
    let back: ModuleTypeDef = serde_json::from_str(&json).unwrap();
    assert_eq!(def, back);
}

fn one_class(c: ClassDef) -> String {
    render_module(&ModuleTypeDef {
        module: "x".into(),
        items: vec![Item::Class(c)],
    })
}

#[test]
fn private_constructor_renders() {
    let out = one_class(ClassDef {
        name: "Stmt".into(),
        docs: vec![],
        constructor: None,
        private_constructor: true,
        members: vec![],
    });
    assert!(out.contains("private constructor();"), "{out}");
}

#[test]
fn non_trailing_optional_param_uses_union_not_question_mark() {
    let out = one_class(ClassDef {
        name: "C".into(),
        docs: vec![],
        constructor: None,
        private_constructor: false,
        members: vec![method(
            "f",
            &[],
            vec![param("a", "number", true), param("b", "string", false)],
            "void",
        )],
    });
    // `a` is optional but not trailing, so it must not use `?`.
    assert!(
        out.contains("f(a: number | undefined, b: string): void;"),
        "{out}"
    );
}

#[test]
fn jsdoc_escapes_comment_terminator() {
    let out = render_module(&ModuleTypeDef {
        module: "x".into(),
        items: vec![Item::Interface(InterfaceDef {
            name: "I".into(),
            docs: vec!["ratio w/h */ danger".into()],
            fields: vec![],
        })],
    });
    assert!(!out.contains("*/ danger"), "unescaped terminator: {out}");
    assert!(out.contains("*\\/ danger"), "{out}");
}
