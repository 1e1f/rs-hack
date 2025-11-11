use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use walkdir::WalkDir;
use glob::glob;

mod operations;
mod visitor;
mod editor;
mod state;
mod diff;
mod path_resolver;
mod surgical;

#[cfg(test)]
mod tests;

use operations::*;
use editor::RustEditor;
use state::{*, save_backup_nodes};
use diff::{print_diff, print_summary_diff, DiffStats};

#[derive(Parser)]
#[command(name = "rs-hack")]
#[command(about = "AST-aware Rust code editing tool for AI agents", long_about = None)]
#[command(version)]
struct Cli {
    /// Use project-local state directory (.rs-hack) instead of ~/.rs-hack
    #[arg(long, global = true)]
    local_state: bool,

    /// Output format: "default", "diff", or "summary"
    #[arg(long, default_value = "default", global = true)]
    format: String,

    /// Show summary statistics after diff output
    #[arg(long, global = true)]
    summary: bool,

    /// Filter targets based on traits or attributes (e.g., "derives_trait:Clone", "derives_trait:Serialize,Debug")
    #[arg(long, global = true)]
    r#where: Option<String>,

    /// Exclude paths matching these patterns (can be used multiple times)
    #[arg(long, global = true, num_args = 0..)]
    exclude: Vec<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a field to a struct (idempotent - skips if field already exists)
    AddStructField {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the struct to modify
        #[arg(short, long)]
        struct_name: String,

        /// Field to add (e.g., "field_name: Type" or just "field_name" if using --literal-default)
        #[arg(short, long)]
        field: String,

        /// Where to insert: "first", "last", or "after:field_name"
        #[arg(short = 'P', long, default_value = "last")]
        position: String,

        /// Default value for struct literals (e.g., "None", "vec![]", "0")
        /// - If provided: tries to add to definition (idempotent), always adds to literals
        /// - If omitted: only adds to definition
        /// Common case: field already exists in struct, you just want to add it to all literals
        #[arg(long)]
        literal_default: Option<String>,

        /// Output path (if specified, writes to new file instead of modifying in place)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Update an existing struct field (changes type/visibility)
    UpdateStructField {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the struct to modify
        #[arg(short, long)]
        struct_name: String,

        /// Field definition (e.g., "field_name: NewType" or "pub field_name: Type")
        #[arg(short, long)]
        field: String,

        /// Output path (if specified, writes to new file instead of modifying in place)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Remove a field from a struct
    RemoveStructField {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the struct to modify
        #[arg(short, long)]
        struct_name: String,

        /// Name of the field to remove
        #[arg(short = 'n', long)]
        field_name: String,

        /// Output path (if specified, writes to new file instead of modifying in place)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// DEPRECATED: Use add-struct-field --literal-only instead
    /// Add a field to struct literal expressions (initialization)
    #[deprecated]
    AddStructLiteralField {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the struct to target
        #[arg(short, long)]
        struct_name: String,

        /// Field with value to add (e.g., "return_type: None")
        #[arg(short, long)]
        field: String,

        /// Where to insert: "first", "last", "after:field_name", or "before:field_name"
        #[arg(short = 'P', long, default_value = "last")]
        position: String,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Add a variant to an enum (skips if variant already exists)
    AddEnumVariant {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the enum to modify
        #[arg(short, long)]
        enum_name: String,

        /// Variant to add (e.g., "NewVariant" or "NewVariant { field: Type }")
        #[arg(short, long)]
        variant: String,

        /// Where to insert: "first", "last", or "after:VariantName"
        #[arg(short = 'P', long, default_value = "last")]
        position: String,

        /// Output path (if specified, writes to new file instead of modifying in place)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Update an existing enum variant
    UpdateEnumVariant {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the enum to modify
        #[arg(short, long)]
        enum_name: String,

        /// Variant definition (e.g., "VariantName { new_field: Type }")
        #[arg(short, long)]
        variant: String,

        /// Output path (if specified, writes to new file instead of modifying in place)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Remove a variant from an enum
    RemoveEnumVariant {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the enum to modify
        #[arg(short, long)]
        enum_name: String,

        /// Name of the variant to remove
        #[arg(short = 'n', long)]
        variant_name: String,

        /// Output path (if specified, writes to new file instead of modifying in place)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Rename an enum variant across the codebase
    RenameEnumVariant {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the enum containing the variant
        #[arg(short, long)]
        enum_name: String,

        /// Current name of the variant
        #[arg(short = 'o', long)]
        old_variant: String,

        /// New name for the variant
        #[arg(short = 'n', long)]
        new_variant: String,

        /// Optional fully-qualified path to the enum (e.g., "crate::compiler::types::IRValue")
        /// When provided, enables safe matching of fully qualified paths by tracking use statements
        #[arg(long)]
        enum_path: Option<String>,

        /// Edit mode: 'surgical' (default, preserves formatting) or 'reformat' (uses prettyplease)
        #[arg(long, default_value = "surgical")]
        edit_mode: String,

        /// Validate mode: check for remaining references without making changes
        #[arg(long)]
        validate: bool,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Rename a function across the codebase
    RenameFunction {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Current name of the function
        #[arg(short = 'o', long)]
        old_name: String,

        /// New name for the function
        #[arg(short = 'n', long)]
        new_name: String,

        /// Optional fully-qualified path to the function (e.g., "crate::utils::process_v2")
        /// When provided, enables safe matching of fully qualified paths by tracking use statements
        #[arg(long)]
        function_path: Option<String>,

        /// Edit mode: 'surgical' (default, preserves formatting) or 'reformat' (uses prettyplease)
        #[arg(long, default_value = "surgical")]
        edit_mode: String,

        /// Validate mode: check for remaining references without making changes
        #[arg(long)]
        validate: bool,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Add a match arm for a specific pattern
    AddMatchArm {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Pattern to match (e.g., "MyEnum::NewVariant"). Not required with --auto-detect
        #[arg(short = 'P', long)]
        pattern: Option<String>,

        /// Body of the match arm (e.g., "todo!()" or "println!(\"handled\")")
        #[arg(short, long)]
        body: String,

        /// Optional: function name containing the match
        #[arg(short, long)]
        function: Option<String>,

        /// Auto-detect and add all missing enum variants
        #[arg(long)]
        auto_detect: bool,

        /// Enum name for auto-detection (required if --auto-detect is used)
        #[arg(short, long)]
        enum_name: Option<String>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Update an existing match arm
    UpdateMatchArm {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Pattern to match (e.g., "MyEnum::Variant")
        #[arg(short = 'P', long)]
        pattern: String,

        /// New body for the match arm
        #[arg(short, long)]
        body: String,

        /// Optional: function name containing the match
        #[arg(short, long)]
        function: Option<String>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Remove a match arm
    RemoveMatchArm {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Pattern to remove (e.g., "MyEnum::Variant")
        #[arg(short = 'P', long)]
        pattern: String,

        /// Optional: function name containing the match
        #[arg(short, long)]
        function: Option<String>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Batch operation from JSON or YAML specification
    Batch {
        /// Path to JSON or YAML file with batch operations
        #[arg(short, long)]
        spec: PathBuf,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },
    
    /// Find locations of AST nodes (for debugging/inspection)
    Find {
        /// Path to the Rust file (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Type of node: "struct", "enum", "fn", "impl"
        #[arg(short = 't', long)]
        node_type: String,

        /// Name of the node
        #[arg(short, long)]
        name: String,
    },

    /// Inspect and list AST nodes with full content (supports glob patterns)
    Inspect {
        /// Path to Rust file(s) - supports multiple paths and glob patterns (e.g., "tests/*.rs")
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Type of node: Expression-level: "struct-literal", "match-arm", "enum-usage", "function-call", "method-call", "macro-call", "identifier", "type-ref". Definition-level: "struct", "enum", "function", "impl-method", "trait", "const", "static", "type-alias", "mod"
        #[arg(short = 't', long)]
        node_type: String,

        /// Filter by name (e.g., "Shadow", "Operator::Error", "unwrap", "eprintln", "Vec")
        #[arg(short, long)]
        name: Option<String>,

        /// Filter by content - only show nodes whose source contains this string (e.g., "[SHADOW RENDER]")
        #[arg(short = 'c', long)]
        content_filter: Option<String>,

        /// Include preceding comments (doc and regular) in output
        #[arg(long, default_value = "true", action = clap::ArgAction::Set)]
        include_comments: bool,

        /// Output format: "json", "locations", "snippets"
        #[arg(short = 'f', long, default_value = "snippets")]
        format: String,
    },

    /// Add derive macros to a struct or enum
    AddDerive {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Type of target: "struct" or "enum"
        #[arg(short = 't', long)]
        target_type: String,

        /// Name of the struct or enum
        #[arg(short, long)]
        name: String,

        /// Derives to add (comma-separated, e.g., "Clone,Debug,Serialize")
        #[arg(short, long)]
        derives: String,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Add a method to an impl block
    AddImplMethod {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Name of the type (struct/enum) that the impl is for
        #[arg(short = 't', long)]
        target: String,

        /// Method definition (e.g., "pub fn get_id(&self) -> u64 { self.id }")
        #[arg(short, long)]
        method: String,

        /// Where to insert: "first", "last", "after:method_name", or "before:method_name"
        #[arg(short = 'P', long, default_value = "last")]
        position: String,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Add a use statement
    AddUse {
        /// Path to the Rust file or directory (supports multiple paths and glob patterns)
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Use path (e.g., "std::collections::HashMap" or "serde::Serialize")
        #[arg(short = 'u', long)]
        use_path: String,

        /// Where to insert: "first", "last", or "after:module"
        #[arg(short = 'P', long, default_value = "last")]
        position: String,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Show history of rs-hack runs
    History {
        /// Number of recent runs to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// Revert a specific run
    Revert {
        /// Run ID to revert (from history)
        run_id: String,

        /// Force revert even if files changed since
        #[arg(long)]
        force: bool,
    },

    /// Clean old state data
    Clean {
        /// Keep runs from last N days
        #[arg(long, default_value = "30")]
        keep_days: u32,
    },

    /// Transform AST nodes (generic find-and-transform operation)
    Transform {
        /// Path to Rust file(s) - supports multiple paths and glob patterns (e.g., "src/**/*.rs")
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Type of node: "macro-call", "method-call", "function-call", etc.
        #[arg(short = 't', long)]
        node_type: String,

        /// Filter by name (e.g., "eprintln", "unwrap")
        #[arg(short, long)]
        name: Option<String>,

        /// Filter by content - only transform nodes containing this string
        #[arg(short = 'c', long)]
        content_filter: Option<String>,

        /// Action to perform: "comment", "remove", or "replace"
        #[arg(short, long)]
        action: String,

        /// Replacement code (required if action is "replace")
        #[arg(short = 'w', long)]
        with: Option<String>,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Add documentation comment to an item
    AddDocComment {
        /// Path to Rust file(s) - supports multiple paths and glob patterns
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Type of target: "struct", "enum", "function", "field", "variant"
        #[arg(short = 't', long)]
        target_type: String,

        /// Name of the target (e.g., "User", "Status::Draft", "User::id")
        #[arg(short, long)]
        name: String,

        /// Documentation comment text (without /// prefix)
        #[arg(short = 'd', long)]
        doc_comment: String,

        /// Comment style: "line" (///) or "block" (/** */)
        #[arg(long, default_value = "line")]
        style: String,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Update existing documentation comment
    UpdateDocComment {
        /// Path to Rust file(s) - supports multiple paths and glob patterns
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Type of target: "struct", "enum", "function", "field", "variant"
        #[arg(short = 't', long)]
        target_type: String,

        /// Name of the target
        #[arg(short, long)]
        name: String,

        /// New documentation comment text
        #[arg(short = 'd', long)]
        doc_comment: String,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },

    /// Remove documentation comment from an item
    RemoveDocComment {
        /// Path to Rust file(s) - supports multiple paths and glob patterns
        #[arg(short, long, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// Type of target: "struct", "enum", "function", "field", "variant"
        #[arg(short = 't', long)]
        target_type: String,

        /// Name of the target
        #[arg(short, long)]
        name: String,

        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },
}

/// Validate that an enum variant rename would catch all references
fn validate_enum_variant_rename(
    files: &[PathBuf],
    enum_name: &str,
    old_variant: &str,
    enum_path: Option<&str>,
) -> Result<()> {
    use syn::{visit::Visit, File};

    struct VariantFinder<'a> {
        enum_name: &'a str,
        variant_name: &'a str,
        enum_path: Option<&'a str>,
        references: Vec<(String, usize, usize, String)>, // (file, line, col, code)
    }

    impl<'a, 'ast> Visit<'ast> for VariantFinder<'a> {
        fn visit_path(&mut self, path: &'ast syn::Path) {
            let path_str = quote::quote!(#path).to_string();

            // Check if this path contains our variant
            // Handle cases like:
            // - EnumName::VariantName
            // - super::EnumName::VariantName
            // - crate::module::EnumName::VariantName
            if path_str.contains(self.variant_name) {
                let segments: Vec<_> = path.segments.iter().collect();
                let len = segments.len();

                if len >= 2 {
                    let enum_seg = &segments[len - 2];
                    let variant_seg = &segments[len - 1];

                    if enum_seg.ident == self.enum_name && variant_seg.ident == self.variant_name {
                        // Found a match - we'll record it in visit_file
                        syn::visit::visit_path(self, path);
                        return;
                    }
                } else if len == 1 && segments[0].ident == self.variant_name {
                    // Might be an imported variant
                    syn::visit::visit_path(self, path);
                    return;
                }
            }

            syn::visit::visit_path(self, path);
        }
    }

    let mut finder = VariantFinder {
        enum_name,
        variant_name: old_variant,
        enum_path,
        references: Vec::new(),
    };

    for file_path in files {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read {}", file_path.display()))?;

        let syntax_tree: File = syn::parse_str(&content)
            .with_context(|| format!("Failed to parse {}", file_path.display()))?;

        // Search for simple text matches (this catches things AST might miss)
        for (line_num, line) in content.lines().enumerate() {
            if line.contains(old_variant) {
                // Check if it's part of our enum variant pattern
                if line.contains(&format!("{}::{}", enum_name, old_variant)) ||
                   line.contains(&format!("::{}", old_variant)) {
                    finder.references.push((
                        file_path.display().to_string(),
                        line_num + 1,
                        0,
                        line.trim().to_string(),
                    ));
                }
            }
        }
    }

    if finder.references.is_empty() {
        println!("✓ No references to '{}::{}' found.", enum_name, old_variant);
        println!("  All occurrences have been renamed or there were none to begin with.");
    } else {
        println!("❌ Found {} remaining references to '{}::{}':",
                 finder.references.len(), enum_name, old_variant);
        println!();

        for (file, line, _col, code) in &finder.references {
            println!("  - {}:{}", file, line);
            println!("    {}", code);
        }

        println!();
        println!("💡 Suggestions:");
        if enum_path.is_none() {
            println!("  - Try using --enum-path to enable better matching of fully qualified paths");
        }
        println!("  - Run without --validate to rename these references");
        println!("  - Check if these are false positives (comments, strings, etc.)");
    }

    Ok(())
}

/// Validate that a function rename would catch all references
fn validate_function_rename(
    files: &[PathBuf],
    old_name: &str,
    function_path: Option<&str>,
) -> Result<()> {
    let mut references = Vec::new();

    for file_path in files {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read {}", file_path.display()))?;

        // Search for simple text matches
        for (line_num, line) in content.lines().enumerate() {
            if line.contains(old_name) {
                // Check if it looks like a function reference
                // This is a simple heuristic - matches function calls, definitions, etc.
                let patterns = [
                    format!("fn {}(", old_name),
                    format!("fn {}<", old_name),
                    format!("{}(", old_name),
                    format!("{}::", old_name),
                    format!("::{}", old_name),
                ];

                if patterns.iter().any(|p| line.contains(p)) {
                    references.push((
                        file_path.display().to_string(),
                        line_num + 1,
                        line.trim().to_string(),
                    ));
                }
            }
        }
    }

    if references.is_empty() {
        println!("✓ No references to '{}' found.", old_name);
        println!("  All occurrences have been renamed or there were none to begin with.");
    } else {
        println!("❌ Found {} remaining references to '{}':", references.len(), old_name);
        println!();

        for (file, line, code) in &references {
            println!("  - {}:{}", file, line);
            println!("    {}", code);
        }

        println!();
        println!("💡 Suggestions:");
        if function_path.is_none() {
            println!("  - Try using --function-path to enable better matching of fully qualified paths");
        }
        println!("  - Run without --validate to rename these references");
        println!("  - Check if these are false positives (comments, strings, etc.)");
    }

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::AddStructField { paths, struct_name, field, position, literal_default, output, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::AddStructField(AddStructFieldOp {
                struct_name: struct_name.clone(),
                field_def: field.clone(),
                position: parse_position(&position)?,
                literal_default: literal_default.clone(),
                where_filter: cli.r#where.clone(),
            });

            execute_operation_with_state(&files, &op, apply, output.as_ref(), &cli.local_state, &cli.format, cli.summary)?;
        }

        Commands::UpdateStructField { paths, struct_name, field, output, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::UpdateStructField(UpdateStructFieldOp {
                struct_name: struct_name.clone(),
                field_def: field.clone(),
                where_filter: cli.r#where.clone(),
            });

            execute_operation_with_state(&files, &op, apply, output.as_ref(), &cli.local_state, &cli.format, cli.summary)?;
        }

        Commands::RemoveStructField { paths, struct_name, field_name, output, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::RemoveStructField(RemoveStructFieldOp {
                struct_name: struct_name.clone(),
                field_name: field_name.clone(),
                where_filter: cli.r#where.clone(),
            });

            execute_operation_with_state(&files, &op, apply, output.as_ref(), &cli.local_state, &cli.format, cli.summary)?;
        }

        Commands::AddStructLiteralField { paths, struct_name, field, position, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::AddStructLiteralField(AddStructLiteralFieldOp {
                struct_name: struct_name.clone(),
                field_def: field.clone(),
                position: parse_position(&position)?,
                struct_path: None,  // Deprecated command doesn't support path resolution
            });

            execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary)?;
        }

        Commands::AddEnumVariant { paths, enum_name, variant, position, output, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::AddEnumVariant(AddEnumVariantOp {
                enum_name: enum_name.clone(),
                variant_def: variant.clone(),
                position: parse_position(&position)?,
                where_filter: cli.r#where.clone(),
            });

            execute_operation_with_state(&files, &op, apply, output.as_ref(), &cli.local_state, &cli.format, cli.summary)?;
        }

        Commands::UpdateEnumVariant { paths, enum_name, variant, output, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::UpdateEnumVariant(UpdateEnumVariantOp {
                enum_name: enum_name.clone(),
                variant_def: variant.clone(),
                where_filter: cli.r#where.clone(),
            });

            execute_operation_with_state(&files, &op, apply, output.as_ref(), &cli.local_state, &cli.format, cli.summary)?;
        }

        Commands::RemoveEnumVariant { paths, enum_name, variant_name, output, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::RemoveEnumVariant(RemoveEnumVariantOp {
                enum_name: enum_name.clone(),
                variant_name: variant_name.clone(),
                where_filter: cli.r#where.clone(),
            });

            execute_operation_with_state(&files, &op, apply, output.as_ref(), &cli.local_state, &cli.format, cli.summary)?;
        }

        Commands::RenameEnumVariant { paths, enum_name, old_variant, new_variant, enum_path, edit_mode, validate, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            // If validate mode, run validation instead of rename
            if validate {
                validate_enum_variant_rename(&files, &enum_name, &old_variant, enum_path.as_deref())?;
            } else {
                // Parse edit mode
                let edit_mode = edit_mode.parse::<EditMode>()
                    .map_err(|e| anyhow::anyhow!("{}", e))?;

                let op = Operation::RenameEnumVariant(RenameEnumVariantOp {
                    enum_name: enum_name.clone(),
                    old_variant: old_variant.clone(),
                    new_variant: new_variant.clone(),
                    enum_path: enum_path.clone(),
                    edit_mode,
                });

                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary)?;
            }
        }

        Commands::RenameFunction { paths, old_name, new_name, function_path, edit_mode, validate, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            // If validate mode, run validation instead of rename
            if validate {
                validate_function_rename(&files, &old_name, function_path.as_deref())?;
            } else {
                // Parse edit mode
                let edit_mode = edit_mode.parse::<EditMode>()
                    .map_err(|e| anyhow::anyhow!("{}", e))?;

                let op = Operation::RenameFunction(RenameFunctionOp {
                    old_name: old_name.clone(),
                    new_name: new_name.clone(),
                    function_path: function_path.clone(),
                    edit_mode,
                });

                execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary)?;
            }
        }

        Commands::AddMatchArm { paths, pattern, body, function, auto_detect, enum_name, apply } => {
            // Validate auto_detect requires enum_name
            if auto_detect && enum_name.is_none() {
                anyhow::bail!("--enum-name is required when using --auto-detect");
            }

            // Validate pattern is provided when not using auto_detect
            if !auto_detect && pattern.is_none() {
                anyhow::bail!("--pattern is required when not using --auto-detect");
            }

            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::AddMatchArm(AddMatchArmOp {
                pattern: pattern.unwrap_or_default(),
                body: body.clone(),
                function_name: function,
                auto_detect,
                enum_name,
            });

            execute_operation(&files, &op, apply, None, &cli.format, cli.summary)?;
        }

        Commands::UpdateMatchArm { paths, pattern, body, function, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::UpdateMatchArm(UpdateMatchArmOp {
                pattern: pattern.clone(),
                new_body: body.clone(),
                function_name: function,
            });

            execute_operation(&files, &op, apply, None, &cli.format, cli.summary)?;
        }

        Commands::RemoveMatchArm { paths, pattern, function, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::RemoveMatchArm(RemoveMatchArmOp {
                pattern: pattern.clone(),
                function_name: function,
            });

            execute_operation(&files, &op, apply, None, &cli.format, cli.summary)?;
        }

        Commands::Batch { spec, apply } => {
            let content = std::fs::read_to_string(&spec)
                .context("Failed to read batch spec file")?;

            // Auto-detect format based on file extension
            let batch: BatchSpec = if spec.extension().and_then(|s| s.to_str()) == Some("yaml")
                || spec.extension().and_then(|s| s.to_str()) == Some("yml") {
                serde_yaml::from_str(&content)
                    .context("Failed to parse batch spec YAML")?
            } else {
                // Try JSON first, fall back to YAML if JSON fails
                serde_json::from_str(&content)
                    .or_else(|_| serde_yaml::from_str(&content))
                    .context("Failed to parse batch spec (tried both JSON and YAML)")?
            };

            execute_batch(&batch, apply, &cli.exclude)?;
        }
        
        Commands::Find { paths, node_type, name } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            for file in files {
                let content = std::fs::read_to_string(&file)
                    .context(format!("Failed to read file: {:?}", file))?;

                let editor = RustEditor::new(&content)?;
                let locations = editor.find_node(&node_type, &name)?;

                println!("{}", serde_json::to_string_pretty(&locations)?);
            }
        }

        Commands::Inspect { paths, node_type, name, content_filter, include_comments, format } => {
            use operations::InspectResult;

            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let mut all_results: Vec<InspectResult> = Vec::new();

            for file in files {
                let content = std::fs::read_to_string(&file)
                    .context(format!("Failed to read file: {:?}", file))?;

                let editor = RustEditor::new(&content)?;
                let mut results = editor.inspect(&node_type, name.as_deref(), include_comments)?;

                // Fill in file paths
                for result in &mut results {
                    result.file_path = file.to_string_lossy().to_string();
                }

                // Apply content filter if specified
                if let Some(ref filter) = content_filter {
                    results.retain(|r| r.snippet.contains(filter));
                }

                all_results.extend(results);
            }

            // Format output based on format flag
            match format.as_str() {
                "json" => {
                    println!("{}", serde_json::to_string_pretty(&all_results)?);
                }
                "locations" => {
                    for result in &all_results {
                        println!("{}:{}:{}", result.file_path, result.location.line, result.location.column);
                    }
                }
                "snippets" => {
                    for result in &all_results {
                        println!("// {}:{}:{} - {}",
                            result.file_path,
                            result.location.line,
                            result.location.column,
                            result.identifier);
                        // Show preceding comment if present
                        if let Some(ref comment) = result.preceding_comment {
                            println!("{}", comment);
                        }
                        println!("{}\n", result.snippet);
                    }
                }
                _ => {
                    anyhow::bail!("Unknown format: {}. Use 'json', 'locations', or 'snippets'", format);
                }
            }
        }

        Commands::AddDerive { paths, target_type, name, derives, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let derive_vec: Vec<String> = derives
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();

            let op = Operation::AddDerive(AddDeriveOp {
                target_name: name.clone(),
                target_type: target_type.clone(),
                derives: derive_vec,
                where_filter: cli.r#where.clone(),
            });

            execute_operation(&files, &op, apply, None, &cli.format, cli.summary)?;
        }

        Commands::AddImplMethod { paths, target, method, position, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            let op = Operation::AddImplMethod(AddImplMethodOp {
                target: target.clone(),
                method_def: method.clone(),
                position: parse_position(&position)?,
            });

            execute_operation(&files, &op, apply, None, &cli.format, cli.summary)?;
        }

        Commands::AddUse { paths, use_path, position, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            let op = Operation::AddUseStatement(AddUseStatementOp {
                use_path: use_path.clone(),
                position: parse_position(&position)?,
            });

            execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary)?;
        }

        Commands::History { limit } => {
            let state_dir = get_state_dir(cli.local_state)?;
            show_history(limit, &state_dir)?;
        }

        Commands::Revert { run_id, force } => {
            let state_dir = get_state_dir(cli.local_state)?;
            revert_run(&run_id, force, &state_dir)?;
        }

        Commands::Clean { keep_days } => {
            let state_dir = get_state_dir(cli.local_state)?;
            clean_old_state(keep_days, &state_dir)?;
        }

        Commands::Transform { paths, node_type, name, content_filter, action, with, apply } => {
            use operations::{TransformOp, TransformAction};

            // Parse the action
            let transform_action = match action.as_str() {
                "comment" => TransformAction::Comment,
                "remove" => TransformAction::Remove,
                "replace" => {
                    let replacement = with.ok_or_else(|| anyhow::anyhow!("--with is required when action is 'replace'"))?;
                    TransformAction::Replace { with: replacement }
                }
                _ => anyhow::bail!("Invalid action: {}. Use 'comment', 'remove', or 'replace'", action),
            };

            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;
            let op = Operation::Transform(TransformOp {
                node_type: node_type.clone(),
                name_filter: name,
                content_filter,
                action: transform_action,
            });

            execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary)?;
        }

        Commands::AddDocComment { paths, target_type, name, doc_comment, style, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            // Parse style
            let doc_style = style.parse::<DocCommentStyle>()
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            let op = Operation::AddDocComment(AddDocCommentOp {
                target_type: target_type.clone(),
                name: name.clone(),
                doc_comment: doc_comment.clone(),
                style: doc_style,
            });

            execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary)?;
        }

        Commands::UpdateDocComment { paths, target_type, name, doc_comment, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            let op = Operation::UpdateDocComment(UpdateDocCommentOp {
                target_type: target_type.clone(),
                name: name.clone(),
                doc_comment: doc_comment.clone(),
            });

            execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary)?;
        }

        Commands::RemoveDocComment { paths, target_type, name, apply } => {
            let files = collect_rust_files_with_exclusions(&paths, &cli.exclude)?;

            let op = Operation::RemoveDocComment(RemoveDocCommentOp {
                target_type: target_type.clone(),
                name: name.clone(),
            });

            execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary)?;
        }
    }

    Ok(())
}

fn collect_rust_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    collect_rust_files_with_exclusions(paths, &[])
}

fn collect_rust_files_with_exclusions(paths: &[PathBuf], exclude_patterns: &[String]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for path in paths {
        let path_str = path.to_string_lossy();

        // Check if path contains glob pattern characters
        if path_str.contains('*') || path_str.contains('?') || path_str.contains('[') {
            // Use glob pattern matching
            for entry in glob(&path_str)
                .context("Failed to parse glob pattern")?
            {
                match entry {
                    Ok(file_path) => {
                        if file_path.is_file() && file_path.extension().and_then(|s| s.to_str()) == Some("rs") {
                            files.push(file_path);
                        }
                    }
                    Err(e) => eprintln!("Warning: Error reading glob entry: {}", e),
                }
            }
        } else if path.is_file() {
            if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                files.push(path.clone());
            }
        } else if path.is_dir() {
            for entry in WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("rs"))
            {
                files.push(entry.path().to_path_buf());
            }
        }
    }

    // Filter out excluded paths
    if !exclude_patterns.is_empty() {
        files.retain(|file| {
            let file_str = file.to_string_lossy();
            !exclude_patterns.iter().any(|pattern| {
                // Check if the file matches the exclude pattern
                if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
                    // Use glob matching
                    glob::Pattern::new(pattern)
                        .map(|p| p.matches(&file_str))
                        .unwrap_or(false)
                } else {
                    // Simple string matching for non-glob patterns
                    file_str.contains(pattern.as_str())
                }
            })
        });
    }

    Ok(files)
}

fn parse_position(pos: &str) -> Result<InsertPosition> {
    match pos {
        "first" => Ok(InsertPosition::First),
        "last" => Ok(InsertPosition::Last),
        s if s.starts_with("after:") => {
            let name = s.strip_prefix("after:").unwrap().to_string();
            Ok(InsertPosition::After(name))
        }
        s if s.starts_with("before:") => {
            let name = s.strip_prefix("before:").unwrap().to_string();
            Ok(InsertPosition::Before(name))
        }
        _ => anyhow::bail!("Invalid position: {}. Use 'first', 'last', 'after:name', or 'before:name'", pos),
    }
}

fn execute_operation(
    files: &[PathBuf],
    op: &Operation,
    apply: bool,
    output: Option<&PathBuf>,
    format: &str,
    show_summary: bool,
) -> Result<()> {
    let mut changes = Vec::new();
    let mut total_stats = DiffStats::default();

    for file_path in files {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read {}", file_path.display()))?;

        let mut editor = RustEditor::new(&content)?;

        match editor.apply_operation(op) {
            Ok(result) => {
                if result.changed {
                    let new_content = editor.to_string();
                    changes.push(FileChange {
                        path: file_path.clone(),
                        old_content: content.clone(),
                        new_content: new_content.clone(),
                    });

                    if format == "diff" {
                        // In diff mode, print the diff
                        let stats = print_diff(file_path, &content, &new_content);
                        total_stats.add(&stats);

                        if apply {
                            // Apply changes and show a message
                            let write_path = output.unwrap_or(file_path);
                            std::fs::write(write_path, &new_content)
                                .with_context(|| format!("Failed to write {}", write_path.display()))?;
                        }
                    } else if format == "summary" {
                        // In summary mode, print only changed lines
                        let stats = print_summary_diff(file_path, &content, &new_content);
                        total_stats.add(&stats);

                        if apply {
                            // Apply changes and show a message
                            let write_path = output.unwrap_or(file_path);
                            std::fs::write(write_path, &new_content)
                                .with_context(|| format!("Failed to write {}", write_path.display()))?;
                        }
                    } else {
                        // Default mode - original behavior
                        if apply {
                            let write_path = output.unwrap_or(file_path);
                            std::fs::write(write_path, &new_content)
                                .with_context(|| format!("Failed to write {}", write_path.display()))?;
                            if let Some(out) = output {
                                println!("✓ Written to: {}", out.display());
                            } else {
                                println!("✓ Modified: {}", file_path.display());
                            }
                        } else {
                            if output.is_some() {
                                println!("Would write to: {}", output.unwrap().display());
                            } else {
                                println!("Would modify: {}", file_path.display());
                            }
                        }
                    }
                }
            }
            Err(e) => {
                // Not an error if the target doesn't exist in this file
                if files.len() == 1 {
                    return Err(e);
                }
            }
        }
    }

    if changes.is_empty() {
        println!("No changes made - target not found in any files");
    } else if format == "diff" && show_summary {
        // Print summary for diff mode
        total_stats.print_summary();
    } else if format == "default" && !apply {
        println!("\n🔍 Dry run complete. Use --apply to make changes.");
        println!("Summary: {} file(s) would be modified", changes.len());
    }

    Ok(())
}

fn execute_batch(batch: &BatchSpec, apply: bool, exclude_patterns: &[String]) -> Result<()> {
    for op in &batch.operations {
        let files = collect_rust_files_with_exclusions(&[batch.base_path.clone()], exclude_patterns)?;
        execute_operation(&files, op, apply, None, "default", false)?;
    }
    Ok(())
}

fn execute_operation_with_state(
    files: &[PathBuf],
    op: &Operation,
    apply: bool,
    output: Option<&PathBuf>,
    local_state: &bool,
    format: &str,
    show_summary: bool,
) -> Result<()> {
    // If not applying or output is specified (not in-place), don't track state
    if !apply || output.is_some() {
        return execute_operation(files, op, apply, output, format, show_summary);
    }

    // Generate run ID and get state dir
    let run_id = generate_run_id();
    let state_dir = get_state_dir(*local_state)?;

    // Collect command line for metadata
    let command = std::env::args().collect::<Vec<_>>().join(" ");
    let operation_name = match op {
        Operation::AddStructField(_) => "AddStructField",
        Operation::UpdateStructField(_) => "UpdateStructField",
        Operation::RemoveStructField(_) => "RemoveStructField",
        Operation::AddStructLiteralField(_) => "AddStructLiteralField",
        Operation::AddEnumVariant(_) => "AddEnumVariant",
        Operation::UpdateEnumVariant(_) => "UpdateEnumVariant",
        Operation::RemoveEnumVariant(_) => "RemoveEnumVariant",
        Operation::RenameEnumVariant(_) => "RenameEnumVariant",
        Operation::AddMatchArm(_) => "AddMatchArm",
        Operation::UpdateMatchArm(_) => "UpdateMatchArm",
        Operation::RemoveMatchArm(_) => "RemoveMatchArm",
        Operation::AddImplMethod(_) => "AddImplMethod",
        Operation::AddUseStatement(_) => "AddUseStatement",
        Operation::AddDerive(_) => "AddDerive",
        Operation::Transform(_) => "Transform",
        Operation::RenameFunction(_) => "RenameFunction",
        Operation::AddDocComment(_) => "AddDocComment",
        Operation::UpdateDocComment(_) => "UpdateDocComment",
        Operation::RemoveDocComment(_) => "RemoveDocComment",
    };

    // First pass: collect changes and backup files
    let mut file_modifications = Vec::new();
    let mut changes_made = false;
    let mut total_stats = DiffStats::default();

    for file_path in files {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read {}", file_path.display()))?;

        let mut editor = RustEditor::new(&content)?;

        match editor.apply_operation(op) {
            Ok(result) => {
                if result.changed {
                    let new_content = editor.to_string();

                    // Print diff if format is "diff" or "summary"
                    if format == "diff" {
                        let stats = print_diff(file_path, &content, &new_content);
                        total_stats.add(&stats);
                    } else if format == "summary" {
                        let stats = print_summary_diff(file_path, &content, &new_content);
                        total_stats.add(&stats);
                    }

                    // Save backup nodes before modifying
                    let hash_before = hash_file(file_path)?;
                    save_backup_nodes(file_path, &result.modified_nodes, &run_id, &state_dir)?;

                    // Write the new content
                    std::fs::write(file_path, &new_content)
                        .with_context(|| format!("Failed to write {}", file_path.display()))?;

                    let hash_after = hash_file(file_path)?;

                    file_modifications.push(FileModification {
                        path: file_path.clone(),
                        hash_before,
                        hash_after,
                        backup_nodes: result.modified_nodes.clone(),
                    });

                    if format != "diff" {
                        println!("✓ Modified: {}", file_path.display());
                    }
                    changes_made = true;
                }
            }
            Err(e) => {
                // Not an error if the target doesn't exist in this file
                if files.len() == 1 {
                    return Err(e);
                }
            }
        }
    }

    // Save run metadata if any changes were made
    if !file_modifications.is_empty() {
        let metadata = RunMetadata {
            run_id: run_id.clone(),
            timestamp: chrono::Utc::now(),
            command,
            operation: operation_name.to_string(),
            files_modified: file_modifications,
            status: RunStatus::Applied,
            can_revert: true,
        };

        save_run_metadata(&metadata, &state_dir)?;

        if format == "diff" && show_summary {
            total_stats.print_summary();
        }

        println!("\n📝 Run ID: {} (use 'rs-hack revert {}' to undo)", run_id, run_id);
    } else if !changes_made {
        println!("No changes made - target not found in any files");
    }

    Ok(())
}

#[derive(Debug)]
#[allow(dead_code)]
struct FileChange {
    path: PathBuf,
    old_content: String,
    new_content: String,
}
