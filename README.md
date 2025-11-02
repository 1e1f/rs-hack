# rs-hack

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

### From crates.io (after publishing)

```bash
cargo install rs-hack
```

### From source

```bash
git clone https://github.com/1e1f/rs-hack
cd rs-hack
cargo install --path .
```

The binary will be installed to `~/.cargo/bin/rs-hack`.

## Quick Start

```bash
# Add derive macros with glob pattern
rs-hack add-derive --path "src/**/*.rs" \
  --target-type struct --name User \
  --derives "Clone,Debug,Serialize" --apply

# Auto-detect and add missing match arms
rs-hack add-match-arm --path src/handler.rs \
  --pattern "Status" \
  --auto-detect \
  --body "todo!()" \
  --enum-name Status \
  --function handle_status \
  --apply

# Add a method to an impl block
rs-hack add-impl-method --path src/user.rs \
  --target User \
  --method 'pub fn get_id(&self) -> u64 { self.id }' --apply

# Add a use statement
rs-hack add-use --path src/lib.rs \
  --use-path "serde::Serialize" --apply
```

## Supported Operations (20 commands)

### Struct Operations (4)
- ✅ **add-struct-field**: Add fields to definitions (with optional `--literal-default`)
- ✅ **add-struct-literal-field**: Add fields to literal expressions only
- ✅ **update-struct-field**: Update field types/visibility
- ✅ **remove-struct-field**: Remove fields

### Enum Operations (3)
- ✅ **add-enum-variant**, **update-enum-variant**, **remove-enum-variant**

### Match Operations (3)
- ✅ **add-match-arm** (with `--auto-detect` for missing variants)
- ✅ **update-match-arm**, **remove-match-arm**

### Code Organization (3)
- ✅ **add-derive**: Derive macros (with `--where` filter support)
- ✅ **add-impl-method**: Methods to impl blocks
- ✅ **add-use**: Use statements

### Inspection & Search (2)
- ✅ **find**: Locate AST node definitions (structs, enums, functions)
- ✅ **inspect**: List and view AST nodes (struct literals, etc.) with glob support

### State & Utilities (5)
- ✅ **history**: View past operations
- ✅ **revert**: Undo specific changes
- ✅ **clean**: Remove old state
- ✅ **batch**: Run multiple operations from JSON
- ✅ `--format diff`: Generate git-compatible patches

### Pattern-Based Filtering (NEW!)
- ✅ **`--where`**: Filter targets by traits or attributes
  - Supported on: `add-struct-field`, `update-struct-field`, `remove-struct-field`, `add-enum-variant`, `update-enum-variant`, `remove-enum-variant`, `add-derive`
  - Example: `--where "derives_trait:Clone"` or `--where "derives_trait:Clone,Debug"`

## Usage

### Glob Pattern Support

All commands now support glob patterns for targeting multiple files:

```bash
# Add derives to all structs in src directory
rs-hack add-derive --path "src/**/*.rs" \
  --target-type struct --name User \
  --derives "Clone,Debug" --apply

# Add match arms across multiple handler files
rs-hack add-match-arm --path "src/handlers/*.rs" \
  --auto-detect \
  --enum-name Status \
  --body "todo!()" \
  --apply

# Common glob patterns:
# src/**/*.rs        - All .rs files in src and subdirectories
# src/models/*.rs    - All .rs files in src/models
# src/**/handler.rs  - All handler.rs files anywhere under src
```

**Benefits**:
- Perform bulk operations across your codebase
- Target specific directories or file patterns
- Ideal for migrations and refactoring tasks

### Pattern-Based Filtering with `--where`

Filter which structs/enums to modify based on their traits or attributes:

```bash
# Add field only to structs that derive Clone
rs-hack add-struct-field \
  --path "src/**/*.rs" \
  --struct-name Config \
  --field "version: u32" \
  --where "derives_trait:Clone" \
  --apply

# Add Serialize to all types that already derive Clone OR Debug
rs-hack add-derive \
  --path "src/**/*.rs" \
  --target-type struct \
  --name User \
  --derives "Serialize" \
  --where "derives_trait:Clone,Debug" \
  --apply

# Update field only in Debug-enabled structs
rs-hack update-struct-field \
  --path "src/**/*.rs" \
  --struct-name Config \
  --field "port: u32" \
  --where "derives_trait:Debug" \
  --apply

# Remove variant only from enums with Clone
rs-hack remove-enum-variant \
  --path "src/**/*.rs" \
  --enum-name Status \
  --variant-name Deprecated \
  --where "derives_trait:Clone" \
  --apply
```

