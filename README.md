# rust-ast-edit

AST-aware Rust code editing tool designed for AI agents and automated refactoring.

## Why?

String-based search and replace (sed, Python regex) is fragile for code:
- Breaks on formatting changes
- Can't distinguish between similar patterns
- No semantic understanding
- Risk of partial matches

This tool uses Rust's `syn` parser to make **precise, AST-aware edits** based on actual code structure.

## Use Cases

Perfect for AI agents making systematic changes across codebases:

✅ **Migration tasks**: "Add `#[derive(Clone)]` to all structs"
✅ **API updates**: "Add new field with default to 50 struct definitions"
✅ **Enum expansion**: "Add `Unknown` variant to all enums, update matches"
✅ **Code generation**: "Add builder methods to all structs with >3 fields"

## Installation

```bash
cargo install --path .
```

## Usage

### Struct Operations

#### Add Field
```bash
# Add field (idempotent - skips if exists)
rust-ast-edit add-struct-field \
  --path src/models.rs \
  --struct-name User \
  --field "email: String" \
  --apply

# With position control
rust-ast-edit add-struct-field \
  --path src/ \
  --struct-name Config \
  --field "timeout_ms: u64" \
  --position "after:port" \
  --apply

# Write to different file (non-destructive)
rust-ast-edit add-struct-field \
  --path src/user.rs \
  --struct-name User \
  --field "age: u32" \
  --output /tmp/modified.rs \
  --apply
```

#### Update Field
```bash
# Change field visibility
rust-ast-edit update-struct-field \
  --path src/models.rs \
  --struct-name User \
  --field "pub email: String" \
  --apply

# Change field type
rust-ast-edit update-struct-field \
  --path src/models.rs \
  --struct-name User \
  --field "id: i64" \
  --apply
```

#### Remove Field
```bash
# Remove field cleanly
rust-ast-edit remove-struct-field \
  --path src/models.rs \
  --struct-name User \
  --field-name deprecated_field \
  --apply
```

### Enum Operations

#### Add Variant
```bash
# Add simple variant (idempotent)
rust-ast-edit add-enum-variant \
  --path src/types.rs \
  --enum-name Status \
  --variant "Pending" \
  --apply

# Add variant with data
rust-ast-edit add-enum-variant \
  --path src/types.rs \
  --enum-name Message \
  --variant "Error { code: i32, msg: String }" \
  --apply
```

#### Update Variant
```bash
# Add fields to existing variant
rust-ast-edit update-enum-variant \
  --path src/types.rs \
  --enum-name Status \
  --variant "Draft { created_at: u64 }" \
  --apply
```

#### Remove Variant
```bash
# Remove variant cleanly
rust-ast-edit remove-enum-variant \
  --path src/types.rs \
  --enum-name Status \
  --variant-name Deprecated \
  --apply
```

### Match Arm Operations

#### Add Match Arm
```bash
# Add match arm (idempotent)
rust-ast-edit add-match-arm \
  --path src/handler.rs \
  --pattern "Status::Archived" \
  --body '"archived".to_string()' \
  --function handle_status \
  --apply
```

#### Update Match Arm
```bash
# Update existing match arm body
rust-ast-edit update-match-arm \
  --path src/handler.rs \
  --pattern "Status::Draft" \
  --body '"pending".to_string()' \
  --function handle_status \
  --apply
```

#### Remove Match Arm
```bash
# Remove match arm
rust-ast-edit remove-match-arm \
  --path src/handler.rs \
  --pattern "Status::Deleted" \
  --function handle_status \
  --apply
```

**Note:** Match operations automatically format the modified function using `prettyplease` to ensure consistent, readable code.

### Find AST Nodes

```bash
# Find struct location
rust-ast-edit find \
  --path src/models.rs \
  --node-type struct \
  --name User

# Output:
# [{
#   "line": 10,
#   "column": 0,
#   "end_line": 15,
#   "end_column": 1
# }]
```

### Batch Operations

Create a JSON file with multiple operations:

```json
{
  "base_path": "src/",
  "operations": [
    {
      "type": "AddStructField",
      "struct_name": "User",
      "field_def": "created_at: Option<DateTime<Utc>>",
      "position": "Last"
    },
    {
      "type": "AddStructField",
      "struct_name": "Post",
      "field_def": "created_at: Option<DateTime<Utc>>",
      "position": "Last"
    },
    {
      "type": "AddEnumVariant",
      "enum_name": "Status",
      "variant_def": "Archived",
      "position": "Last"
    }
  ]
}
```

Run batch:

```bash
rust-ast-edit batch --spec migrations.json --apply
```

## AI Agent Integration

### Example: Claude using rust-ast-edit

```bash
# Claude reads the task
"Add an 'updated_at: Option<DateTime>' field to all structs in src/models/"

# Claude executes
rust-ast-edit add-struct-field \
  --path src/models/ \
  --struct-name User \
  --field "updated_at: Option<DateTime<Utc>>" \
  --position last \
  --apply

rust-ast-edit add-struct-field \
  --path src/models/ \
  --struct-name Post \
  --field "updated_at: Option<DateTime<Utc>>" \
  --position last \
  --apply
```

### Example: Batch Migration

```bash
# User: "We're migrating from Status enum to have a new 'Archived' state. 
#       Add the variant and update all matches to handle it with todo!()"

# Claude creates batch spec:
cat > migration.json << 'EOF'
{
  "base_path": "src/",
  "operations": [
    {
      "type": "AddEnumVariant",
      "enum_name": "Status",
      "variant_def": "Archived",
      "position": "Last"
    }
  ]
}
EOF

rust-ast-edit batch --spec migration.json --apply
```

## Architecture

### Core Components

1. **Parser** (`syn` crate): Parses Rust → AST
2. **Editor**: Manipulates AST and tracks byte positions
3. **Operations**: Type-safe operation definitions
4. **CLI**: User-friendly interface

### Key Design Decisions

- **Preserves formatting**: Inserts at exact byte positions
- **Idempotent**: Running twice doesn't duplicate changes
- **Fail-fast**: Returns errors clearly, doesn't corrupt code
- **Dry-run default**: Must explicitly `--apply` to modify files

## Supported Operations

### Struct Operations
- ✅ `AddStructField`: Add fields to structs (idempotent)
- ✅ `UpdateStructField`: Update existing field types/visibility
- ✅ `RemoveStructField`: Remove fields from structs

### Enum Operations
- ✅ `AddEnumVariant`: Add variants to enums (idempotent)
- ✅ `UpdateEnumVariant`: Update existing variant structure
- ✅ `RemoveEnumVariant`: Remove variants from enums

### Match Arm Operations
- ✅ `AddMatchArm`: Add arms to match expressions (idempotent)
- ✅ `UpdateMatchArm`: Update existing match arm bodies
- ✅ `RemoveMatchArm`: Remove match arms

**Note:** Match operations automatically format the modified function using `prettyplease` to ensure proper formatting without affecting the rest of the file.

### Future Operations
- 🚧 `AddImplMethod`: Add methods to impl blocks
- 🚧 `AddUseStatement`: Add use statements
- 🚧 `AddDerive`: Add derive macros

Legend: ✅ Implemented (9 operations) | 🚧 Planned (3 operations)

## Comparison with Alternatives

| Tool | AST-Aware | Rust-Specific | AI-Friendly | Batch Ops |
|------|-----------|---------------|-------------|-----------|
| `sed` | ❌ | ❌ | ⚠️ | ✅ |
| `rust-analyzer` | ✅ | ✅ | ❌ | ❌ |
| `syn` + custom | ✅ | ✅ | ⚠️ | ⚠️ |
| **rust-ast-edit** | ✅ | ✅ | ✅ | ✅ |

## Development

```bash
# Build
cargo build --release

# Test with example
cargo run -- add-struct-field \
  --path examples/sample.rs \
  --struct-name Person \
  --field "age: u32" \
  --apply

# Run tests
cargo test
```

## Contributing

PRs welcome! Priority areas:

1. **Match arm insertion** (complex: needs expression traversal)
2. **Impl method addition**
3. **Use statement management**
4. **Better error messages**
5. **More test coverage**

## License

MIT or Apache-2.0 (your choice)

## Credits

Built for AI agents to stop using sed on Rust code. 🦀
