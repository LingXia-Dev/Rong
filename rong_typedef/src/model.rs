//! Descriptor model for a module's JS-facing API surface.
//!
//! [`crate::extract`] builds these from `syn` AST (mapping Rust types to TS via
//! [`crate::map`]); [`crate::render`] renders them to `.ts`. The model is
//! engine-free and drives generation for any crate using the `#[js_*]` macros —
//! rong's modules or a downstream crate such as `lingxia-logic`.

/// One JS-facing item exported by a module.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// A `#[js_class]` type exposed as a JS class.
    Class(ClassDef),
    /// A numeric constant enum exposed as a JS object and TS literal union.
    NumericEnum(NumericEnumDef),
    /// An interface generated from a `#[derive(FromJSObject)]` / `IntoJSObject`
    /// struct, or declared via a `ts_type` escape hatch.
    Interface(InterfaceDef),
    /// Functions and values registered on a global namespace such as `Rong`.
    Namespace(NamespaceDef),
    /// A TypeScript-only alias whose source of truth lives beside the runtime bindings.
    TypeAlias(TypeAliasDef),
}

/// A TypeScript type alias with no one-to-one backing Rust type.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeAliasDef {
    pub name: String,
    pub ts_type: String,
    pub docs: Vec<String>,
}

/// A global JavaScript namespace represented by an exact mergeable TypeScript
/// interface name (for example `RongNamespace` or `Lx`).
#[derive(Debug, Clone, PartialEq)]
pub struct NamespaceDef {
    pub name: String,
    pub members: Vec<NamespaceMember>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NamespaceMember {
    Function(FunctionDef),
    Value(NamespaceValueDef),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDef {
    pub name: String,
    pub docs: Vec<String>,
    pub sig: FnSig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NamespaceValueDef {
    pub name: String,
    pub ts_type: String,
}

/// A numeric constant enum (`export declare const Name` + `export type Name`).
#[derive(Debug, Clone, PartialEq)]
pub struct NumericEnumDef {
    pub name: String,
    pub docs: Vec<String>,
    pub variants: Vec<NumericEnumVariant>,
}

/// One constant enum variant.
#[derive(Debug, Clone, PartialEq)]
pub struct NumericEnumVariant {
    pub name: String,
    pub value: u32,
    pub docs: Vec<String>,
}

/// A JS class (`export declare class Name { … }`).
#[derive(Debug, Clone, PartialEq)]
pub struct ClassDef {
    /// TS class name (after any `rename`).
    pub name: String,
    /// Doc lines (from `///`), rendered as a JSDoc block.
    pub docs: Vec<String>,
    /// Constructor signature, if the class has a callable one.
    pub constructor: Option<FnSig>,
    /// Constructor doc lines, rendered immediately before its declaration.
    pub constructor_docs: Vec<String>,
    /// The binding declares `#[js_method(constructor, private)]`, so the class
    /// renders `private constructor()` and instances come from a factory.
    pub private_constructor: bool,
    /// Instance/static members, in declaration order.
    pub members: Vec<Member>,
}

/// A single class member.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub kind: MemberKind,
    pub name: String,
    pub docs: Vec<String>,
    pub sig: FnSig,
}

/// The role of a class member, which controls how it renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    Method,
    StaticMethod,
    /// A getter with no matching setter renders as `readonly`.
    Getter,
    /// A getter that also has a setter (rendered as a plain property).
    Property,
}

/// A callable signature, already lowered to TS types.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FnSig {
    pub params: Vec<Param>,
    /// TS return type (already unwrapped from `JSResult`; `Promise<…>` when
    /// the Rust fn is `async`).
    pub ret: String,
    /// Verbatim TS parameter list from a `ts_params` escape hatch. When set, it
    /// replaces [`params`] during rendering (used where inferred types are too
    /// coarse, e.g. a precise union).
    pub raw_params: Option<String>,
}

/// One parameter of a signature.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    /// TS type text.
    pub ts_type: String,
    /// Renders a trailing `?` and omits the type's own `| undefined`.
    pub optional: bool,
    /// A variadic (`Rest<T>`): renders as `...name: T[]`.
    pub rest: bool,
}

/// A generated TS interface.
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceDef {
    pub name: String,
    pub docs: Vec<String>,
    pub fields: Vec<Field>,
}

/// One interface field.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub ts_type: String,
    pub optional: bool,
    pub readonly: bool,
    pub docs: Vec<String>,
}

/// All items contributed by one module (e.g. `rong_sqlite`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModuleTypeDef {
    /// Module identifier (matches the registry name, e.g. `"sqlite"`).
    pub module: String,
    pub items: Vec<Item>,
}
