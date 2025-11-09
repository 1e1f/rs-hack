# rs-hack

Stop using sed on Rust code 🦀

AST-aware refactoring tool for AI agents or other automated pipelines.

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

## Documentation

Three levels of documentation for different needs:

📖 **[HUMAN.md](HUMAN.md)** - Quick command reference
- Fast syntax lookup for humans
- One-liner examples for every command
- Perfect when you know what you want, just need the flags

🤖 **[templates/claude-skills/rs-hack.md](templates/claude-skills/rs-hack.md)** - Claude Code skill
- Complete workflows with best practices
- Teaches Claude Code how to use rs-hack effectively
- Copy to your project's `.claude/skills/` directory

📚 **[README.md](README.md)** - You are here
- Complete documentation with examples
- Architecture and design decisions
- Integration guides

## Installation

### From crates.io

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
# Comment out all eprintln! macros containing "[SHADOW RENDER]"
rs-hack transform \
  --path "src/**/*.rs" \
  --node-type macro-call \
  --name eprintln \
  --content-filter "[SHADOW RENDER]" \
  --action comment \
  --apply

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

## Supported Operations (21 commands)

### Generic Transform (1) ⭐ NEW
- ✅ **transform**: Find and modify any AST nodes (comment, remove, or replace)
  - Works with all node types from `inspect`
  - Content filtering for precise targeting
  - Single command replaces many specialized operations

### Struct Operations (3) ⭐ CONSOLIDATED
- ✅ **add-struct-field**: Add fields to definitions, literals, or both
  - (default) → definition only
  - `--literal-default VALUE` → definition + literals
  - `--literal-only` → literals only (replaces old `add-struct-literal-field`)
- ✅ **update-struct-field**: Update field types/visibility
- ✅ **remove-struct-field**: Remove fields

### Enum Operations (4)
- ✅ **add-enum-variant**, **update-enum-variant**, **remove-enum-variant**
- ✅ **rename-enum-variant**: Rename enum variants across entire codebase ⭐ NEW

### Match Operations (3)
- ✅ **add-match-arm** (with `--auto-detect` for missing variants)
- ✅ **update-match-arm**, **remove-match-arm**

### Code Organization (3)
- ✅ **add-derive**: Derive macros (with `--where` filter support)
- ✅ **add-impl-method**: Methods to impl blocks
- ✅ **add-use**: Use statements

### Inspection & Search (2)
- ✅ **find**: Locate AST node definitions (structs, enums, functions)
- ✅ **inspect**: List and view AST nodes with glob support (now includes macro-call)

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
# Common case: field already exists in definition, just add to all literals
# Simply omit the type (:Type) and provide --literal-default
rs-hack add-struct-field \
  --path "src/**/*.rs" \
  --struct-name IRCtx \
  --field "return_type" \
  --literal-default "None" \
  --position "after:current_function_frame" \
  --apply

# This modifies initialization expressions like:
# IRCtx { stack: vec![], current_function_frame: None, return_type: None }
#
# NOT the struct definition:
# pub struct IRCtx { /* no change */ }

# How it works:
# - No ':' in --field means "literals only" (definition is skipped)
# - With ':' in --field, it tries definition (idempotent) + literals

# OLD (deprecated): add-struct-literal-field command
```

**Pattern Matching for `--struct-name`** (v0.4.0+):

The `--struct-name` parameter supports pattern matching to distinguish between struct literals and enum variant constructors:

```bash
# Match ONLY pure struct literals (no :: prefix)
--struct-name "Rectangle"
# Matches:   Rectangle { ... }
# Ignores:   View::Rectangle { ... }
# Ignores:   ViewType::Rectangle { ... }

# Match ANY path ending with Rectangle (wildcard)
--struct-name "*::Rectangle"
# Matches:   View::Rectangle { ... }
# Matches:   ViewType::Rectangle { ... }
# Matches:   Rectangle { ... }

# Match EXACT path only
--struct-name "View::Rectangle"
# Matches:   View::Rectangle { ... }
# Ignores:   ViewType::Rectangle { ... }
# Ignores:   Rectangle { ... }
```

**Why This Matters:**

Without explicit patterns, `View::Rectangle { ... }` is an **enum variant constructor**, not a struct literal:

```rust
// Enum definition
enum View {
    Rectangle { width: f32, height: f32 },  // ← Enum variant with named fields
    Circle { radius: f32 },
}