**Filter Syntax:**
- `derives_trait:Clone` - Matches if type derives Clone
- `derives_trait:Clone,Debug` - Matches if type derives Clone OR Debug (OR logic)

**Supported Operations:**
- All struct operations: `add-struct-field`, `update-struct-field`, `remove-struct-field`
- All enum operations: `add-enum-variant`, `update-enum-variant`, `remove-enum-variant`
- Derive operations: `add-derive`

**Benefits:**
- **Selective refactoring**: Only modify types that meet specific criteria
- **Safe migrations**: Add fields only to serializable types, etc.
- **Powerful combinations**: Combine with glob patterns for precise bulk operations

### Struct Operations

#### Add Field to Struct Definition
```bash
# Add field to struct definition only (idempotent - skips if exists)
rs-hack add-struct-field \
  --path src/models.rs \
  --struct-name User \
  --field "email: String" \
  --apply

# With position control
rs-hack add-struct-field \
  --path src/models.rs \
  --struct-name Config \
  --field "timeout_ms: u64" \
  --position "after:port" \
  --apply

# NEW: Add field to BOTH struct definition AND all struct literals in one command!
rs-hack add-struct-field \
  --path "src/**/*.rs" \
  --struct-name IRCtx \
  --field "return_type: Option<Type>" \
  --position "after:current_function_frame" \
  --literal-default "None" \
  --apply

# This updates BOTH:
# 1. The struct definition:
#    pub struct IRCtx {
#        ...
#        current_function_frame: Option<Frame>,
#        return_type: Option<Type>,  ← Added
#    }
#
# 2. All struct initialization expressions:
#    IRCtx {
#        ...
#        current_function_frame: None,
#        return_type: None,  ← Added
#    }
```

**Note**: The `--literal-default` flag is optional. When omitted, only the struct definition is updated (original behavior). When provided, it also updates all struct literals with the given default value.

#### Add Field to Struct Literal Expressions Only
```bash
# Add field to ALL struct initialization expressions (idempotent)
# Use this when the field already exists in the struct definition
rs-hack add-struct-literal-field \
  --path "src/**/*.rs" \
  --struct-name IRCtx \
  --field "return_type: None" \
  --position "after:current_function_frame" \
  --apply

# This modifies initialization expressions like:
# IRCtx { stack: vec![], current_function_frame: None, ... }
#
# NOT the struct definition:
# pub struct IRCtx { ... }
```

#### Update Field
```bash
# Change field visibility
rs-hack update-struct-field \
  --path src/models.rs \
  --struct-name User \
  --field "pub email: String" \
  --apply

# Change field type
rs-hack update-struct-field \
  --path src/models.rs \
  --struct-name User \
  --field "id: i64" \
  --apply
```

#### Remove Field
```bash
rs-hack remove-struct-field \
  --path src/models.rs \
  --struct-name User \
  --field-name deprecated_field \
  --apply
```

### Enum Operations

#### Add Variant
```bash
# Add simple variant (idempotent)
rs-hack add-enum-variant \
  --path src/types.rs \
  --enum-name Status \
  --variant "Pending" \
  --apply

# Add variant with data
rs-hack add-enum-variant \
  --path src/types.rs \
  --enum-name Message \
  --variant "Error { code: i32, msg: String }" \
  --apply
```

#### Update Variant
```bash
rs-hack update-enum-variant \
  --path src/types.rs \
  --enum-name Status \
  --variant "Draft { created_at: u64 }" \
  --apply
```

#### Remove Variant
```bash
rs-hack remove-enum-variant \
  --path src/types.rs \
  --enum-name Status \
  --variant-name Deprecated \
  --apply
```

### Match Arm Operations

#### Add Match Arm
```bash
# Add match arm (idempotent)
rs-hack add-match-arm \
  --path src/handler.rs \
  --pattern "Status::Archived" \
  --body '"archived".to_string()' \
  --function handle_status \
  --apply

# Auto-detect missing variants and add them all
rs-hack add-match-arm \
  --path src/handler.rs \
  --auto-detect \
  --enum-name Status \
  --body "todo!()" \
  --function handle_status \
  --apply
```

