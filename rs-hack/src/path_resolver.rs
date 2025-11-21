/// Path resolution module for safely matching qualified paths in Rust code.
///
/// This module helps determine whether a path in code refers to a specific target
/// by tracking use statements and validating path qualifications.
///
/// Example: When looking for `crate::compiler::types::IRValue::Variant`, this resolver
/// will match:
/// - `IRValue::Variant` (if `use crate::compiler::types::IRValue;` exists)
/// - `types::IRValue::Variant` (if `use crate::compiler;` exists)
/// - `crate::compiler::types::IRValue::Variant` (fully qualified)
///
/// But will NOT match:
/// - `OtherEnum::Variant` (different enum entirely)
/// - `IRValue::Variant` (if no appropriate use statement exists)

use std::collections::HashMap;
use syn::{visit::Visit, File, ItemUse, Path, UseTree};

/// Tracks use statements and validates whether paths refer to a specific target.
///
/// This is generic enough to work for enums, structs, functions, traits, etc.
#[derive(Debug, Clone)]
pub struct PathResolver {
    /// The canonical fully-qualified path we're looking for
    /// e.g., ["crate", "compiler", "types", "IRValue"]
    target_canonical_segments: Vec<String>,

    /// The simple name of the target (last segment of canonical path)
    /// e.g., "IRValue"
    target_simple_name: String,

    /// Maps local names/aliases to their canonical path segments
    /// e.g., "IRValue" -> ["crate", "compiler", "types", "IRValue"]
    /// e.g., "types" -> ["crate", "compiler", "types"]
    /// e.g., "IV" -> ["crate", "compiler", "types", "IRValue"] (aliased)
    local_aliases: HashMap<String, Vec<String>>,

    /// Tracks if we found a glob import that might include our target
    /// e.g., `use crate::compiler::types::*;`
    has_potential_glob_import: bool,
}

impl PathResolver {
    /// Create a new path resolver for a specific canonical path.
    ///
    /// # Arguments
    /// * `canonical_path` - The fully qualified path (e.g., "crate::compiler::types::IRValue")
    ///
    /// # Returns
    /// A new PathResolver, or None if the path is invalid
    ///
    /// # Example
    /// ```
    /// use rs_hack::path_resolver::PathResolver;
    /// let resolver = PathResolver::new("crate::compiler::types::IRValue");
    /// ```
    pub fn new(canonical_path: &str) -> Option<Self> {
        if canonical_path.is_empty() {
            return None;
        }

        let segments: Vec<String> = canonical_path
            .split("::")
            .map(String::from)
            .collect();

        if segments.is_empty() {
            return None;
        }

        let simple_name = segments.last().unwrap().clone();

        Some(Self {
            target_canonical_segments: segments,
            target_simple_name: simple_name,
            local_aliases: HashMap::new(),
            has_potential_glob_import: false,
        })
    }

    /// Create a resolver that only matches exact simple paths (backward compatible mode).
    ///
    /// This matches the old behavior where only `EnumName::Variant` is matched,
    /// without any use statement tracking.
    pub fn simple(name: &str) -> Self {
        Self {
            target_canonical_segments: vec![name.to_string()],
            target_simple_name: name.to_string(),
            local_aliases: HashMap::new(),
            has_potential_glob_import: false,
        }
    }

    /// Scan a file to build the local alias map from use statements.
    ///
    /// This should be called once per file before using `matches_target()`.
    pub fn scan_file(&mut self, file: &File) {
        let mut scanner = UseStatementScanner {
            target_canonical_segments: &self.target_canonical_segments,
            local_aliases: &mut self.local_aliases,
            has_potential_glob_import: &mut self.has_potential_glob_import,
        };
        scanner.visit_file(file);
    }