// Struct definition
struct Rectangle {
    width: f32,
    height: f32,
}

// Usage:
let view = View::Rectangle { width: 100.0, height: 50.0 };  // ← Enum variant constructor
let rect = Rectangle { width: 100.0, height: 50.0 };         // ← Struct literal
```

The pattern matching prevents accidental modification of enum variants when you meant to target struct literals.

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

#### Rename Variant ⭐ NEW

Rename an enum variant across the entire codebase in a type-safe, AST-aware manner:

```bash
# Rename across all files in src directory
rs-hack rename-enum-variant \
  --paths "src/**/*.rs" \
  --enum-name IRValue \
  --old-variant HashMapV2 \
  --new-variant HashMap \
  --apply

# Dry-run with diff preview
rs-hack rename-enum-variant \
  --paths "src/**/*.rs" \
  --enum-name Status \
  --old-variant Draft \
  --new-variant Pending \
  --format diff

# Summary format (shows only changed lines) ⭐ NEW Sprint 2
rs-hack rename-enum-variant \
  --paths "src/**/*.rs" \
  --enum-name Status \
  --old-variant Draft \
  --new-variant Pending \
  --format summary

# Validate mode: check for remaining references ⭐ NEW Sprint 2
rs-hack rename-enum-variant \
  --paths "src/**/*.rs" \
  --enum-name Status \
  --old-variant Draft \
  --new-variant Pending \
  --validate

# Batch rename multiple variants (using batch command)
cat <<EOF | rs-hack batch --apply
{
  "base_path": "src/",
  "operations": [
    {
      "type": "RenameEnumVariant",
      "enum_name": "IRValue",
      "old_variant": "HashMapV2",
      "new_variant": "HashMap"
    },
    {
      "type": "RenameEnumVariant",
      "enum_name": "IRValue",
      "old_variant": "ListV2",
      "new_variant": "List"
    }
  ]
}
EOF
```

**What it renames:**
- ✅ Enum variant definitions (`pub enum IRValue { HashMapV2(Frame) }`)
- ✅ Match arm patterns (`IRValue::HashMapV2(frame) => { ... }`)
- ✅ Constructor calls (`let val = IRValue::HashMapV2(data)`)
- ✅ Reference patterns (`&IRValue::HashMapV2(_) => { ... }`)
- ✅ Struct patterns (`Some(IRValue::HashMapV2(f)) => { ... }`)

**Benefits over sed/awk:**
- ✅ **Type-safe**: Only renames actual enum variants, not strings/comments
- ✅ **Complete**: Finds all usages across patterns, expressions, and types
- ✅ **Fast**: Processes entire codebase in seconds
- ✅ **Safe**: Dry-run mode with diff preview
- ✅ **Reversible**: Tracked in history for easy revert

**New in Sprint 2:**
- ⭐ **Validation mode** (`--validate`): Check for remaining references after a rename
  - Helps identify patterns that weren't caught (e.g., fully qualified paths)
  - Provides suggestions for fixing missed references
  - Perfect for verifying completeness of large refactors
- ⭐ **Summary format** (`--format summary`): Show only changed lines
  - Cleaner output than full diffs for large files
  - Focuses on what actually changed
  - Easier to review and verify changes

**Real-world example:**

The original motivation for this command was renaming `IRValue::HashMapV2` → `IRValue::HashMap` across 23 files in the noisetable/koda codebase. What would have been a 4-6 hour manual refactor became a 30-second operation.

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

# Find all eprintln! macros (NEW!)
rs-hack inspect \
  --path "src/**/*.rs" \
  --node-type macro-call \
  --name "eprintln" \
  --format snippets

# Find eprintln! macros with specific content (NEW!)
rs-hack inspect \
  --path "src/**/*.rs" \
  --node-type macro-call \
  --name "eprintln" \
  --content-filter "[SHADOW RENDER]" \
  --format locations
```

**Supported Node Types:**
- `struct-literal` - Struct initialization expressions
- `match-arm` - Match expression arms
- `enum-usage` - Enum variant references/usages anywhere in code
- `function-call` - Function invocations
- `method-call` - Method calls
- `macro-call` - Macro invocations (e.g., `println!`, `eprintln!`, `todo!`) ⭐ NEW
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

### Transform - Generic Find and Modify ⭐ NEW

