use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Edit mode for operations - controls how changes are applied to source files
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditMode {
    /// Surgical mode: preserve all formatting, only change specific locations
    /// This is the recommended default for minimal diffs
    Surgical,
    /// Reformat mode: use prettyplease to reformat the entire file
    /// Use this if you want consistent formatting across the file
    Reformat,
}

impl Default for EditMode {
    fn default() -> Self {
        EditMode::Surgical
    }
}

impl std::fmt::Display for EditMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditMode::Surgical => write!(f, "surgical"),
            EditMode::Reformat => write!(f, "reformat"),
        }
    }
}

impl std::str::FromStr for EditMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "surgical" => Ok(EditMode::Surgical),
            "reformat" => Ok(EditMode::Reformat),
            _ => Err(format!("Invalid edit mode: {}. Valid values are 'surgical' or 'reformat'", s)),
        }
    }
}

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
    RenameFunction(RenameFunctionOp),
    AddDocComment(AddDocCommentOp),
    UpdateDocComment(UpdateDocCommentOp),
    RemoveDocComment(RemoveDocCommentOp),
    SetStructLiteralBase(SetStructLiteralBaseOp),
    AddCallArg(AddCallArgOp),
    UpdateCallArg(UpdateCallArgOp),
    RemoveCallArg(RemoveCallArgOp),
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
    pub literal_only: bool, // If true, only remove from struct literals, not the definition
    #[serde(default)]
    pub where_filter: Option<String>, // Optional: filter targets (e.g., "derives_trait:Clone")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddStructLiteralFieldOp {
    pub struct_name: String,
    pub field_def: String, // e.g., "return_type: None"
    pub position: InsertPosition,
    #[serde(default)]
    pub struct_path: Option<String>,  // Optional canonical path (e.g., "crate::types::Rectangle")
}

/// Add or set the base expression (..expr) on struct literals
/// e.g., adds `..Default::default()` to `Foo { a: 1 }` → `Foo { a: 1, ..Default::default() }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetStructLiteralBaseOp {
    pub struct_name: String,
    /// The base expression (e.g., "Default::default()" or just "default")
    /// If "default", expands to "Default::default()"
    pub base_expr: String,
    #[serde(default)]
    pub struct_path: Option<String>,  // Optional canonical path (e.g., "crate::types::Rectangle")
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
    /// Unmatched qualified paths (only populated for struct literal operations with simple names)
    /// Maps fully qualified path to count of instances found but not matched
    pub unmatched_qualified_paths: Option<std::collections::HashMap<String, usize>>,
}

/// Result of inspecting/listing AST nodes
#[derive(Debug, Serialize, Deserialize)]
pub struct InspectResult {
    pub file_path: String,
    pub node_type: String,      // "ExprStruct", "ExprMatch", etc.
    pub identifier: String,      // "Shadow", "Config", etc.
    pub location: NodeLocation,
    pub snippet: String,         // Formatted code snippet
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preceding_comment: Option<String>,  // Doc comments + regular comments before the node
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
    #[serde(default)]
    pub enum_path: Option<String>,  // Optional canonical path (e.g., "crate::compiler::types::IRValue")
    #[serde(default)]
    pub edit_mode: EditMode,    // How to apply changes (surgical vs reformat)
}

/// Rename a function across the codebase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameFunctionOp {
    pub old_name: String,       // Current function name (e.g., "process_v2")
    pub new_name: String,       // New function name (e.g., "process")
    #[serde(default)]
    pub function_path: Option<String>,  // Optional canonical path (e.g., "crate::utils::process_v2")
    #[serde(default)]
    pub edit_mode: EditMode,    // How to apply changes (surgical vs reformat)
}

/// Add documentation comment to an item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddDocCommentOp {
    pub target_type: String,    // "struct", "enum", "function", "field", "variant"
    pub name: String,           // Name of the target (e.g., "User", "Status::Draft")
    pub doc_comment: String,    // Documentation text (without /// prefix)
    #[serde(default)]
    pub style: DocCommentStyle, // Line (///) or Block (/** */)
}

