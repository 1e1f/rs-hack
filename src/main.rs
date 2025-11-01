use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use walkdir::WalkDir;

mod operations;
mod visitor;
mod editor;

use operations::*;
use editor::RustEditor;

#[derive(Parser)]
#[command(name = "rust-ast-edit")]
#[command(about = "AST-aware Rust code editing tool for AI agents", long_about = None)]
struct Cli {
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
        /// Path to the Rust file or directory
        #[arg(short, long)]
        path: PathBuf,

        /// Pattern to match (e.g., "MyEnum::NewVariant")
        #[arg(short = 'P', long)]
        pattern: String,

        /// Body of the match arm (e.g., "todo!()" or "println!(\"handled\")")
        #[arg(short, long)]
        body: String,

        /// Optional: function name containing the match
        #[arg(short, long)]
        function: Option<String>,

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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::AddStructField { path, struct_name, field, position, output, apply } => {
            let files = collect_rust_files(&path)?;
            let op = Operation::AddStructField(AddStructFieldOp {
                struct_name: struct_name.clone(),
                field_def: field.clone(),
                position: parse_position(&position)?,
            });

            execute_operation(&files, &op, apply, output.as_ref())?;
        }

        Commands::UpdateStructField { path, struct_name, field, output, apply } => {
            let files = collect_rust_files(&path)?;
            let op = Operation::UpdateStructField(UpdateStructFieldOp {
                struct_name: struct_name.clone(),
                field_def: field.clone(),
            });

            execute_operation(&files, &op, apply, output.as_ref())?;
        }

        Commands::RemoveStructField { path, struct_name, field_name, output, apply } => {
            let files = collect_rust_files(&path)?;
            let op = Operation::RemoveStructField(RemoveStructFieldOp {
                struct_name: struct_name.clone(),
                field_name: field_name.clone(),
            });

            execute_operation(&files, &op, apply, output.as_ref())?;
        }

        Commands::AddEnumVariant { path, enum_name, variant, position, output, apply } => {
            let files = collect_rust_files(&path)?;
            let op = Operation::AddEnumVariant(AddEnumVariantOp {
                enum_name: enum_name.clone(),
                variant_def: variant.clone(),
                position: parse_position(&position)?,
            });

            execute_operation(&files, &op, apply, output.as_ref())?;
        }

        Commands::UpdateEnumVariant { path, enum_name, variant, output, apply } => {
            let files = collect_rust_files(&path)?;
            let op = Operation::UpdateEnumVariant(UpdateEnumVariantOp {
                enum_name: enum_name.clone(),
                variant_def: variant.clone(),
            });

            execute_operation(&files, &op, apply, output.as_ref())?;
        }

        Commands::RemoveEnumVariant { path, enum_name, variant_name, output, apply } => {
            let files = collect_rust_files(&path)?;
            let op = Operation::RemoveEnumVariant(RemoveEnumVariantOp {
                enum_name: enum_name.clone(),
                variant_name: variant_name.clone(),
            });

            execute_operation(&files, &op, apply, output.as_ref())?;
        }

        Commands::AddMatchArm { path, pattern, body, function, apply } => {
            let files = collect_rust_files(&path)?;
            let op = Operation::AddMatchArm(AddMatchArmOp {
                pattern: pattern.clone(),
                body: body.clone(),
                function_name: function,
            });

            execute_operation(&files, &op, apply, None)?;
        }

        Commands::UpdateMatchArm { path, pattern, body, function, apply } => {
            let files = collect_rust_files(&path)?;
            let op = Operation::UpdateMatchArm(UpdateMatchArmOp {
                pattern: pattern.clone(),
                new_body: body.clone(),
                function_name: function,
            });

            execute_operation(&files, &op, apply, None)?;
        }

        Commands::RemoveMatchArm { path, pattern, function, apply } => {
            let files = collect_rust_files(&path)?;
            let op = Operation::RemoveMatchArm(RemoveMatchArmOp {
                pattern: pattern.clone(),
                function_name: function,
            });

            execute_operation(&files, &op, apply, None)?;
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
            });

            execute_operation(&files, &op, apply, None)?;
        }

        Commands::AddImplMethod { path, target, method, position, apply } => {
            let files = collect_rust_files(&path)?;

            let op = Operation::AddImplMethod(AddImplMethodOp {
                target: target.clone(),
                method_def: method.clone(),
                position: parse_position(&position)?,
            });

            execute_operation(&files, &op, apply, None)?;
        }

        Commands::AddUse { path, use_path, position, apply } => {
            let files = collect_rust_files(&path)?;

            let op = Operation::AddUseStatement(AddUseStatementOp {
                use_path: use_path.clone(),
                position: parse_position(&position)?,
            });

            execute_operation(&files, &op, apply, None)?;
        }
    }

    Ok(())
}

fn collect_rust_files(path: &PathBuf) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    
    if path.is_file() {
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

fn execute_operation(files: &[PathBuf], op: &Operation, apply: bool, output: Option<&PathBuf>) -> Result<()> {
    let mut changes = Vec::new();

    for file_path in files {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read {}", file_path.display()))?;

        let mut editor = RustEditor::new(&content)?;

        match editor.apply_operation(op) {
            Ok(modified) => {
                if modified {
                    let new_content = editor.to_string();
                    changes.push(FileChange {
                        path: file_path.clone(),
                        old_content: content,
                        new_content: new_content.clone(),
                    });

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
    } else if !apply {
        println!("\n🔍 Dry run complete. Use --apply to make changes.");
        println!("Summary: {} file(s) would be modified", changes.len());
    }

    Ok(())
}

fn execute_batch(batch: &BatchSpec, apply: bool) -> Result<()> {
    for op in &batch.operations {
        let files = collect_rust_files(&batch.base_path)?;
        execute_operation(&files, op, apply, None)?;
    }
    Ok(())
}

#[derive(Debug)]
struct FileChange {
    path: PathBuf,
    old_content: String,
    new_content: String,
}