The `transform` command provides a generic way to find and modify any AST nodes. It combines the power of `inspect` with mutation capabilities, keeping the API surface small while offering maximum flexibility.

**Perfect for AI agents**: One command to learn instead of dozens of specialized operations.

#### Basic Usage

```bash
# Comment out all eprintln! macros containing "[SHADOW RENDER]"
rs-hack transform \
  --path "src/**/*.rs" \
  --node-type macro-call \
  --name eprintln \
  --content-filter "[SHADOW RENDER]" \
  --action comment \
  --apply

# Remove all .unwrap() calls in renderer code
rs-hack transform \
  --path "src/renderer/**/*.rs" \
  --node-type method-call \
  --name unwrap \
  --action remove \
  --apply

# Comment out all todo!() macros
rs-hack transform \
  --path "src/**/*.rs" \
  --node-type macro-call \
  --name todo \
  --action comment \
  --apply

# Replace specific function calls
rs-hack transform \
  --path "src/handlers/*.rs" \
  --node-type function-call \
  --name old_handler \
  --action replace \
  --with "new_handler" \
  --apply
```

#### Supported Actions

- **`comment`**: Wraps matched nodes in `// ...` comments
- **`remove`**: Deletes matched nodes entirely
- **`replace`**: Replaces with provided code (via `--with` flag)

#### Supported Node Types

Works with all node types from `inspect`:
- `macro-call` - Macro invocations (e.g., `println!`, `eprintln!`, `todo!`)
- `method-call` - Method calls (e.g., `.unwrap()`, `.clone()`)
- `function-call` - Function invocations
- `enum-usage` - Enum variant references
- `struct-literal` - Struct initialization expressions
- `match-arm` - Match expression arms
- `identifier` - Any identifier reference
- `type-ref` - Type usages

#### Filtering Options

**Name Filter** (`--name`): Filter by the name of the node
```bash
# Only eprintln! macros, not println!
--name eprintln
```

**Content Filter** (`--content-filter`): Filter by source code content
```bash
# Only macros containing specific text
--content-filter "[SHADOW RENDER]"
```

**Combined Filters**: Use both for precise targeting
```bash
rs-hack transform \
  --path "src/**/*.rs" \
  --node-type macro-call \
  --name eprintln \
  --content-filter "[DEBUG]" \
  --action comment \
  --apply
```

#### Real-World Examples

**Clean up debug logs:**
```bash
# Comment out all debug eprintln! statements
rs-hack transform \
  --path "src/**/*.rs" \
  --node-type macro-call \
  --name eprintln \
  --content-filter "[DEBUG]" \
  --action comment \
  --apply
```

**Remove dangerous unwrap calls:**
```bash
# Remove all .unwrap() calls (review first!)
rs-hack transform \
  --path "src/**/*.rs" \
  --node-type method-call \
  --name unwrap \
  --action comment \
  --apply
```

**Migrate from old to new API:**
```bash
# Replace old function calls
rs-hack transform \
  --path "src/**/*.rs" \
  --node-type function-call \
  --name legacy_init \
  --action replace \
  --with "modern_init" \
  --apply
```

**Workflow**: Use `inspect` first to see what matches, then `transform` to modify:
```bash
# 1. See what will be affected
rs-hack inspect \
  --path "src/**/*.rs" \
  --node-type macro-call \
  --name eprintln \
  --content-filter "[SHADOW RENDER]"

# 2. Apply transformation (dry-run first!)
rs-hack transform \
  --path "src/**/*.rs" \
  --node-type macro-call \
  --name eprintln \
  --content-filter "[SHADOW RENDER]" \
  --action comment

# 3. Apply for real
rs-hack transform \
  --path "src/**/*.rs" \
  --node-type macro-call \
  --name eprintln \
  --content-filter "[SHADOW RENDER]" \
  --action comment \
  --apply
```

**Why Transform is Better than Specialized Commands:**
- ✅ Single command for AI agents to learn
- ✅ Works with any AST node type
- ✅ Content filtering for precise targeting
- ✅ Composable with all inspect node types
- ✅ Fewer commands = smaller API surface

### Batch Operations

Create a JSON or YAML file with multiple operations ⭐ YAML support NEW in Sprint 3:

**JSON format:**
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

