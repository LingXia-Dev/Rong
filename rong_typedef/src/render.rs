//! Render a [`ModuleTypeDef`] to TypeScript declaration text.

use crate::model::{
    ClassDef, ConstEnumDef, Field, InterfaceDef, Item, MemberKind, ModuleTypeDef, Param,
    TypeAliasDef,
};
use std::fmt::Write as _;

/// Render a module's items to `.ts` source (interfaces and type aliases first,
/// then classes — so referenced types are declared before use).
pub fn render_module(def: &ModuleTypeDef) -> String {
    let mut out = String::new();

    for item in &def.items {
        if let Item::TypeAlias(a) = item {
            render_type_alias(&mut out, a);
        }
    }
    for item in &def.items {
        if let Item::Interface(i) = item {
            render_interface(&mut out, i);
        }
    }
    for item in &def.items {
        if let Item::ConstEnum(e) = item {
            render_const_enum(&mut out, e);
        }
    }
    for item in &def.items {
        if let Item::Class(c) = item {
            render_class(&mut out, c);
        }
    }

    out
}

fn render_const_enum(out: &mut String, e: &ConstEnumDef) {
    render_docs(out, &e.docs, "");
    let _ = writeln!(out, "export declare const {}: {{", e.name);
    for variant in &e.variants {
        render_docs(out, &variant.docs, "  ");
        let _ = writeln!(out, "  readonly {}: {};", variant.name, variant.value);
    }
    let _ = writeln!(out, "}};");
    let _ = writeln!(
        out,
        "export type {} = (typeof {})[keyof typeof {}];\n",
        e.name, e.name, e.name
    );
}

fn render_type_alias(out: &mut String, a: &TypeAliasDef) {
    render_docs(out, &a.docs, "");
    let _ = writeln!(out, "export type {} = {};\n", a.name, a.value);
}

fn render_interface(out: &mut String, i: &InterfaceDef) {
    render_docs(out, &i.docs, "");
    let _ = writeln!(out, "export interface {} {{", i.name);
    for field in &i.fields {
        render_field(out, field);
    }
    let _ = writeln!(out, "}}\n");
}

fn render_field(out: &mut String, f: &Field) {
    render_docs(out, &f.docs, "  ");
    let readonly = if f.readonly { "readonly " } else { "" };
    let opt = if f.optional { "?" } else { "" };
    let _ = writeln!(out, "  {readonly}{}{opt}: {};", f.name, f.ts_type);
}

fn render_class(out: &mut String, c: &ClassDef) {
    render_docs(out, &c.docs, "");
    let _ = writeln!(out, "export declare class {} {{", c.name);

    if c.private_constructor {
        let _ = writeln!(out, "  private constructor();");
    } else if let Some(ctor) = &c.constructor {
        let _ = writeln!(out, "  constructor({});", params_text(ctor));
    }

    for member in &c.members {
        render_docs(out, &member.docs, "  ");
        match member.kind {
            MemberKind::Getter => {
                let _ = writeln!(out, "  readonly {}: {};", member.name, member.sig.ret);
            }
            MemberKind::Property => {
                let _ = writeln!(out, "  {}: {};", member.name, member.sig.ret);
            }
            MemberKind::Method | MemberKind::StaticMethod => {
                let stat = if member.kind == MemberKind::StaticMethod {
                    "static "
                } else {
                    ""
                };
                let _ = writeln!(
                    out,
                    "  {stat}{}({}): {};",
                    member.name,
                    params_text(&member.sig),
                    member.sig.ret
                );
            }
        }
    }

    let _ = writeln!(out, "}}\n");
}

/// A signature's parameter list: the `ts_args` override verbatim if present,
/// otherwise the mapped params.
fn params_text(sig: &crate::model::FnSig) -> String {
    match &sig.raw_params {
        Some(raw) => raw.clone(),
        None => render_params(&sig.params),
    }
}

fn render_params(params: &[Param]) -> String {
    params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if p.rest {
                return format!("...{}: {}", p.name, p.ts_type);
            }
            // TS forbids a required param after an optional one, so an optional
            // param that is *not* trailing renders `T | undefined` instead of `?`.
            let trailing = params[i + 1..].iter().all(|q| q.optional || q.rest);
            if p.optional && trailing {
                format!("{}?: {}", p.name, p.ts_type)
            } else if p.optional {
                format!("{}: {} | undefined", p.name, p.ts_type)
            } else {
                format!("{}: {}", p.name, p.ts_type)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Render doc lines as a JSDoc block at the given indent. A single line renders
/// as `/** … */`; multiple lines as a multi-line block. No output when empty.
fn render_docs(out: &mut String, docs: &[String], indent: &str) {
    // Escape any `*/` so a doc line can't terminate the JSDoc block early.
    let docs: Vec<String> = docs
        .iter()
        .map(|d| d.trim().replace("*/", "*\\/"))
        .filter(|d| !d.is_empty())
        .collect();
    match docs.as_slice() {
        [] => {}
        [single] => {
            let _ = writeln!(out, "{indent}/** {single} */");
        }
        many => {
            let _ = writeln!(out, "{indent}/**");
            for line in many {
                let _ = writeln!(out, "{indent} * {line}");
            }
            let _ = writeln!(out, "{indent} */");
        }
    }
}
