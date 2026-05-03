//! `doc-coverage` command: report missing doc-comments for public items.

use std::path::PathBuf;

use anyhow::Result;
use syn::spanned::Spanned;

use crate::files::collect_rust_files_with_exclusions;

#[derive(Debug)]
pub struct DocCoverageReport {
    pub missing_items: Vec<MissingDoc>,
    pub total_items: usize,
    pub total_fields: usize,
    pub missing_item_count: usize,
    pub missing_field_count: usize,
}

#[derive(Debug)]
pub struct MissingDoc {
    pub file_path: String,
    pub line: usize,
    pub label: String, // e.g. "AppState (struct)", "RenderedCell::width (field)"
}

pub fn run(paths: &[PathBuf], check_fields: bool, exclude: &[String]) -> Result<DocCoverageReport> {
    let files = collect_rust_files_with_exclusions(paths, exclude)?;

    let mut missing_items: Vec<MissingDoc> = Vec::new();
    let mut total_items = 0usize;
    let mut total_fields = 0usize;
    let mut missing_item_count = 0usize;
    let mut missing_field_count = 0usize;

    for file in &files {
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("⚠️  Skipping {}: {}", file.display(), e);
                continue;
            }
        };
        let syntax = match syn::parse_file(&content) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("⚠️  Skipping {} (parse error): {}", file.display(), e);
                continue;
            }
        };

        let fp = file.to_string_lossy().to_string();

        for item in &syntax.items {
            process_item(
                item,
                &fp,
                check_fields,
                &mut missing_items,
                &mut total_items,
                &mut total_fields,
                &mut missing_item_count,
                &mut missing_field_count,
            );
        }
    }

    Ok(DocCoverageReport {
        missing_items,
        total_items,
        total_fields,
        missing_item_count,
        missing_field_count,
    })
}

pub fn render(report: &DocCoverageReport) {
    println!("Missing docs (items): {}", report.missing_item_count);
    println!("Missing docs (fields): {}", report.missing_field_count);

    if report.missing_items.is_empty() {
        println!("All public items are documented.");
        return;
    }

    // Sort by file then line
    let mut offenders: Vec<&MissingDoc> = report.missing_items.iter().collect();
    offenders.sort_by(|a, b| a.file_path.cmp(&b.file_path).then(a.line.cmp(&b.line)));

    let top: Vec<_> = offenders.iter().take(10).collect();
    println!("\nTop offenders:");
    for doc in top {
        println!("  {}:{}: {}", doc.file_path, doc.line, doc.label);
    }
    if offenders.len() > 10 {
        println!("  ... and {} more", offenders.len() - 10);
    }
}

// ---- helpers ----------------------------------------------------------------

fn has_doc(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("doc"))
}

fn line_of(span: proc_macro2::Span) -> usize {
    span.start().line
}