**YAML format** (easier for humans to write):
```yaml
base_path: src/
operations:
  - type: RenameEnumVariant
    enum_name: Status
    old_variant: DraftV2
    new_variant: Draft
    edit_mode: surgical

  - type: RenameEnumVariant
    enum_name: Status
    old_variant: PublishedV2
    new_variant: Published
    edit_mode: surgical

  - type: RenameFunction
    old_name: process_event_v2
    new_name: process_event
```

Run batch:

```bash
# Auto-detects format from file extension
rs-hack batch --spec migrations.yaml --apply
rs-hack batch --spec migrations.json --apply

# With exclude patterns ⭐ NEW in Sprint 3
rs-hack batch --spec migrations.yaml --exclude "**/tests/**" --exclude "**/deprecated/**" --apply
```

## Exclude Patterns ⭐ NEW in Sprint 3

Skip certain paths during operations using glob patterns:

```bash
# Exclude test fixtures and deprecated code
rs-hack rename-enum-variant \
  --paths "src/**/*.rs" \
  --enum-name Status \
  --old-variant Draft \
  --new-variant Pending \
  --exclude "**/tests/fixtures/**" \
  --exclude "**/deprecated/**" \
  --apply

# Exclude multiple patterns (use --exclude multiple times)
rs-hack add-struct-field \
  --paths "**/*.rs" \
  --struct-name User \
  --field "verified: bool" \
  --exclude "**/tests/**" \
  --exclude "**/examples/**" \
  --exclude "**/vendor/**" \
  --apply

# Works with any command that accepts --paths
rs-hack transform \
  --paths "src/**/*.rs" \
  --node-type macro-call \
  --name eprintln \
  --action comment \
  --exclude "**/production/**" \
  --apply
```

**Use cases:**
- Skip test fixtures that should remain unchanged
- Exclude deprecated code that will be removed
- Avoid vendored/third-party code
- Selective refactoring of specific modules

**Pattern matching:**
- Glob patterns: `**/tests/**`, `src/deprecated/*.rs`
- Simple strings: `deprecated`, `test` (matches anywhere in path)
- Multiple patterns: Use `--exclude` multiple times

## Documentation Comment Operations

Add, update, or remove documentation comments systematically:

```bash
# Add documentation to items
rs-hack add-doc-comment \
  --paths "src/**/*.rs" \
  --target-type struct \
  --name User \
  --doc-comment "Represents a user in the system" \
  --apply

# Update existing documentation
rs-hack update-doc-comment \
  --paths "src/**/*.rs" \
  --target-type function \
  --name process_user \
  --doc-comment "Updated documentation" \
  --apply

# Remove documentation
rs-hack remove-doc-comment \
  --paths "src/**/*.rs" \
  --target-type enum \
  --name Status \
  --apply
```

**Supported targets:** `struct`, `enum`, `function`