    /// Check if a path definitely refers to our target.
    ///
    /// This uses conservative matching - only returns true if we're certain
    /// the path refers to our target based on:
    /// 1. Exact canonical path match
    /// 2. Import alias resolution
    /// 3. Module path resolution
    ///
    /// # Arguments
    /// * `path` - The syn::Path to check
    ///
    /// # Returns
    /// true if the path definitely refers to our target
    pub fn matches_target(&self, path: &Path) -> bool {
        if path.segments.is_empty() {
            return false;
        }

        let path_segments: Vec<String> = path
            .segments
            .iter()
            .map(|seg| seg.ident.to_string())
            .collect();

        // Case 1: Exact canonical path match
        // e.g., `crate::compiler::types::IRValue` matches exactly
        if path_segments == self.target_canonical_segments {
            return true;
        }

        // Case 2: Check if any prefix is an alias we know about
        // e.g., if `use crate::compiler::types;` exists,
        // then `types::IRValue` should match
        for i in 1..=path_segments.len() {
            let prefix = &path_segments[0..i];
            let prefix_str = prefix.join("::");

            if let Some(canonical_prefix) = self.local_aliases.get(&prefix_str) {
                // Rebuild the full path using the canonical prefix
                let mut full_path = canonical_prefix.clone();
                full_path.extend_from_slice(&path_segments[i..]);

                if full_path == self.target_canonical_segments {
                    return true;
                }
            }
        }

        // Case 3: Simple import case
        // e.g., if `use crate::compiler::types::IRValue;` exists,
        // then just `IRValue` should match
        if path_segments.len() == 1 {
            if let Some(canonical) = self.local_aliases.get(&path_segments[0]) {
                return canonical == &self.target_canonical_segments;
            }
        }

        false
    }

    /// Check if a path ends with the preceding segment (e.g., enum name).
    ///
    /// This is useful for matching patterns like `EnumName::VariantName`
    /// regardless of what the variant name is, when combined with path validation.
    ///
    /// # Arguments
    /// * `path` - The path to check
    /// * `preceding_segment` - The segment before the variant (e.g., "IRValue" for enum variants)
    ///
    /// # Returns
    /// true if the path has at least 2 segments and the second-to-last matches preceding_segment
    pub fn path_ends_with(&self, path: &Path, preceding_segment: &str) -> bool {
        let segments: Vec<_> = path.segments.iter().collect();
        let len = segments.len();

        if len >= 2 {
            segments[len - 2].ident == preceding_segment
        } else {
            false
        }
    }

    /// Get the simple name of the target.
    pub fn target_name(&self) -> &str {
        &self.target_simple_name
    }

    /// Check if a path could potentially match via glob import.
    ///
    /// Returns true if:
    /// - We found a glob import that could include our target
    /// - The path's simple name matches our target
    pub fn might_match_via_glob(&self, path: &Path) -> bool {
        if !self.has_potential_glob_import {
            return false;
        }

        // Check if the last segment matches our target name
        path.segments
            .last()
            .map(|seg| seg.ident == self.target_simple_name)
            .unwrap_or(false)
    }
}

/// Visitor that scans use statements to build the alias map.
struct UseStatementScanner<'a> {
    target_canonical_segments: &'a [String],
    local_aliases: &'a mut HashMap<String, Vec<String>>,
    has_potential_glob_import: &'a mut bool,
}

impl<'a> UseStatementScanner<'a> {
    /// Process a use tree and extract aliases.
    fn process_use_tree(&mut self, tree: &UseTree, prefix: Vec<String>) {
        match tree {
            UseTree::Path(path) => {
                let mut new_prefix = prefix.clone();
                new_prefix.push(path.ident.to_string());
                self.process_use_tree(&path.tree, new_prefix);
            }
            UseTree::Name(name) => {
                // Simple import: `use crate::foo::Bar;`
                let mut full_path = prefix.clone();
                full_path.push(name.ident.to_string());

                // Map the simple name to the full path
                let local_name = name.ident.to_string();
                self.local_aliases.insert(local_name, full_path.clone());

                // Also map intermediate paths
                // e.g., `use crate::compiler::types;` maps "types" to ["crate", "compiler", "types"]
                if !prefix.is_empty() {
                    let prefix_str = prefix.join("::");
                    self.local_aliases.insert(prefix_str, prefix);
                }
            }
            UseTree::Rename(rename) => {
                // Aliased import: `use crate::foo::Bar as Baz;`
                let mut full_path = prefix.clone();
                full_path.push(rename.ident.to_string());

                let local_name = rename.rename.to_string();
                self.local_aliases.insert(local_name, full_path);
            }
            UseTree::Glob(_glob) => {
                // Glob import: `use crate::foo::*;`
                // Check if this glob could import our target
                if self.is_potential_glob_for_target(&prefix) {
                    *self.has_potential_glob_import = true;
                }
            }
            UseTree::Group(group) => {
                // Grouped imports: `use crate::foo::{Bar, Baz};`
                for tree in &group.items {
                    self.process_use_tree(tree, prefix.clone());
                }
            }
        }
    }