**Auto-Detect Feature**: The `--auto-detect` flag analyzes your enum definition and match expressions to automatically add match arms for ALL missing variants. This is perfect for:
- Ensuring exhaustive match coverage after adding new enum variants
- Quickly scaffolding match expressions with placeholder implementations
- Maintaining consistency across multiple match sites

#### Update Match Arm
```bash
rs-hack update-match-arm \
  --path src/handler.rs \
  --pattern "Status::Draft" \
  --body '"pending".to_string()' \
  --function handle_status \
  --apply
```

#### Remove Match Arm
```bash
rs-hack remove-match-arm \
  --path src/handler.rs \
  --pattern "Status::Deleted" \
  --function handle_status \
  --apply
```

**Note:** Match operations automatically format the modified function using `prettyplease` to ensure consistent, readable code.

### Derive Macros

```bash
# Add derive macros (idempotent)
rs-hack add-derive \
  --path src/models.rs \
  --target-type struct \
  --name User \
  --derives "Clone,Debug,Serialize" \
  --apply

# Works with enums too
rs-hack add-derive \
  --path src/types.rs \
  --target-type enum \
  --name Status \
  --derives "Copy,Clone" \
  --apply
```

### Impl Methods

```bash
# Add method to impl block
rs-hack add-impl-method \
  --path src/user.rs \
  --target User \
  --method 'pub fn get_id(&self) -> u64 { self.id }' \
  --apply

# With position control
rs-hack add-impl-method \
  --path src/user.rs \
  --target User \
  --method 'pub fn get_name(&self) -> &str { &self.name }' \
  --position "after:get_id" \
  --apply
```

### Use Statements

```bash
# Add use statement (idempotent)
rs-hack add-use \
  --path src/lib.rs \
  --use-path "serde::Serialize" \
  --apply

# Position control
rs-hack add-use \
  --path src/lib.rs \
  --use-path "std::fmt::Display" \
  --position "after:collections" \
  --apply
```

### Find AST Nodes

Locate definitions (structs, enums, functions) in a single file:

```bash
# Find struct definition location
rs-hack find \
  --path src/models.rs \
  --node-type struct \
  --name User

# Find enum definition
rs-hack find \
  --path src/types.rs \
  --node-type enum \
  --name Status

# Output (JSON):
# [{
#   "line": 10,
#   "column": 0,
#   "end_line": 15,
#   "end_column": 1
# }]
```

### Inspect AST Nodes

List and view AST nodes (struct literals, etc.) across multiple files with glob support:

```bash
# List all Shadow struct initializations
rs-hack inspect \
  --path "tests/shadow_*.rs" \
  --node-type struct-literal \
  --name Shadow \
  --format snippets

# Output:
# // tests/shadow_bold.rs:42:18 - Shadow
# Shadow { offset: Vec2::new(2.0, 2.0), blur: 4.0, color: Color32::BLACK, }
#
# // tests/shadow_test.rs:15:20 - Shadow
# Shadow { offset: Vec2::ZERO, blur: 0.0, color: Color32::WHITE, }

# Get locations only (like grep -n but AST-aware)
rs-hack inspect \
  --path "src/**/*.rs" \
  --node-type struct-literal \
  --name Config \
  --format locations

# Output:
# src/app.rs:25:18
# src/config.rs:45:12
# src/main.rs:10:21

# Get structured JSON output
rs-hack inspect \
  --path "src/**/*.rs" \
  --node-type struct-literal \
  --name User \
  --format json

# List ALL struct literals (no name filter)
rs-hack inspect \
  --path "src/models.rs" \
  --node-type struct-literal \
  --format snippets

# Find match arms for specific enum variant (better than grep!)
rs-hack inspect \
  --path "src/**/*.rs" \
  --node-type match-arm \
  --name "Operator::AssertSome" \
  --format snippets

# Output:
# // src/format/print.rs:766:12 - Operator::AssertSome
# Operator::AssertSome => write!(f, "!_"),
#
# // src/eval.rs:45:12 - Operator::AssertSome
# Operator::AssertSome => self.unwrap_or_panic(value),

# Find ALL match arms in a file
rs-hack inspect \
  --path "src/handler.rs" \
  --node-type match-arm \
  --format locations

# Find enum variant usages (better than grep!)
rs-hack inspect \
  --path "src/**/*.rs" \
  --node-type enum-usage \
  --name "Operator::PropagateError" \
  --format snippets

# Output:
# // src/format/print.rs:763:12 - Operator::PropagateError
# Operator::PropagateError
#
# // src/eval.rs:120:25 - Operator::PropagateError
# Operator::PropagateError

# Find ALL usages of any Operator variant
rs-hack inspect \
  --path "src/**/*.rs" \
  --node-type enum-usage \
  --name "Operator::" \
  --format locations | wc -l

# Find all calls to a specific function
rs-hack inspect \
  --path "src/**/*.rs" \
  --node-type function-call \
  --name "handle_error" \
  --format snippets

# Output:
# // src/error.rs:42:4 - handle_error
# handle_error()
#
# // src/parser.rs:156:8 - handle_error
# handle_error(err)

# Find all .unwrap() calls (great for auditing!)
rs-hack inspect \
  --path "src/**/*.rs" \
  --node-type method-call \
  --name "unwrap" \
  --format locations

# Find all references to a variable/identifier
rs-hack inspect \
  --path "src/**/*.rs" \
  --node-type identifier \
  --name "config" \
  --format snippets

# Find all usages of a type
rs-hack inspect \
  --path "src/**/*.rs" \
  --node-type type-ref \
  --name "Vec" \
  --format snippets

# Output:
# // src/lib.rs:15:18 - Vec<String>
# Vec<String>
#
# // src/lib.rs:42:11 - Vec<i32>
# Vec<i32>
```

**Supported Node Types:**
- `struct-literal` - Struct initialization expressions
- `match-arm` - Match expression arms
- `enum-usage` - Enum variant references/usages anywhere in code
- `function-call` - Function invocations
- `method-call` - Method calls
- `identifier` - Any identifier reference
- `type-ref` - Type usages

**Output Formats:**
- `snippets` (default): Shows file location + formatted code on single line
- `locations`: File:line:column format (great for piping to other tools)
- `json`: Structured data with full location info and code snippets

**Use Cases:**
- **Better than grep**: Find code without false positives from comments/strings
- **Multi-file search**: Use glob patterns to search across many files
- **Extract code chunks**: Get full struct literal/match arm/path content, not just the first line
- **Prepare for refactoring**: Inspect before bulk modifications
- **Find enum usage**: Locate all places where a specific enum variant is used (matches, returns, comparisons, etc.)
- **Track variant usage**: See everywhere `Status::Active` or `Operator::Error` appears in your codebase
- **Audit function calls**: Find all calls to specific functions (e.g., `handle_error`, `format_operator`)
- **Audit method calls**: Find all `.unwrap()`, `.clone()`, or `.to_string()` calls
- **Track identifiers**: Find all references to variables, constants, or parameters
- **Type usage analysis**: See where types like `Vec`, `Option`, or `Result` are used

### Batch Operations

Create a JSON file with multiple operations:

```json
{
  "base_path": "src/",
  "operations": [
    {
      "type": "AddDerive",
      "target_name": "User",
      "target_type": "struct",
      "derives": ["Clone", "Debug"]
    },
    {
      "type": "AddStructField",
      "struct_name": "User",
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
rs-hack batch --spec migrations.json --apply
```

## Diff Output

Generate git-compatible patches for review before applying:

```bash
# Generate diff for review
rs-hack add-struct-field \
  --path src/user.rs \
  --struct-name User \
  --field "age: u32" \
  --format diff

# Output:
# --- src/user.rs
# +++ src/user.rs
# @@ -1,5 +1,6 @@
#  pub struct User {
#      id: u64,
# +    age: u32,
#      name: String,
#  }

# Save to patch file
rs-hack add-struct-field ... --format diff > changes.patch

# Apply with git
git apply changes.patch

# Or apply AND show diff
rs-hack add-struct-field ... --format diff --apply
```

Perfect for AI-generated changes that need human review!

## State Storage and Revert System

