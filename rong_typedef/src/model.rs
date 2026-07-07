//! Descriptor model for a module's JS-facing API surface.
//!
//! [`crate::extract`] builds these from `syn` AST (mapping Rust types to TS via
//! [`crate::map`]); [`crate::render`] renders them to `.ts`. The model is
//! engine-free and drives generation for any crate using the `#[js_*]` macros —
//! rong's modules or a downstream crate such as `lingxia-logic`.

use serde::{Deserialize, Serialize};

/// One JS-facing item exported by a module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Item {
    /// A `#[js_class]` type exposed as a JS class.
    Class(ClassDef),
    /// A numeric constant enum exposed as a JS object and TS literal union.
    ConstEnum(ConstEnumDef),
    /// An interface generated from a `#[derive(FromJSObj)]` / `IntoJSObj`
    /// struct, or declared via a `ts_type` escape hatch.
    Interface(InterfaceDef),
    /// A standalone function or `export type` alias.
    TypeAlias(TypeAliasDef),
}

/// A numeric constant enum (`export declare const Name` + `export type Name`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstEnumDef {
    pub name: String,
    #[serde(default)]
    pub docs: Vec<String>,
    #[serde(default)]
    pub variants: Vec<ConstEnumVariant>,
}

/// One constant enum variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstEnumVariant {
    pub name: String,
    pub value: u32,
    #[serde(default)]
    pub docs: Vec<String>,
}

/// A JS class (`export declare class Name { … }`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassDef {
    /// TS class name (after any `rename`).
    pub name: String,
    /// Doc lines (from `///`), rendered as a JSDoc block.
    #[serde(default)]
    pub docs: Vec<String>,
    /// Constructor signature, if the class has a callable one.
    #[serde(default)]
    pub constructor: Option<FnSig>,
    /// The Rust constructor rejects direct construction (`illegal_constructor`),
    /// so the class renders `private constructor()` — instances come from a
    /// factory method, never `new`.
    #[serde(default)]
    pub private_constructor: bool,
    /// Instance/static members, in declaration order.
    #[serde(default)]
    pub members: Vec<Member>,
}

/// A single class member.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Member {
    pub kind: MemberKind,
    pub name: String,
    #[serde(default)]
    pub docs: Vec<String>,
    pub sig: FnSig,
}

/// The role of a class member, which controls how it renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberKind {
    Method,
    StaticMethod,
    /// A getter with no matching setter renders as `readonly`.
    Getter,
    /// A getter that also has a setter (rendered as a plain property).
    Property,
}

/// A callable signature, already lowered to TS types.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FnSig {
    #[serde(default)]
    pub params: Vec<Param>,
    /// TS return type (already unwrapped from `JSResult`; `Promise<…>` when
    /// the Rust fn is `async`).
    pub ret: String,
    /// Verbatim TS parameter list from a `ts_args` escape hatch. When set, it
    /// replaces [`params`] during rendering (used where inferred types are too
    /// coarse, e.g. a precise union).
    #[serde(default)]
    pub raw_params: Option<String>,
}

/// One parameter of a signature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    /// TS type text.
    pub ts_type: String,
    /// Renders a trailing `?` and omits the type's own `| undefined`.
    #[serde(default)]
    pub optional: bool,
    /// A variadic (`Rest<T>`): renders as `...name: T[]`.
    #[serde(default)]
    pub rest: bool,
}

/// A generated TS interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterfaceDef {
    pub name: String,
    #[serde(default)]
    pub docs: Vec<String>,
    #[serde(default)]
    pub fields: Vec<Field>,
}

/// One interface field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub ts_type: String,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub readonly: bool,
    #[serde(default)]
    pub docs: Vec<String>,
}

/// A `export type Name = …;` alias.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeAliasDef {
    pub name: String,
    #[serde(default)]
    pub docs: Vec<String>,
    pub value: String,
}

/// All items contributed by one module (e.g. `rong_sqlite`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ModuleTypeDef {
    /// Module identifier (matches the registry name, e.g. `"sqlite"`).
    pub module: String,
    #[serde(default)]
    pub items: Vec<Item>,
}
