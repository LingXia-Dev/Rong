//! Shared declaration syntax and TypeScript type-definition support for RongJS
//! bindings.
//!
//! [`api`] is the single `js_api!` syntax model used by both `rong_macro` and
//! the `rong-typegen` CLI, keeping runtime registration and published types aligned.
//! The `typegen` feature provides the engine-free type-generation
//! core, driven entirely from parsed Rust source:
//!
//! - [`model`] — a descriptor of a module's JS-facing API.
//! - [`map`] — the Rust→TypeScript type mapping.
//! - [`extract`] — `syn` AST → [`model`] descriptors.
//! - [`render`] — renders descriptors to `.ts` declaration text.
//!
//! Because it only reads source, generation works for any crate that uses the
//! `#[js_*]` macros — rong's modules or a downstream crate such as
//! `lingxia-logic` — without touching the macro or the runtime build graph.

pub mod api;
pub mod attributes;
#[cfg(feature = "typegen")]
pub mod extract;
#[cfg(feature = "typegen")]
pub mod map;
#[cfg(feature = "typegen")]
pub mod model;
#[cfg(feature = "typegen")]
pub mod render;

pub use api::{ClassExport, ConstExport, FunctionExport, JsApiEntry, JsApiInput, TypeAliasExport};
pub use attributes::{
    JsClassOptions, JsDefault, JsFieldOptions, JsMethodOptions, js_class_options, js_field_options,
    js_method_options, parse_js_class_args, path_last_is, u32_integer_expr,
};
#[cfg(feature = "typegen")]
pub use extract::{
    extract_function, extract_impl, extract_numeric_enum, extract_struct, extract_union,
    has_orphan_js_methods,
};
#[cfg(feature = "typegen")]
pub use map::{TsType, is_injected, map_return, rust_type_to_ts};
#[cfg(feature = "typegen")]
pub use model::{
    ClassDef, Field, FnSig, FunctionDef, InterfaceDef, Item, Member, MemberKind, ModuleTypeDef,
    NamespaceDef, NamespaceMember, NamespaceValueDef, Param, TypeAliasDef,
};
#[cfg(feature = "typegen")]
pub use render::render_module;

/// DOM-free standard globals installed by Rong's Logic runtime profile.
#[cfg(feature = "typegen")]
pub const LOGIC_WEB_PROFILE: &str = include_str!("../profiles/logic-web.d.ts");