rs-hack includes a powerful state tracking and revert system that allows you to safely experiment with changes and undo them if needed. This is especially useful for AI agents that want to try different approaches.

### How It Works

Every time you run a command with `--apply`, rs-hack:
1. Generates a unique run ID (7 characters, like git)
2. Backs up **only the AST nodes being modified** (not entire files)
3. Computes checksums for integrity verification
4. Stores operation metadata for auditing

### Commands

#### View History
```bash
# Show last 10 runs
rs-hack history

# Show last 50 runs
rs-hack history --limit 50

# Example output:
# Recent runs (showing up to 10):
#
# a05a626  2025-11-01 18:45  AddStructField        1 file      [can revert]
# def456a  2025-11-01 09:15  add-derive            1 file      [can revert]
# ghi789b  2025-10-31 16:45  add-match-arm         2 files     [reverted]
```

#### Revert Changes
```bash
# Revert a specific run
rs-hack revert a05a626

# Force revert even if files have changed since
rs-hack revert a05a626 --force
```

#### Clean Old State
```bash
# Clean runs older than 30 days (default)
rs-hack clean

# Keep only last 7 days
rs-hack clean --keep-days 7
```

### State Directory

rs-hack stores state in different locations based on your needs:

**Priority order:**
1. **Custom directory** (via `RS_HACK_STATE_DIR` environment variable) - highest priority
2. **Local state** (via `--local-state` flag) - uses `./.rs-hack` in current directory
3. **Global default** - uses system data directory (`~/.rs-hack` on Unix-like systems)

#### Using Environment Variable (Recommended for Testing)

```bash
# Set custom state directory
export RS_HACK_STATE_DIR=/tmp/my-test-state
rs-hack add-struct-field --path src/lib.rs --struct-name User --field "age: u32" --apply

# View history from custom state
RS_HACK_STATE_DIR=/tmp/my-test-state rs-hack history

# Revert using custom state
RS_HACK_STATE_DIR=/tmp/my-test-state rs-hack revert a05a626

# Perfect for CI/CD or isolated testing
RS_HACK_STATE_DIR=/path/to/ci/state rs-hack batch --spec migrations.json --apply
```

**Note:** The environment variable takes precedence over `--local-state`, allowing you to override state location for testing without changing commands.

#### Using Local State Flag

```bash
# Use ./.rs-hack directory for state storage
rs-hack --local-state add-struct-field --path src/lib.rs --struct-name User --field "age: u32" --apply

# View history from local state
rs-hack --local-state history

# Revert using local state
rs-hack --local-state revert a05a626
```

#### Using Global State (Default)

```bash
# No flag needed - uses ~/.rs-hack by default
rs-hack add-struct-field --path src/lib.rs --struct-name User --field "age: u32" --apply
rs-hack history
rs-hack revert a05a626
```

### Safety Features

1. **Hash Verification**: Ensures files haven't changed before reverting (unless `--force`)
2. **Atomic Operations**: Uses temp files and atomic renames
3. **AST Node Backups**: Stores only modified nodes (85-95% space savings)
4. **Auto-Cleanup**: Removes old backups with `clean` command
5. **Idempotent**: Safe to run operations multiple times

### AI Agent Workflow Example

```bash
# AI tries adding a field
rs-hack add-struct-field --path src/user.rs --struct-name User --field "email: String" --apply
# Output: Run ID: a05a626

# AI runs tests - they fail!
cargo test  # FAIL

# AI reverts the change
rs-hack revert a05a626
# Output: ✓ Run a05a626 reverted successfully

# AI tries a different approach
rs-hack add-struct-field --path src/user.rs --struct-name User --field "email: Option<String>" --apply
# Output: Run ID: b12c789

# Tests pass!
cargo test  # PASS
```

### Use Cases

- **Experimentation**: Try changes and easily revert if they don't work
- **Multi-step Migrations**: Revert to any checkpoint if something breaks
- **Debugging**: Understand what changed when tests start failing
- **Safety Net**: Confidence to let AI agents make changes automatically

### Storage Format

```
~/.rs-hack/
  runs.json              # Index of all runs
  a05a626.json           # Metadata for run a05a626
  a05a626/               # Backup directory
    node_0.json          # Modified struct (AST node only)
    node_1.json          # Modified enum (AST node only)
```