    /// Check if a glob import could potentially import our target.
    fn is_potential_glob_for_target(&self, glob_prefix: &[String]) -> bool {
        // Check if our target starts with this prefix
        if self.target_canonical_segments.len() <= glob_prefix.len() {
            return false;
        }

        // Check if the glob prefix matches the start of our target
        for (i, segment) in glob_prefix.iter().enumerate() {
            if i >= self.target_canonical_segments.len() {
                return false;
            }
            if segment != &self.target_canonical_segments[i] {
                return false;
            }
        }

        // The glob is one level above our target
        self.target_canonical_segments.len() == glob_prefix.len() + 1
    }
}

impl<'ast, 'a> Visit<'ast> for UseStatementScanner<'a> {
    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        self.process_use_tree(&node.tree, Vec::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn test_exact_canonical_path_match() {
        let resolver = PathResolver::new("crate::compiler::types::IRValue").unwrap();
        let path: Path = parse_quote!(crate::compiler::types::IRValue);
        assert!(resolver.matches_target(&path));
    }

    #[test]
    fn test_simple_import() {
        let mut resolver = PathResolver::new("crate::compiler::types::IRValue").unwrap();
        let file: File = parse_quote! {
            use crate::compiler::types::IRValue;

            fn foo() {}
        };
        resolver.scan_file(&file);

        let path: Path = parse_quote!(IRValue);
        assert!(resolver.matches_target(&path));
    }

    #[test]
    fn test_module_import() {
        let mut resolver = PathResolver::new("crate::compiler::types::IRValue").unwrap();
        let file: File = parse_quote! {
            use crate::compiler::types;

            fn foo() {}
        };
        resolver.scan_file(&file);

        let path: Path = parse_quote!(types::IRValue);
        assert!(resolver.matches_target(&path));
    }

    #[test]
    fn test_aliased_import() {
        let mut resolver = PathResolver::new("crate::compiler::types::IRValue").unwrap();
        let file: File = parse_quote! {
            use crate::compiler::types::IRValue as IV;

            fn foo() {}
        };
        resolver.scan_file(&file);

        let path: Path = parse_quote!(IV);
        assert!(resolver.matches_target(&path));
    }

    #[test]
    fn test_does_not_match_different_path() {
        let resolver = PathResolver::new("crate::compiler::types::IRValue").unwrap();
        let path: Path = parse_quote!(crate::other::types::IRValue);
        assert!(!resolver.matches_target(&path));
    }

    #[test]
    fn test_does_not_match_without_import() {
        let mut resolver = PathResolver::new("crate::compiler::types::IRValue").unwrap();
        let file: File = parse_quote! {
            // No imports
            fn foo() {}
        };
        resolver.scan_file(&file);

        let path: Path = parse_quote!(IRValue);
        assert!(!resolver.matches_target(&path));
    }

    #[test]
    fn test_glob_import_detection() {
        let mut resolver = PathResolver::new("crate::compiler::types::IRValue").unwrap();
        let file: File = parse_quote! {
            use crate::compiler::types::*;

            fn foo() {}
        };
        resolver.scan_file(&file);

        let path: Path = parse_quote!(IRValue);
        assert!(resolver.might_match_via_glob(&path));
    }

    #[test]
    fn test_path_ends_with() {
        let resolver = PathResolver::new("crate::compiler::types::IRValue").unwrap();

        let path1: Path = parse_quote!(IRValue::HashMap);
        assert!(resolver.path_ends_with(&path1, "IRValue"));

        let path2: Path = parse_quote!(crate::compiler::types::IRValue::HashMap);
        assert!(resolver.path_ends_with(&path2, "IRValue"));

        let path3: Path = parse_quote!(OtherEnum::HashMap);
        assert!(!resolver.path_ends_with(&path3, "IRValue"));
    }

    #[test]
    fn test_grouped_imports() {
        let mut resolver = PathResolver::new("crate::compiler::types::IRValue").unwrap();
        let file: File = parse_quote! {
            use crate::compiler::types::{IRValue, Frame};

            fn foo() {}
        };
        resolver.scan_file(&file);

        let path: Path = parse_quote!(IRValue);
        assert!(resolver.matches_target(&path));
    }
}
