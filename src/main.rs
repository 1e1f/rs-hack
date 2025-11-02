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

#[cfg(test)]
mod tests;

use operations::*;
use editor::RustEditor;
use state::{*, save_backup_nodes};
use diff::{print_diff, DiffStats};

#[derive(Parser)]
#[command(name = "rust-ast-edit")]
#[command(about = "AST-aware Rust code editing tool for AI agents", long_about = None)]
struct Cli {
    /// Use project-local state directory (.rs-hack) instead of ~/.rs-hack
    #[arg(long, global = true)]
    local_state: bool,

    /// Output format: "default" or "diff"
    #[arg(long, default_value = "default", global = true)]
    format: String,

    /// Show summary statistics after diff output
    #[arg(long, global = true)]
    summary: bool,

    /// Filter targets based on traits or attributes (e.g., "derives_trait:Clone", "derives_trait:Serialize,Debug")
    #[arg(long, global = true)]
    r#where: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a field to a struct (skips if field already exists)
    AddStructField {
        /// Path to the Rust file or directory
        #[arg(short, long)]
        path: PathBuf,

        /// Name of the struct to modify
        #[arg(short, long)]
        struct_name: String,

        /// Field to add (e.g., "field_name: Type" or "field_name: Option<Type>")
        #[arg(short, long)]
        field: String,

        /// Where to insert: "first", "last", or "after:field_name"
        #[arg(short = 'P', long, default_value = "last")]
        position: String,

        /// Optional: default value for struct literals (e.g., "None", "vec![]", "0")
        /// If provided, also updates all struct initialization expressions
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
        /// Path to the Rust file or directory
        #[arg(short, long)]
        path: PathBuf,

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
        /// Path to the Rust file or directory
        #[arg(short, long)]
        path: PathBuf,

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

    /// Add a field to struct literal expressions (initialization)
    AddStructLiteralField {
        /// Path to the Rust file or directory (supports glob patterns)
        #[arg(short, long)]
        path: PathBuf,

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
        /// Path to the Rust file or directory
        #[arg(short, long)]
        path: PathBuf,

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
        /// Path to the Rust file or directory
        #[arg(short, long)]
        path: PathBuf,

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
        /// Path to the Rust file or directory
        #[arg(short, long)]
        path: PathBuf,

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

    /// Add a match arm for a specific pattern
    AddMatchArm {
        /// Path to the Rust file or directory (supports glob patterns)
        #[arg(short, long)]
        path: PathBuf,

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
        /// Path to the Rust file or directory
        #[arg(short, long)]
        path: PathBuf,

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
        /// Path to the Rust file or directory
        #[arg(short, long)]
        path: PathBuf,

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

    /// Batch operation from JSON specification
    Batch {
        /// Path to JSON file with batch operations
        #[arg(short, long)]
        spec: PathBuf,
        
        /// Apply changes (default is dry-run)
        #[arg(long)]
        apply: bool,
    },
    
    /// Find locations of AST nodes (for debugging/inspection)
    Find {
        /// Path to the Rust file
        #[arg(short, long)]
        path: PathBuf,

        /// Type of node: "struct", "enum", "fn", "impl"
        #[arg(short = 't', long)]
        node_type: String,

        /// Name of the node
        #[arg(short, long)]
        name: String,
    },

    /// Inspect and list AST nodes with full content (supports glob patterns)
    Inspect {
        /// Path to Rust file(s) - supports glob patterns (e.g., "tests/*.rs")
        #[arg(short, long)]
        path: PathBuf,

        /// Type of node: "struct-literal", "match-arm", "enum-usage", "function-call", "method-call", "identifier", "type-ref"
        #[arg(short = 't', long)]
        node_type: String,

        /// Filter by name (e.g., "Shadow", "Operator::Error", "unwrap", "handle_error", "Vec")
        #[arg(short, long)]
        name: Option<String>,

        /// Output format: "json", "locations", "snippets"
        #[arg(short = 'f', long, default_value = "snippets")]
        format: String,
    },

    /// Add derive macros to a struct or enum
    AddDerive {
        /// Path to the Rust file or directory
        #[arg(short, long)]
        path: PathBuf,

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
        /// Path to the Rust file or directory
        #[arg(short, long)]
        path: PathBuf,

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
        /// Path to the Rust file or directory
        #[arg(short, long)]
        path: PathBuf,

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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::AddStructField { path, struct_name, field, position, literal_default, output, apply } => {
            let files = collect_rust_files(&path)?;
            let op = Operation::AddStructField(AddStructFieldOp {
                struct_name: struct_name.clone(),
                field_def: field.clone(),
                position: parse_position(&position)?,
                literal_default,
                where_filter: cli.r#where.clone(),
            });

            execute_operation_with_state(&files, &op, apply, output.as_ref(), &cli.local_state, &cli.format, cli.summary)?;
        }

        Commands::UpdateStructField { path, struct_name, field, output, apply } => {
            let files = collect_rust_files(&path)?;
            let op = Operation::UpdateStructField(UpdateStructFieldOp {
                struct_name: struct_name.clone(),
                field_def: field.clone(),
                where_filter: cli.r#where.clone(),
            });

            execute_operation_with_state(&files, &op, apply, output.as_ref(), &cli.local_state, &cli.format, cli.summary)?;
        }

        Commands::RemoveStructField { path, struct_name, field_name, output, apply } => {
            let files = collect_rust_files(&path)?;
            let op = Operation::RemoveStructField(RemoveStructFieldOp {
                struct_name: struct_name.clone(),
                field_name: field_name.clone(),
                where_filter: cli.r#where.clone(),
            });

            execute_operation_with_state(&files, &op, apply, output.as_ref(), &cli.local_state, &cli.format, cli.summary)?;
        }

        Commands::AddStructLiteralField { path, struct_name, field, position, apply } => {
            let files = collect_rust_files(&path)?;
            let op = Operation::AddStructLiteralField(AddStructLiteralFieldOp {
                struct_name: struct_name.clone(),
                field_def: field.clone(),
                position: parse_position(&position)?,
            });

            execute_operation_with_state(&files, &op, apply, None, &cli.local_state, &cli.format, cli.summary)?;
        }

        Commands::AddEnumVariant { path, enum_name, variant, position, output, apply } => {
            let files = collect_rust_files(&path)?;
            let op = Operation::AddEnumVariant(AddEnumVariantOp {
                enum_name: enum_name.clone(),
                variant_def: variant.clone(),
                position: parse_position(&position)?,
                where_filter: cli.r#where.clone(),
            });

            execute_operation_with_state(&files, &op, apply, output.as_ref(), &cli.local_state, &cli.format, cli.summary)?;
        }

        Commands::UpdateEnumVariant { path, enum_name, variant, output, apply } => {
            let files = collect_rust_files(&path)?;
            let op = Operation::UpdateEnumVariant(UpdateEnumVariantOp {
                enum_name: enum_name.clone(),
                variant_def: variant.clone(),
                where_filter: cli.r#where.clone(),
            });

            execute_operation_with_state(&files, &op, apply, output.as_ref(), &cli.local_state, &cli.format, cli.summary)?;
        }

        Commands::RemoveEnumVariant { path, enum_name, variant_name, output, apply } => {
            let files = collect_rust_files(&path)?;
            let op = Operation::RemoveEnumVariant(RemoveEnumVariantOp {
                enum_name: enum_name.clone(),
                variant_name: variant_name.clone(),
                where_filter: cli.r#where.clone(),
            });

            execute_operation_with_state(&files, &op, apply, output.as_ref(), &cli.local_state, &cli.format, cli.summary)?;
        }

        Commands::AddMatchArm { path, pattern, body, function, auto_detect, enum_name, apply } => {
            // Validate auto_detect requires enum_name
            if auto_detect && enum_name.is_none() {
                anyhow::bail!("--enum-name is required when using --auto-detect");
            }

            // Validate pattern is provided when not using auto_detect
            if !auto_detect && pattern.is_none() {
                anyhow::bail!("--pattern is required when not using --auto-detect");
            }

            let files = collect_rust_files(&path)?;
            let op = Operation::AddMatchArm(AddMatchArmOp {
                pattern: pattern.unwrap_or_default(),
                body: body.clone(),
                function_name: function,
                auto_detect,
                enum_name,
            });

            execute_operation(&files, &op, apply, None, &cli.format, cli.summary)?;
        }

        Commands::UpdateMatchArm { path, pattern, body, function, apply } => {
            let files = collect_rust_files(&path)?;
            let op = Operation::UpdateMatchArm(UpdateMatchArmOp {
                pattern: pattern.clone(),
                new_body: body.clone(),
                function_name: function,
            });

            execute_operation(&files, &op, apply, None, &cli.format, cli.summary)?;
        }

        Commands::RemoveMatchArm { path, pattern, function, apply } => {
            let files = collect_rust_files(&path)?;
            let op = Operation::RemoveMatchArm(RemoveMatchArmOp {
                pattern: pattern.clone(),
                function_name: function,
            });

            execute_operation(&files, &op, apply, None, &cli.format, cli.summary)?;
        }

        Commands::Batch { spec, apply } => {
            let content = std::fs::read_to_string(&spec)
                .context("Failed to read batch spec file")?;
            let batch: BatchSpec = serde_json::from_str(&content)
                .context("Failed to parse batch spec JSON")?;

            execute_batch(&batch, apply)?;
        }
        
        Commands::Find { path, node_type, name } => {
            let content = std::fs::read_to_string(&path)
                .context("Failed to read file")?;

            let editor = RustEditor::new(&content)?;
            let locations = editor.find_node(&node_type, &name)?;

            println!("{}", serde_json::to_string_pretty(&locations)?);
        }

        Commands::Inspect { path, node_type, name, format } => {
            use operations::InspectResult;

            let files = collect_rust_files(&path)?;
            let mut all_results: Vec<InspectResult> = Vec::new();

            for file in files {
                let content = std::fs::read_to_string(&file)
                    .context(format!("Failed to read file: {:?}", file))?;

                let editor = RustEditor::new(&content)?;
                let mut results = editor.inspect(&node_type, name.as_deref())?;

                // Fill in file paths
                for result in &mut results {
                    result.file_path = file.to_string_lossy().to_string();
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
                        println!("{}\n", result.snippet);
                    }
                }
                _ => {
                    anyhow::bail!("Unknown format: {}. Use 'json', 'locations', or 'snippets'", format);
                }
            }
        }

        Commands::AddDerive { path, target_type, name, derives, apply } => {
            let files = collect_rust_files(&path)?;
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

        Commands::AddImplMethod { path, target, method, position, apply } => {
            let files = collect_rust_files(&path)?;

            let op = Operation::AddImplMethod(AddImplMethodOp {
                target: target.clone(),
                method_def: method.clone(),
                position: parse_position(&position)?,
            });

            execute_operation(&files, &op, apply, None, &cli.format, cli.summary)?;
        }

        Commands::AddUse { path, use_path, position, apply } => {
            let files = collect_rust_files(&path)?;

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
    }

    Ok(())
}

fn collect_rust_files(path: &PathBuf) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
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

fn execute_batch(batch: &BatchSpec, apply: bool) -> Result<()> {
    for op in &batch.operations {
        let files = collect_rust_files(&batch.base_path)?;
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
        Operation::AddMatchArm(_) => "AddMatchArm",
        Operation::UpdateMatchArm(_) => "UpdateMatchArm",
        Operation::RemoveMatchArm(_) => "RemoveMatchArm",
        Operation::AddImplMethod(_) => "AddImplMethod",
        Operation::AddUseStatement(_) => "AddUseStatement",
        Operation::AddDerive(_) => "AddDerive",
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

                    // Print diff if format is "diff"
                    if format == "diff" {
                        let stats = print_diff(file_path, &content, &new_content);
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