/// Update existing documentation comment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateDocCommentOp {
    pub target_type: String,    // "struct", "enum", "function", "field", "variant"
    pub name: String,           // Name of the target
    pub doc_comment: String,    // New documentation text
}

/// Remove documentation comment from an item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveDocCommentOp {
    pub target_type: String,    // "struct", "enum", "function", "field", "variant"
    pub name: String,           // Name of the target
}

/// Documentation comment style
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocCommentStyle {
    Line,   // /// or //!
    Block,  // /** */ or /*! */
}

impl Default for DocCommentStyle {
    fn default() -> Self {
        DocCommentStyle::Line
    }
}

impl std::str::FromStr for DocCommentStyle {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "line" => Ok(DocCommentStyle::Line),
            "block" => Ok(DocCommentStyle::Block),
            _ => Err(format!("Invalid doc comment style: {}. Valid values are 'line' or 'block'", s)),
        }
    }
}

/// Location of a field in the codebase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldLocation {
    pub file_path: String,
    pub line: usize,
    pub context: FieldContext,
}

/// Insert position for call arguments (numeric since args are positional)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArgPosition {
    /// Insert as first argument
    First,
    /// Insert as last argument
    Last,
    /// Insert at specific index (0-based, shifts existing args right)
    Index(usize),
}

impl Default for ArgPosition {
    fn default() -> Self {
        ArgPosition::Last
    }
}

impl std::fmt::Display for ArgPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArgPosition::First => write!(f, "first"),
            ArgPosition::Last => write!(f, "last"),
            ArgPosition::Index(i) => write!(f, "index:{}", i),
        }
    }
}

impl std::str::FromStr for ArgPosition {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "first" => Ok(ArgPosition::First),
            "last" => Ok(ArgPosition::Last),
            s if s.starts_with("index:") => {
                let idx = s[6..].parse::<usize>()
                    .map_err(|_| format!("Invalid index in position: {}", s))?;
                Ok(ArgPosition::Index(idx))
            }
            s => {
                // Try parsing as plain number
                if let Ok(idx) = s.parse::<usize>() {
                    Ok(ArgPosition::Index(idx))
                } else {
                    Err(format!("Invalid arg position: {}. Valid values are 'first', 'last', or 'index:N'", s))
                }
            }
        }
    }
}

/// Add an argument to function or method calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddCallArgOp {
    /// Name of the function or method to target
    pub call_name: String,
    /// Expression to add as argument (e.g., "None", "ctx.clone()", "Default::default()")
    pub arg_expr: String,
    /// Where to insert the argument
    #[serde(default)]
    pub position: ArgPosition,
    /// Filter to "function" or "method" calls only (None = both)
    #[serde(default)]
    pub call_type: Option<String>,
    /// Filter call sites by content substring
    #[serde(default)]
    pub content_filter: Option<String>,
}

/// Update an argument at a specific index in function or method calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCallArgOp {
    /// Name of the function or method to target
    pub call_name: String,
    /// Index of the argument to update (0-based)
    pub arg_index: usize,
    /// New expression for the argument
    pub new_expr: String,
    /// Filter to "function" or "method" calls only (None = both)
    #[serde(default)]
    pub call_type: Option<String>,
    /// Filter call sites by content substring
    #[serde(default)]
    pub content_filter: Option<String>,
}

/// Remove an argument at a specific index from function or method calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveCallArgOp {
    /// Name of the function or method to target
    pub call_name: String,
    /// Index of the argument to remove (0-based)
    pub arg_index: usize,
    /// Filter to "function" or "method" calls only (None = both)
    #[serde(default)]
    pub call_type: Option<String>,
    /// Filter call sites by content substring
    #[serde(default)]
    pub content_filter: Option<String>,
}

/// Context in which a field appears
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FieldContext {
    StructDefinition {
        struct_name: String,
        field_type: String,
    },
    EnumVariantDefinition {
        enum_name: String,
        variant_name: String,
        field_type: String,
    },
    StructLiteral {
        struct_name: String,
    },
}