**Note**: Only modified AST nodes are backed up (not entire files), resulting in 85-95% space savings.

## AI Agent Integration

### Example: Claude Code using rs-hack

```bash
# Claude reads the task
"Add Clone and Debug derives to all structs, then add get_id methods"

# Claude executes
rs-hack add-derive \
  --path src/models/ \
  --target-type struct \
  --name User \
  --derives "Clone,Debug" \
  --apply

rs-hack add-impl-method \
  --path src/models/ \
  --target User \
  --method 'pub fn get_id(&self) -> u64 { self.id }' \
  --apply
```

## Architecture

### Core Components

1. **Parser** (`syn` crate): Parses Rust → AST
2. **Editor**: Manipulates AST and tracks byte positions
3. **Operations**: Type-safe operation definitions
4. **CLI**: User-friendly interface

### Key Design Decisions

- **Preserves formatting**: Uses `prettyplease` for clean output
- **Idempotent**: Running twice doesn't duplicate changes
- **Fail-fast**: Returns errors clearly, doesn't corrupt code
- **Dry-run default**: Must explicitly `--apply` to modify files

## Real-World Example: rs-hack vs perl/sed

### The Problem: Perl Commands Are Dangerously Ambiguous

Consider this perl command that was used to add a field to struct initialization:

```bash
perl -i -pe 's/current_function_frame: (.*?),\s*$/current_function_frame: $1,\n current_function_return_type: None,\n/' \
  src/compiler/intermediate/types/ctx.rs ...
```

**This command is DANGEROUSLY AMBIGUOUS** because it matches text patterns without understanding Rust syntax:

#### What It Could Match (All Have the Same Text Pattern!)

```rust
// ❌ Struct DEFINITION - probably NOT what you want
pub struct IRCtx {
    stack: Vec<Frame>,
    current_function_frame: Option<Frame>,  // ← Matches! Adds field here
    local_types: HashMap<String, Type>,
}

// ✅ Struct LITERAL - what you actually want
let ctx = IRCtx {
    stack: vec![],
    current_function_frame: None,  // ← Matches! Adds field here
    local_types: HashMap::new(),
};

// ❌ COMMENT - corrupts your code!
// Example: current_function_frame: None,  // ← Matches! Corrupts comment

// ❌ STRING - corrupts your string literal!
let s = "current_function_frame: None,";  // ← Matches! Corrupts string
```

**The perl command can't distinguish between these!** It will modify ALL of them, likely corrupting your code.

### ✅ The Explicit, Safe Way (rs-hack)

rs-hack provides **separate, explicit operations** for each use case. You can update both in one command or separately:

#### Option 1: Update BOTH Definition and Literals (One Command!)
```bash
# NEW: Do BOTH in one command with --literal-default
rs-hack add-struct-field \
  --path "src/**/*.rs" \
  --struct-name IRCtx \
  --field "return_type: Option<Type>" \
  --position "after:current_function_frame" \
  --literal-default "None" \
  --apply
```

This modifies BOTH the struct definition AND all struct literals:
```rust
// ✅ Struct definition updated
pub struct IRCtx {
    stack: Vec<Frame>,
    current_function_frame: Option<Frame>,
    return_type: Option<Type>,  // ✅ Added here
    local_types: HashMap<String, Type>,
}

// ✅ All struct literals updated
let ctx = IRCtx {
    stack: vec![],
    current_function_frame: None,
    return_type: None,  // ✅ Added here
    local_types: HashMap::new(),
};
```

#### Option 2: Separate Operations (When You Need More Control)

**Step 1: Modify Struct Definitions Only**
```bash
rs-hack add-struct-field \
  --path "src/**/*.rs" \
  --struct-name IRCtx \
  --field "return_type: Option<Type>" \
  --position "after:current_function_frame" \
  --apply
```

**Step 2: Modify Struct Literal Expressions Only**
```bash
rs-hack add-struct-literal-field \
  --path "src/**/*.rs" \
  --struct-name IRCtx \
  --field "return_type: None" \
  --position "after:current_function_frame" \
  --apply
```

