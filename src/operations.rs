use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Operation {
    AddStructField(AddStructFieldOp),
    UpdateStructField(UpdateStructFieldOp),
    RemoveStructField(RemoveStructFieldOp),
    AddStructLiteralField(AddStructLiteralFieldOp),
    AddEnumVariant(AddEnumVariantOp),
    UpdateEnumVariant(UpdateEnumVariantOp),
    RemoveEnumVariant(RemoveEnumVariantOp),
    AddMatchArm(AddMatchArmOp),
    UpdateMatchArm(UpdateMatchArmOp),
    RemoveMatchArm(RemoveMatchArmOp),
    AddImplMethod(AddImplMethodOp),
    AddUseStatement(AddUseStatementOp),
    AddDerive(AddDeriveOp),
    Transform(TransformOp),
    RenameEnumVariant(RenameEnumVariantOp),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddStructFieldOp {
    pub struct_name: String,
    pub field_def: String, // e.g., "new_field: Option<String>" or just "new_field" if literal_default is provided
    pub position: InsertPosition,
    #[serde(default)]
    pub literal_default: Option<String>, // If provided: tries to add to definition (idempotent), always updates literals
    #[serde(default)]
    pub where_filter: Option<String>, // Optional: filter targets (e.g., "derives_trait:Clone")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStructFieldOp {
    pub struct_name: String,
    pub field_def: String, // e.g., "field_name: NewType" (field name is parsed from this)
    #[serde(default)]
    pub where_filter: Option<String>, // Optional: filter targets (e.g., "derives_trait:Clone")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveStructFieldOp {
    pub struct_name: String,
    pub field_name: String, // Name of the field to remove
    #[serde(default)]
    pub where_filter: Option<String>, // Optional: filter targets (e.g., "derives_trait:Clone")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddStructLiteralFieldOp {
    pub struct_name: String,
    pub field_def: String, // e.g., "return_type: None"
    pub position: InsertPosition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddEnumVariantOp {
    pub enum_name: String,
    pub variant_def: String, // e.g., "NewVariant" or "NewVariant { x: i32 }"
    pub position: InsertPosition,
    #[serde(default)]
    pub where_filter: Option<String>, // Optional: filter targets (e.g., "derives_trait:Clone")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateEnumVariantOp {
    pub enum_name: String,
    pub variant_def: String, // e.g., "UpdatedVariant { new_field: Type }" (variant name parsed from this)
    #[serde(default)]
    pub where_filter: Option<String>, // Optional: filter targets (e.g., "derives_trait:Clone")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveEnumVariantOp {
    pub enum_name: String,
    pub variant_name: String, // Name of the variant to remove
    #[serde(default)]
    pub where_filter: Option<String>, // Optional: filter targets (e.g., "derives_trait:Clone")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddMatchArmOp {
    pub pattern: String, // e.g., "MyEnum::NewVariant"
    pub body: String,    // e.g., "todo!()"
    pub function_name: Option<String>, // Optional: specific function containing match
    #[serde(default)]
    pub auto_detect: bool, // Auto-detect missing enum variants
    pub enum_name: Option<String>, // Enum name for auto-detection
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMatchArmOp {
    pub pattern: String, // Pattern to find (e.g., "MyEnum::Variant")
    pub new_body: String, // New body for the arm
    pub function_name: Option<String>, // Optional: specific function containing match
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveMatchArmOp {
    pub pattern: String, // Pattern to remove (e.g., "MyEnum::Variant")
    pub function_name: Option<String>, // Optional: specific function containing match
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddImplMethodOp {
    pub target: String, // e.g., "MyStruct" or "impl MyTrait for MyStruct"
    pub method_def: String, // Full method definition
    pub position: InsertPosition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddUseStatementOp {
    pub use_path: String, // e.g., "std::collections::HashMap"
    pub position: InsertPosition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddDeriveOp {
    pub target_name: String, // Name of struct or enum
    pub target_type: String, // "struct" or "enum"
    pub derives: Vec<String>, // e.g., ["Clone", "Debug", "Serialize"]
    #[serde(default)]
    pub where_filter: Option<String>, // Optional: filter targets (e.g., "derives_trait:Clone")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InsertPosition {
    First,
    Last,
    After(String),  // After named item
    Before(String), // Before named item
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchSpec {
    pub base_path: PathBuf,
    pub operations: Vec<Operation>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeLocation {
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

/// Backup of a single AST node before modification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupNode {
    pub node_type: String,        // "ItemStruct", "ItemEnum", "ItemImpl", "ExprStruct", "ExprMatch"
    pub identifier: String,        // "User", "Status::Draft", "process_event", etc.
    pub original_content: String,  // Original AST node as formatted code
    pub location: NodeLocation,
}

/// Result of applying an operation
#[derive(Debug)]
pub struct ModificationResult {
    pub changed: bool,
    pub modified_nodes: Vec<BackupNode>,
}

/// Result of inspecting/listing AST nodes
#[derive(Debug, Serialize, Deserialize)]
pub struct InspectResult {
    pub file_path: String,
    pub node_type: String,      // "ExprStruct", "ExprMatch", etc.
    pub identifier: String,      // "Shadow", "Config", etc.
    pub location: NodeLocation,
    pub snippet: String,         // Formatted code snippet
}

/// Generic transformation operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformOp {
    pub node_type: String,           // "macro-call", "method-call", etc.
    pub name_filter: Option<String>, // Filter by name (e.g., "eprintln")
    pub content_filter: Option<String>, // Filter by content (e.g., "[SHADOW RENDER]")
    pub action: TransformAction,     // What to do with matching nodes
}

/// Actions that can be performed on AST nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TransformAction {
    Comment,                    // Wrap in // comment
    Remove,                     // Delete the node entirely
    Replace { with: String },   // Replace with provided code
}

/// Rename an enum variant across the codebase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameEnumVariantOp {
    pub enum_name: String,      // Name of the enum (e.g., "IRValue")
    pub old_variant: String,    // Current variant name (e.g., "HashMapV2")
    pub new_variant: String,    // New variant name (e.g., "HashMap")
}