**Comment styles:** `line` (///) or `block` (/** */)

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

## Claude Code Integration

rs-hack works seamlessly with [Claude Code](https://claude.com/code) via the Bash tool—no additional setup required!

### Quick Setup

1. **Install rs-hack:**
   ```bash
   cargo install rs-hack
   ```

2. **(Optional) Add skill for guided usage:**

   Create `.claude/skills/rs-hack.md` in your project:
   ```bash
   mkdir -p .claude/skills
   curl -o .claude/skills/rs-hack.md \
     https://raw.githubusercontent.com/1e1f/rs-hack/main/templates/claude-skills/rs-hack.md
   ```

   Or copy from this repo's `templates/claude-skills/rs-hack.md`

### How It Works

Claude can use rs-hack directly through bash commands:

```
User: "Rename the enum variant IRValue::HashMapV2 to HashMap across all files"

Claude: I'll use rs-hack to safely rename that enum variant.

*Runs: rs-hack inspect to find usages*
*Runs: rs-hack rename-enum-variant with --format diff to preview*
*Shows you the diff*
*Runs: rs-hack rename-enum-variant --apply*
*Verifies with: cargo check*

✓ Renamed HashMapV2 → HashMap in 23 files
```

### Best Practices

**The skill teaches Claude to:**
- ✅ Always inspect before transforming
- ✅ Preview changes with `--format diff` before applying
- ✅ Use glob patterns for multi-file operations
- ✅ Verify changes with `cargo check`
- ✅ Track history and revert if needed

### Example Workflows

**Workflow 1: Large-Scale Refactoring**
```
User: "Add a return_type field to all IRCtx struct literals"

Claude:
1. Inspects struct literals: rs-hack inspect --node-type struct-literal --name IRCtx
2. Previews changes: rs-hack add-struct-field ... --format diff
3. Shows you the diff for approval
4. Applies: rs-hack add-struct-field ... --apply
5. Verifies: cargo check
6. Reports: ✓ Added field to 15 struct literals across 8 files
```

**Workflow 2: Clean Up Debug Code**
```
User: "Comment out all eprintln! macros with [DEBUG] in them"

Claude:
1. Finds matches: rs-hack inspect --node-type macro-call --name eprintln --content-filter "[DEBUG]"
2. Previews: rs-hack transform ... --action comment --format diff
3. Applies: rs-hack transform ... --action comment --apply
4. Reports: ✓ Commented out 42 debug statements
```

**Workflow 3: Safe Experimentation**
```
User: "Try adding Clone to all structs and see if tests pass"

Claude:
1. Applies: rs-hack add-derive ... --apply
   (saves run ID: a05a626)
2. Tests: cargo test
   (tests fail!)
3. Reverts: rs-hack revert a05a626
4. Reports: Reverted changes, tests were incompatible
```

### Why It Works Well

- **Type-safe**: Claude won't corrupt code with sed/awk
- **Reversible**: Every change tracked for easy revert
- **Guided**: Skill file teaches best practices
- **Fast**: Bulk operations across entire codebase
- **Verifiable**: Dry-run mode prevents accidents

### Without Skill File

Claude can still use rs-hack effectively by reading the `--help` output, but the skill provides:
- Pre-learned command patterns
- Workflow best practices
- Common use case examples
- Error recovery patterns

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

## Features by Version

### v0.4.2 - Enum Variant Renaming (Current) ⭐
- **`rename-enum-variant` command**: Type-safe enum variant renaming across entire codebase
  - Renames variant in enum definitions, match patterns, constructors, and all usages
  - Handles fully qualified (`Enum::Variant`) and imported (`Variant`) paths
  - AST-aware: won't rename strings, comments, or unrelated identifiers
  - Real-world impact: 4-6 hour refactor → 30 seconds
  - Supports glob patterns for multi-file operations
  - Integrates with state/revert system
  - Example: `IRValue::HashMapV2` → `IRValue::HashMap` across 23 files

### v0.4.0 - Generic Transform & Macro Support ⭐
- **`transform` command**: Generic find-and-modify operation for ANY AST nodes
  - Single command replaces need for dozens of specialized operations
  - Actions: `comment`, `remove`, `replace`
  - Works with all node types from `inspect`
  - Perfect for AI agents: one pattern to learn
  - Content filtering for precise targeting
- **Macro call support**: Find and modify macro invocations
  - New `macro-call` node type in `inspect` and `transform`
  - Great for cleaning up debug logs: `eprintln!`, `println!`, `todo!`, `dbg!`
  - Content filtering: target specific debug categories
- **Enhanced `inspect`**: Added `--content-filter` flag
  - Filter nodes by their source code content
  - Combine name and content filters for surgical precision
- **Pattern matching for struct literals**: Distinguish between struct literals and enum variants
  - `--struct-name "Rectangle"` → only pure struct literals `Rectangle { ... }`
  - `--struct-name "*::Rectangle"` → any enum variant ending with `Rectangle` (e.g., `View::Rectangle { ... }`)
  - `--struct-name "View::Rectangle"` → exact path match only
  - Prevents accidental modification of enum variant constructors
- **Simplified struct field commands**: `add-struct-field` now handles all cases intelligently
  - No `--literal-default` → definition only
  - `--literal-default` with type (`field: Type`) → tries definition (idempotent) + always adds to literals
  - `--literal-default` without type (`field`) → literals only (skips definition)
  - Common case: field exists, just use `--field "name" --literal-default "value"`
  - Eliminates confusing dual-command footgun, natural API
- **State migration**: Automatic handling of incompatible state from previous versions
- **Integration tests**: 36 tests including transform, pattern matching, and literal-only mode
- **Documentation**: Comprehensive examples and workflow guides

### v0.3.2 - Pattern-Based Filtering & Inspection
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

Contributions are welcome! Please feel free to submit issues or pull requests on [GitHub](https://github.com/1e1f/rs-hack).

## License

MIT OR Apache-2.0

## Credits

Built for AI agents to stop using sed on Rust code. 🦀

Created by Leif Shackelford ([@1e1f](https://github.com/1e1f))
