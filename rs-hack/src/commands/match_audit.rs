//! `match-audit` command: detect match expressions that are missing enum variants.

use std::path::PathBuf;

use anyhow::{bail, Result};
use syn::visit::Visit;

use crate::files::collect_rust_files_with_exclusions;

#[derive(Debug)]
pub struct MatchReport {
    pub enum_name: String,
    pub all_variants: Vec<String>,
    pub match_sites: Vec<MatchSite>,
}

#[derive(Debug)]
pub struct MatchSite {
    pub fn_name: String,
    pub file_path: String,
    pub line: usize,
    pub missing_variants: Vec<String>,
    pub has_wildcard: bool,
}

pub fn run(paths: &[PathBuf], enum_name: &str, exclude: &[String]) -> Result<MatchReport> {
    let files = collect_rust_files_with_exclusions(paths, exclude)?;

    // Pass 1: find the enum definition and collect its variants
    let mut all_variants: Vec<String> = Vec::new();

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

        for item in &syntax.items {
            if let syn::Item::Enum(e) = item {
                if e.ident == enum_name {
                    all_variants = e.variants.iter().map(|v| v.ident.to_string()).collect();
                    break;
                }
            }
        }
        if !all_variants.is_empty() {
            break;
        }
    }

    if all_variants.is_empty() {
        bail!(
            "Enum '{}' not found in any of the scanned files. \
             Make sure the paths include the file that defines this enum.",
            enum_name
        );
    }

    // Pass 2: walk match expressions and collect sites
    let mut match_sites: Vec<MatchSite> = Vec::new();

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

        let mut visitor = MatchAuditVisitor {
            enum_name,
            all_variants: &all_variants,
            file_path: file.to_string_lossy().to_string(),
            fn_stack: Vec::new(),
            sites: Vec::new(),
        };
        visitor.visit_file(&syntax);
        match_sites.extend(visitor.sites);
    }

    Ok(MatchReport {
        enum_name: enum_name.to_string(),
        all_variants,
        match_sites,
    })
}

pub fn render(report: &MatchReport) {
    println!("Match audit for enum {}:", report.enum_name);
    println!("  Known variants: {}", report.all_variants.join(", "));
    println!();

    if report.match_sites.is_empty() {
        println!(
            "  No match expressions found for enum {}.",
            report.enum_name
        );
        return;
    }

    println!("Missing variants:");
    let mut any_missing = false;
    for site in &report.match_sites {
        if site.has_wildcard {
            println!(
                "  {} ({}:{}): (wildcard — covers all)",
                site.fn_name, site.file_path, site.line
            );
        } else if site.missing_variants.is_empty() {
            println!("  {}: complete", site.fn_name);
        } else {
            println!(
                "  {} ({}:{}): {}",
                site.fn_name,
                site.file_path,
                site.line,
                site.missing_variants.join(", ")
            );
            any_missing = true;
        }
    }
    if !any_missing {
        println!("  All match expressions are complete.");
    }
}

// ---- visitor ----------------------------------------------------------------

struct MatchAuditVisitor<'a> {
    enum_name: &'a str,
    all_variants: &'a [String],
    file_path: String,
    fn_stack: Vec<String>,
    sites: Vec<MatchSite>,
}

impl<'a> MatchAuditVisitor<'a> {
    fn current_fn(&self) -> String {
        self.fn_stack
            .last()
            .cloned()
            .unwrap_or_else(|| "<top-level>".to_string())
    }
}

impl<'ast, 'a> Visit<'ast> for MatchAuditVisitor<'a> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, node);
        self.fn_stack.pop();
    }

    fn visit_expr_match(&mut self, node: &'ast syn::ExprMatch) {
        // Collect arms that reference our enum
        let mut variants_seen: Vec<String> = Vec::new();
        let mut has_wildcard = false;

        for arm in &node.arms {
            if arm_has_wildcard(&arm.pat) {
                has_wildcard = true;
            }
            collect_enum_variants_from_pat(&arm.pat, self.enum_name, &mut variants_seen);
        }

        // Only report if at least one arm used our enum (wildcard alone is not enough —
        // every `match _ => ...` would otherwise show up under every audit).
        if !variants_seen.is_empty() {
            let missing: Vec<String> = if has_wildcard {
                vec![]
            } else {
                self.all_variants
                    .iter()
                    .filter(|v| !variants_seen.contains(v))
                    .cloned()
                    .collect()
            };

            // Approximate line number via proc_macro2 span
            let line = node.match_token.span.start().line;

            self.sites.push(MatchSite {
                fn_name: self.current_fn(),
                file_path: self.file_path.clone(),
                line,
                missing_variants: missing,
                has_wildcard,
            });
        }

        syn::visit::visit_expr_match(self, node);
    }
}

/// Return true if this pattern is a wildcard (`_`) or an ident that acts as a catch-all.
fn arm_has_wildcard(pat: &syn::Pat) -> bool {
    match pat {
        syn::Pat::Wild(_) => true,
        syn::Pat::Ident(pi) if pi.ident == "_" => true,
        syn::Pat::Or(po) => po.cases.iter().any(arm_has_wildcard),
        _ => false,
    }
}

/// Walk a pattern and push enum variant names into `out` when the second-to-last
/// path segment equals `enum_name`.
fn collect_enum_variants_from_pat(pat: &syn::Pat, enum_name: &str, out: &mut Vec<String>) {
    match pat {
        syn::Pat::Path(pp) => {
            check_path(&pp.path, enum_name, out);
        }
        syn::Pat::TupleStruct(pts) => {
            check_path(&pts.path, enum_name, out);
        }
        syn::Pat::Struct(ps) => {
            check_path(&ps.path, enum_name, out);
        }
        syn::Pat::Or(po) => {
            for case in &po.cases {
                collect_enum_variants_from_pat(case, enum_name, out);
            }
        }
        syn::Pat::Tuple(pt) => {
            for elem in &pt.elems {
                collect_enum_variants_from_pat(elem, enum_name, out);
            }
        }
        syn::Pat::Reference(pr) => collect_enum_variants_from_pat(&pr.pat, enum_name, out),
        syn::Pat::Paren(pp) => collect_enum_variants_from_pat(&pp.pat, enum_name, out),
        _ => {}
    }
}

fn check_path(path: &syn::Path, enum_name: &str, out: &mut Vec<String>) {
    let segs: Vec<&syn::PathSegment> = path.segments.iter().collect();
    if segs.len() >= 2 {
        let second_to_last = segs[segs.len() - 2].ident.to_string();
        if second_to_last == enum_name {
            let variant = segs.last().unwrap().ident.to_string();
            if !out.contains(&variant) {
                out.push(variant);
            }
        }
    }
}
