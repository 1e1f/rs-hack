use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Operation {
    AddStructField(AddStructFieldOp),
    UpdateStructField(UpdateStructFieldOp),
    RemoveStructField(RemoveStructFieldOp),
    AddEnumVariant(AddEnumVariantOp),
    UpdateEnumVariant(UpdateEnumVariantOp),
    RemoveEnumVariant(RemoveEnumVariantOp),
    AddMatchArm(AddMatchArmOp),
    UpdateMatchArm(UpdateMatchArmOp),
    RemoveMatchArm(RemoveMatchArmOp),
    AddImplMethod(AddImplMethodOp),
    AddUseStatement(AddUseStatementOp),
    AddDerive(AddDeriveOp),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddStructFieldOp {
    pub struct_name: String,
    pub field_def: String, // e.g., "new_field: Option<String>"
    pub position: InsertPosition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStructFieldOp {
    pub struct_name: String,
    pub field_def: String, // e.g., "field_name: NewType" (field name is parsed from this)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveStructFieldOp {
    pub struct_name: String,
    pub field_name: String, // Name of the field to remove
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddEnumVariantOp {
    pub enum_name: String,
    pub variant_def: String, // e.g., "NewVariant" or "NewVariant { x: i32 }"
    pub position: InsertPosition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateEnumVariantOp {
    pub enum_name: String,
    pub variant_def: String, // e.g., "UpdatedVariant { new_field: Type }" (variant name parsed from this)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveEnumVariantOp {
    pub enum_name: String,
    pub variant_name: String, // Name of the variant to remove
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddMatchArmOp {
    pub pattern: String, // e.g., "MyEnum::NewVariant"
    pub body: String,    // e.g., "todo!()"
    pub function_name: Option<String>, // Optional: specific function containing match
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

#[derive(Debug, Serialize)]
pub struct NodeLocation {
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}
