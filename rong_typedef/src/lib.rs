//! TypeScript type-definition support for RongJS bindings.
//!
//! This crate is the engine-free core of type generation, driven entirely from
//! parsed Rust source (never from the runtime or the proc-macro):
//!
//! - [`model`] — a descriptor of a module's JS-facing API.
//! - [`map`] — the Rust→TypeScript type mapping.
//! - [`extract`] — `syn` AST → [`model`] descriptors.
//! - [`render`] — renders descriptors to `.ts` declaration text.
//!
//! Because it only reads source, generation works for any crate that uses the
//! `#[js_*]` macros — rong's modules or a downstream crate such as
//! `lingxia-logic` — without touching the macro or the runtime build graph.

pub mod extract;
pub mod map;
pub mod model;
pub mod render;

pub use extract::{extract_const_enum, extract_impl, extract_struct, has_orphan_js_methods};
pub use map::{TsType, is_injected, map_return, rust_type_to_ts};
pub use model::{
    ClassDef, Field, FnSig, InterfaceDef, Item, Member, MemberKind, ModuleTypeDef, Param,
    TypeAliasDef,
};
pub use render::render_module;