**Benefits of Explicit Operations:**
- ✅ **Explicit Intent**: Command name tells you exactly what will be modified
- ✅ **AST-Aware**: Only modifies actual Rust syntax nodes
- ✅ **Never Corrupts**: Won't touch comments, strings, or unrelated code
- ✅ **Idempotent**: Safe to run multiple times
- ✅ **Position Control**: Precise placement of new fields
- ✅ **Glob Patterns**: No manual file listing
- ✅ **Format-Independent**: Works regardless of whitespace/formatting
- ✅ **Dry-Run Default**: Preview changes before applying

**vs Perl/Sed Problems:**
- ❌ **Ambiguous**: Can't distinguish struct definitions from literals from comments
- ❌ **Text-Based**: Breaks on formatting changes
- ❌ **Not Idempotent**: Running twice duplicates fields
- ❌ **No Validation**: Can corrupt code on partial matches
- ❌ **Manual Files**: Need to list every file explicitly
- ❌ **No Preview**: Modifies files immediately

### What rs-hack Does Behind the Scenes

1. **Parse** each file into an AST using `syn`
2. **Traverse** the AST to find struct definitions OR struct literal expressions (depending on operation)
3. **Validate** the target exists and check if the field already exists (idempotent)
4. **Modify** the AST by inserting the new field in the correct position
5. **Format** the result with `prettyplease`
6. **Write** back atomically

Safe, semantic, and correct every time. 🦀

## Comparison with Alternatives

| Tool | AST-Aware | Rust-Specific | AI-Friendly | Batch Ops | Idempotent |
|------|-----------|---------------|-------------|-----------|------------|
| `sed` | ❌ | ❌ | ⚠️ | ✅ | ❌ |
| `rust-analyzer` | ✅ | ✅ | ❌ | ❌ | ⚠️ |
| `syn` + custom | ✅ | ✅ | ⚠️ | ⚠️ | ⚠️ |
| **rs-hack** | ✅ | ✅ | ✅ | ✅ | ✅ |

## Development

```bash
# Build
cargo build --release

# Run tests
cargo test
./tests/integration_test.sh

# Install locally
cargo install --path .
```

## Testing

```bash
# Unit tests
cargo test

# Integration tests
./tests/integration_test.sh

# Test individual operations
rs-hack add-struct-field \
  --path examples/sample.rs \
  --struct-name User \
  --field "age: u32" \
  --apply
```

## Publishing

See [PUBLISHING_GUIDE.md](PUBLISHING_GUIDE.md) for instructions on publishing to crates.io.

## Features by Version

### v0.4.0 - Pattern-Based Filtering & Inspection (Current)
- **`--where` filter**: Pattern-based filtering for selective refactoring
  - `--where "derives_trait:Clone"` - Filter by derived traits
  - OR logic support: `--where "derives_trait:Clone,Debug"`
  - Works on all struct/enum operations + `add-derive`
- **`inspect` command**: AST-aware search and inspection
  - List struct literals, match arms, and enum variant usages across files
  - Find all match arms handling a specific enum variant
  - Find all places where an enum variant is referenced (complete grep replacement!)
  - Three output formats: `snippets`, `locations`, `json`
  - Glob pattern support for multi-file inspection
  - Better than grep: no false positives, extracts full code chunks
- **Enhanced `find`**: Improved documentation for locating definitions

### v0.3.0 - State Storage, Revert & Diff Output
- **State tracking**: Every operation recorded with unique run ID
- **Revert system**: Undo changes with `rs-hack revert <run-id>`
- **Diff output**: Generate git-compatible patches with `--format diff`
- **AST node backups**: Stores only modified nodes (85-95% space savings)
- **Configurable state**: Use `RS_HACK_STATE_DIR` environment variable
- **Commands**: `history`, `revert`, `clean`

### v0.2.0 - Glob Patterns & Auto-Detect
- **Glob patterns**: Target multiple files with `"src/**/*.rs"`
- **Auto-detect match arms**: Find and add all missing enum variants
- **Literal-default**: Update struct definitions AND literals together

## Contributing

PRs welcome! Future ideas:
- Support for generics in impl blocks
- Attribute macro operations
- Function signature modification
- Type alias operations

## License

MIT OR Apache-2.0

## Credits

Built for AI agents to stop using sed on Rust code. 🦀

Created by Leif Shackelford ([@1e1f](https://github.com/1e1f))