#[allow(clippy::too_many_arguments)]
fn process_item(
    item: &syn::Item,
    file_path: &str,
    check_fields: bool,
    missing: &mut Vec<MissingDoc>,
    total_items: &mut usize,
    total_fields: &mut usize,
    missing_item_count: &mut usize,
    missing_field_count: &mut usize,
) {
    use syn::Item;

    match item {
        Item::Struct(s) => {
            *total_items += 1;
            if !has_doc(&s.attrs) {
                *missing_item_count += 1;
                missing.push(MissingDoc {
                    file_path: file_path.to_string(),
                    line: line_of(s.struct_token.span),
                    label: format!("{} (struct)", s.ident),
                });
            }
            if check_fields
                && let syn::Fields::Named(ref named) = s.fields {
                    for field in &named.named {
                        *total_fields += 1;
                        if !has_doc(&field.attrs) {
                            *missing_field_count += 1;
                            let field_name = field
                                .ident
                                .as_ref()
                                .map(|i| i.to_string())
                                .unwrap_or_default();
                            missing.push(MissingDoc {
                                file_path: file_path.to_string(),
                                line: line_of(field.span()),
                                label: format!("{}::{} (field)", s.ident, field_name),
                            });
                        }
                    }
                }
        }
        Item::Enum(e) => {
            *total_items += 1;
            if !has_doc(&e.attrs) {
                *missing_item_count += 1;
                missing.push(MissingDoc {
                    file_path: file_path.to_string(),
                    line: line_of(e.enum_token.span),
                    label: format!("{} (enum)", e.ident),
                });
            }
            if check_fields {
                for variant in &e.variants {
                    *total_fields += 1;
                    if !has_doc(&variant.attrs) {
                        *missing_field_count += 1;
                        missing.push(MissingDoc {
                            file_path: file_path.to_string(),
                            line: line_of(variant.ident.span()),
                            label: format!("{}::{} (variant)", e.ident, variant.ident),
                        });
                    }
                }
            }
        }
        Item::Fn(f) => {
            *total_items += 1;
            if !has_doc(&f.attrs) {
                *missing_item_count += 1;
                missing.push(MissingDoc {
                    file_path: file_path.to_string(),
                    line: line_of(f.sig.fn_token.span),
                    label: format!("{} (fn)", f.sig.ident),
                });
            }
        }
        Item::Trait(t) => {
            *total_items += 1;
            if !has_doc(&t.attrs) {
                *missing_item_count += 1;
                missing.push(MissingDoc {
                    file_path: file_path.to_string(),
                    line: line_of(t.trait_token.span),
                    label: format!("{} (trait)", t.ident),
                });
            }
            if check_fields {
                for ti in &t.items {
                    if let syn::TraitItem::Fn(tf) = ti {
                        *total_fields += 1;
                        if !has_doc(&tf.attrs) {
                            *missing_field_count += 1;
                            missing.push(MissingDoc {
                                file_path: file_path.to_string(),
                                line: line_of(tf.sig.fn_token.span),
                                label: format!("{}::{} (trait method)", t.ident, tf.sig.ident),
                            });
                        }
                    }
                }
            }
        }
        Item::Mod(m) if m.content.is_some() => {
            *total_items += 1;
            if !has_doc(&m.attrs) {
                *missing_item_count += 1;
                missing.push(MissingDoc {
                    file_path: file_path.to_string(),
                    line: line_of(m.mod_token.span),
                    label: format!("{} (mod)", m.ident),
                });
            }
        }
        Item::Type(t) => {
            *total_items += 1;
            if !has_doc(&t.attrs) {
                *missing_item_count += 1;
                missing.push(MissingDoc {
                    file_path: file_path.to_string(),
                    line: line_of(t.type_token.span),
                    label: format!("{} (type alias)", t.ident),
                });
            }
        }
        Item::Const(c) => {
            *total_items += 1;
            if !has_doc(&c.attrs) {
                *missing_item_count += 1;
                missing.push(MissingDoc {
                    file_path: file_path.to_string(),
                    line: line_of(c.const_token.span),
                    label: format!("{} (const)", c.ident),
                });
            }
        }
        Item::Static(s) => {
            *total_items += 1;
            if !has_doc(&s.attrs) {
                *missing_item_count += 1;
                missing.push(MissingDoc {
                    file_path: file_path.to_string(),
                    line: line_of(s.static_token.span),
                    label: format!("{} (static)", s.ident),
                });
            }
        }
        Item::Impl(impl_block) if check_fields => {
            // Walk impl methods
            for impl_item in &impl_block.items {
                if let syn::ImplItem::Fn(f) = impl_item {
                    *total_fields += 1;
                    if !has_doc(&f.attrs) {
                        *missing_field_count += 1;
                        let type_name = match &*impl_block.self_ty {
                            syn::Type::Path(tp) => tp
                                .path
                                .segments
                                .last()
                                .map(|s| s.ident.to_string())
                                .unwrap_or_default(),
                            _ => String::new(),
                        };
                        missing.push(MissingDoc {
                            file_path: file_path.to_string(),
                            line: line_of(f.sig.fn_token.span),
                            label: format!("{}::{} (impl method)", type_name, f.sig.ident),
                        });
                    }
                }
            }
        }
        // Skip: Use, Impl (without --fields), Macro, ExternCrate, ForeignMod, TraitAlias, Union,
        // Verbatim
        _ => {}
    }
}
