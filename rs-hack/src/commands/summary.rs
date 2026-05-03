//! `summary` command: print a module inventory for a single .rs file.

use std::path::PathBuf;

use anyhow::{Context, Result};

#[derive(Debug)]
pub struct SummaryReport {
    pub path: PathBuf,
    pub module_doc: Option<String>,
    pub public_items: Vec<String>,
    pub struct_count: usize,
    pub enum_count: usize,
    pub type_alias_count: usize,
    pub function_names: Vec<String>,
    pub reexports: Vec<String>,
}

pub fn run(path: &PathBuf) -> Result<SummaryReport> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {:?}", path))?;

    let syntax =
        syn::parse_file(&content).with_context(|| format!("Failed to parse file: {:?}", path))?;

    // Module-level doc: inner doc attrs (//! or #![doc = ...])
    let mut module_doc_parts: Vec<String> = Vec::new();
    for attr in &syntax.attrs {
        if let syn::AttrStyle::Inner(_) = attr.style {
            if attr.path().is_ident("doc") {
                if let Ok(syn::MetaNameValue {
                    value:
                        syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Str(s),
                            ..
                        }),
                    ..
                }) = attr.meta.require_name_value().cloned()
                {
                    let text = s.value().trim().to_string();
                    if !text.is_empty() {
                        module_doc_parts.push(text);
                    }
                }
            }
        }
    }
    let module_doc = if module_doc_parts.is_empty() {
        None
    } else {
        Some(module_doc_parts.join(" "))
    };

    let mut public_items: Vec<String> = Vec::new();
    let mut struct_count = 0usize;
    let mut enum_count = 0usize;
    let mut type_alias_count = 0usize;
    let mut function_names: Vec<String> = Vec::new();
    let mut reexports: Vec<String> = Vec::new();

    for item in &syntax.items {
        match item {
            syn::Item::Struct(s) => {
                struct_count += 1;
                if is_public(&s.vis) {
                    public_items.push(s.ident.to_string());
                }
            }
            syn::Item::Enum(e) => {
                enum_count += 1;
                if is_public(&e.vis) {
                    public_items.push(e.ident.to_string());
                }
            }
            syn::Item::Type(t) => {
                type_alias_count += 1;
                if is_public(&t.vis) {
                    public_items.push(t.ident.to_string());
                }
            }
            syn::Item::Fn(f) => {
                function_names.push(f.sig.ident.to_string());
                if is_public(&f.vis) {
                    public_items.push(f.sig.ident.to_string());
                }
            }
            syn::Item::Trait(t) => {
                if is_public(&t.vis) {
                    public_items.push(t.ident.to_string());
                }
            }
            syn::Item::Const(c) => {
                if is_public(&c.vis) {
                    public_items.push(c.ident.to_string());
                }
            }
            syn::Item::Static(s) => {
                if is_public(&s.vis) {
                    public_items.push(s.ident.to_string());
                }
            }
            syn::Item::Mod(m) => {
                if is_public(&m.vis) {
                    public_items.push(m.ident.to_string());
                }
            }
            syn::Item::Use(u) => {
                if is_public(&u.vis) {
                    let tokens = quote::quote!(#u);
                    reexports.push(
                        tokens
                            .to_string()
                            .replace(" :: ", "::")
                            .replace(" as ", " as "),
                    );
                }
            }
            _ => {}
        }
    }

    Ok(SummaryReport {
        path: path.clone(),
        module_doc,
        public_items,
        struct_count,
        enum_count,
        type_alias_count,
        function_names,
        reexports,
    })
}

pub fn render(report: &SummaryReport) {
    println!("Module: {}", report.path.display());

    if report.public_items.is_empty() {
        println!("Public items: (none)");
    } else {
        println!("Public items: {}", report.public_items.join(", "));
    }

    println!(
        "Types: {} struct{}, {} enum{}, {} type alias{}",
        report.struct_count,
        if report.struct_count == 1 { "" } else { "s" },
        report.enum_count,
        if report.enum_count == 1 { "" } else { "s" },
        report.type_alias_count,
        if report.type_alias_count == 1 {
            ""
        } else {
            "es"
        },
    );

    if report.function_names.is_empty() {
        println!("Functions: (none)");
    } else {
        println!("Functions: {}", report.function_names.join(", "));
    }

    if report.reexports.is_empty() {
        println!("Re-exports: (none)");
    } else {
        for r in &report.reexports {
            println!("Re-exports: {}", r);
        }
    }

    match &report.module_doc {
        Some(doc) => println!("Doc: {:?}", doc),
        None => println!("Doc: (none)"),
    }
}

fn is_public(vis: &syn::Visibility) -> bool {
    matches!(
        vis,
        syn::Visibility::Public(_) | syn::Visibility::Restricted(_)
    )
}
