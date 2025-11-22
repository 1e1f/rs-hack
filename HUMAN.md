# rs-hack Human Reference

Ultra-concise command reference for quick lookup.

> **For Claude Code:** See [.claude/skills/rs-hack.md](.claude/skills/rs-hack.md)
> **For Complete Docs:** See [README.md](README.md)

## v0.5.0 Quick Reference ⭐ NEW

**5 Unified Commands:**
```bash
find      # Discover what exists
add       # Add fields, variants, methods, derives
remove    # Remove fields, variants, methods
update    # Update fields, variants
rename    # Rename functions, enum variants
```

**Discovery Workflow:**
```bash
# 1. Find what exists
rs-hack find --name X                              # Search all types (auto-grouped)
rs-hack find --kind struct --name X                # Semantic grouping
rs-hack find --node-type struct-literal --name X   # Granular control

# 2. Operate on it (same --name syntax)
rs-hack add --name X --field "..."
rs-hack remove --name X --field-name ...
rs-hack update --name X --field "..."
rs-hack rename --name X --to Y
```

**Key Flags:**
```bash
--kind <KIND>          # Semantic grouping: struct, function, enum, match, etc.
--node-type <TYPE>     # Granular: struct-literal, function-call, etc.
--name <NAME>          # Target name (consistent across all commands)
--paths <PATTERN>      # File, dir, or glob
--apply                # Actually modify (default is dry-run)
```

## Command Format

```bash
rs-hack <COMMAND> [OPTIONS] --apply
```

**Note:** Omit `--apply` for dry-run (preview only).

## Global Flags

```bash
--paths "pattern"      # File, dir, or glob: "src/**/*.rs"
--where "filter"       # Filter: "derives_trait:Clone" or "derives_trait:Clone,Debug"
--exclude "pattern"    # Exclude paths: "**/tests/**" (can use multiple)
--format diff          # Preview as git diff
--format summary       # Show only changed lines (cleaner)
--apply                # Actually modify (default is dry-run)
```

## Unified Commands (v0.5.0+)

### Find Command

```bash
# Discover what exists (searches ALL types, auto-grouped output)
rs-hack find --name Rectangle --paths src

# Use --kind for semantic grouping
rs-hack find --kind struct --name Config --paths src    # Definitions + literals
rs-hack find --kind function --name handle --paths src  # Definitions + calls
rs-hack find --kind enum --name Status --paths src      # Definitions + usages

# Use --node-type for granular control
rs-hack find --node-type struct --name Config --paths src           # Only definitions
rs-hack find --node-type struct-literal --name Config --paths src   # Only literals
rs-hack find --node-type function-call --name handle --paths src    # Only calls

# Find with variant filtering
rs-hack find --kind enum --variant Rectangle --paths src        # Any enum with Rectangle
rs-hack find --name View::Rectangle --paths src                 # View enum, Rectangle variant

# Wildcard patterns for qualified paths (v0.5.3+)
rs-hack find --name "TouchableProps" --paths src                # Only simple paths
rs-hack find --name "*::TouchableProps" --paths src             # All qualified paths
rs-hack find --name "crate::view::builder::TouchableProps" --paths src  # Exact path

# Find with content filtering
rs-hack find --node-type macro-call --name eprintln \
  --content-filter "[DEBUG]" --paths src

# Output formats
rs-hack find --name User --paths src --format snippets    # Code snippets (default)
rs-hack find --name User --paths src --format locations   # file:line:col
rs-hack find --name User --paths src --format json        # Structured data
```

**Supported `--kind` values:**
- `struct` - Struct definitions + struct literals
- `function` - Function definitions + function calls
- `enum` - Enum definitions + enum usages
- `match` - Match expressions + match arms
- `identifier` - Identifier references
- `type` - Type aliases + type references
- `macro` - Macro definitions + macro calls
- `const`, `trait`, `mod`, `use`

**Supported `--node-type` values:**
- Definitions: `struct`, `enum`, `function`, `impl-method`, `trait`, `const`, `static`, `type-alias`, `mod`
- Expressions: `struct-literal`, `enum-usage`, `function-call`, `method-call`, `macro-call`, `match-arm`, `identifier`, `type-ref`

### Add Command

```bash
# Add struct field (auto-detects it's a struct)
rs-hack add --name User --field "email: String" --paths src --apply

# Add field with position
rs-hack add --name Config --field "timeout: u64" \
  --position "after:port" --paths src --apply

# Add field to BOTH definition AND all literals
rs-hack add --name IRCtx --field "return_type: Option<Type>" \
  --literal-default "None" --paths "src/**/*.rs" --apply

# Add to literals only (omit type)
rs-hack add --name IRCtx --field "return_type" \
  --literal-default "None" --paths "src/**/*.rs" --apply

# Wildcard patterns for qualified paths (v0.5.3+)
rs-hack add --name "*::TouchableProps" --field-name "on_long_press" \
  --field-value "None" --paths src --apply  # Matches all qualified paths

# Add enum variant (auto-detects it's an enum)
rs-hack add --name Status --variant "Archived" --paths src --apply

# Add variant with fields
rs-hack add --name Message --variant "Error { code: i32, msg: String }" \
  --paths src --apply

# Add derive (auto-detects target type)
rs-hack add --name User --derive "Clone,Debug,Serialize" --paths src --apply

# Add method to impl block
rs-hack add --name User \
  --method 'pub fn get_id(&self) -> u64 { self.id }' \
  --paths src --apply

# Add use statement
rs-hack add --use "serde::Serialize" --paths src --apply

# Add match arm
rs-hack add --name Status --match-arm "Status::Archived" \
  --body '"archived".to_string()' --paths src --apply

# Auto-detect missing match arms
rs-hack add --auto-detect --enum-name Status \
  --body "todo!()" --function handle_status --paths src --apply

# Add documentation comment (requires --node-type or --kind)
rs-hack add --name User --node-type struct \
  --doc-comment "Represents a user" --paths src --apply

# Use --kind to affect both definitions and expressions
rs-hack add --name Config --kind struct --derive "Debug" --paths src --apply
```

**Auto-detection logic:**
- `--field` → struct field operation
- `--variant` → enum variant operation
- `--method` → impl method operation
- `--use` → use statement operation
- `--derive` → derive macro operation
- `--match-arm` → match arm operation

### Remove Command

```bash
# Remove struct field (auto-detects it's a struct)
rs-hack remove --name User --field-name email --paths src --apply

# Wildcard patterns for qualified paths (v0.5.3+)
rs-hack remove --name "*::TouchableProps" --field-name "on_tap" \
  --literal-only --paths src --apply  # Matches all qualified paths

# Remove enum variant field (definition + all literals)
rs-hack remove --name View::Rectangle --field-name color --paths src --apply

# Remove from literals only
rs-hack remove --name View::Rectangle --field-name color \
  --literal-only --paths src --apply

# Remove enum variant
rs-hack remove --name Status --variant Draft --paths src --apply

# Remove derive
rs-hack remove --name User --derive Clone --paths src --apply

# Remove match arm
rs-hack remove --name Status --match-arm "Status::Draft" --paths src --apply

# Remove documentation comment (requires --node-type or --kind)
rs-hack remove --name User --node-type struct --doc-comment --paths src --apply

# Use --kind for semantic grouping
rs-hack remove --name Config --kind struct --derive Debug --paths src --apply
```

### Update Command

```bash
# Update struct field visibility
rs-hack update --name User --field "pub email: String" --paths src --apply

# Update struct field type
rs-hack update --name User --field "id: i64" --paths src --apply

# Update enum variant
rs-hack update --name Status \
  --variant "Draft { created_at: u64 }" --paths src --apply

# Update match arm
rs-hack update --name Status --match-arm "Status::Draft" \
  --body '"pending".to_string()' --paths src --apply

# Update documentation comment (requires --node-type or --kind)
rs-hack update --name User --node-type struct \
  --doc-comment "Updated user model" --paths src --apply

# Use --kind for semantic grouping
rs-hack update --name Config --kind struct --field "port: u32" --paths src --apply
```

### Rename Command

```bash
# Rename enum variant across entire codebase
rs-hack rename --name Status::Draft --to Pending --paths "src/**/*.rs" --apply

# Rename function across entire codebase
rs-hack rename --name process_v2 --to process --paths "src/**/*.rs" --apply

# Validate rename (check for remaining references)
rs-hack rename --name Status::Draft --to Pending \
  --validate --paths "src/**/*.rs"

# Use --kind for disambiguation
rs-hack rename --name handle_error --to process_error \
  --kind function --paths src --apply

# Use --node-type for granular control
rs-hack rename --name unwrap --to expect \
  --node-type method-call --paths src --apply

# Preview with summary format
rs-hack rename --name Status::Active --to Enabled \
  --format summary --paths src
```

**What it renames:**
- Enum variants: definitions, match patterns, constructors, references
- Functions: definitions, calls, references
- With `--node-type`: Specific expression-level nodes (function-call, method-call, identifier, etc.)

## Transform - Generic Find & Modify

```bash
# Comment out matching nodes
rs-hack transform --paths "src/**/*.rs" --node-type macro-call \
  --name eprintln --content-filter "[DEBUG]" \
  --action comment --apply

# Remove matching nodes
rs-hack transform --paths "src/**/*.rs" --node-type method-call \
  --name unwrap --action remove --apply

# Replace matching nodes
rs-hack transform --paths "src/**/*.rs" --node-type function-call \
  --name old_handler --action replace --with "new_handler()" --apply

# Actions:
# comment  - Wrap in // comment
# remove   - Delete entirely
# replace  - Replace with provided code (requires --with)

# Node types: macro-call, method-call, function-call, enum-usage,
#             struct-literal, match-arm, identifier, type-ref
```

## Batch Operations

```bash
# YAML format (easier to write)
cat > migrations.yaml << 'EOF'
base_path: src/
operations:
  - type: RenameEnumVariant
    enum_name: Status
    old_variant: DraftV2
    new_variant: Draft
  - type: RenameFunction
    old_name: process_v2
    new_name: process
  - type: AddDocComment
    target_type: struct
    name: User
    doc_comment: "User model"
EOF

rs-hack batch --spec migrations.yaml \
  --exclude "**/tests/**" --apply

# JSON format also supported (backward compatible)
rs-hack batch --spec migrations.json --apply
```

## State Commands

```bash
rs-hack history [--limit 10]           # Show recent runs
rs-hack revert <run-id> [--force]      # Undo changes
rs-hack clean [--keep-days 30]         # Clean old state
```

## Common Patterns

```bash
# Glob patterns
--paths "src/**/*.rs"        # All .rs files recursively
--paths "src/models/*.rs"    # Files in specific dir
--paths "tests/shadow_*.rs"  # Wildcard matching

# Exclude patterns
--exclude "**/tests/**"           # Skip all test directories
--exclude "**/fixtures/**"        # Skip fixtures
--exclude "**/deprecated/**"      # Skip deprecated code
# Multiple excludes: use --exclude multiple times

# Filter by traits (OR logic)
--where "derives_trait:Clone"           # Has Clone
--where "derives_trait:Clone,Debug"     # Has Clone OR Debug

# Preview before applying
--format diff               # Show git-style diff
--format summary            # Show only changed lines
--apply                     # Then apply when ready

# Validation
--validate                  # Check for remaining references (rename ops)

# Combine for power
rs-hack add --name Config --field "version: u32" \
  --paths "src/**/*.rs" \
  --exclude "**/tests/**" \
  --where "derives_trait:Serialize" \
  --format summary
```

## Position Options

```
first           # Start of list
last            # End of list (default)
after:name      # After specific field/variant/method
before:name     # Before specific field/variant/method
```

## Common Workflows

### Workflow 1: Discovery → Operation
```bash
# 1. Discover what exists
rs-hack find --name Rectangle --paths src

# 2. Operate on it (same --name)
rs-hack add --name Rectangle --field "color: String" --paths src --apply
```

### Workflow 2: Safe Rename with Validation
```bash
# 1. Validate what will be renamed
rs-hack rename --name Status::Draft --to Pending \
  --validate --paths "src/**/*.rs"

# 2. Preview with summary format
rs-hack rename --name Status::Draft --to Pending \
  --format summary --paths "src/**/*.rs"

# 3. Apply
rs-hack rename --name Status::Draft --to Pending \
  --paths "src/**/*.rs" --apply

# 4. Validate again to check for missed references
rs-hack rename --name Status::Draft --to Pending \
  --validate --paths "src/**/*.rs"
```

### Workflow 3: Batch Operations with Exclusions
```bash
cat > refactor.yaml << 'EOF'
base_path: src/
operations:
  - type: RenameEnumVariant
    enum_name: Status
    old_variant: ActiveV2
    new_variant: Active
  - type: AddDocComment
    target_type: enum
    name: Status
    doc_comment: "User status enumeration"
EOF

rs-hack batch --spec refactor.yaml \
  --exclude "**/tests/**" \
  --exclude "**/deprecated/**" \
  --format summary --apply
```

### Workflow 4: Inspect + Transform with Exclusions
```bash
# 1. Find what you want to change
rs-hack find --node-type macro-call --name eprintln \
  --content-filter "[DEBUG]" --format locations --paths src

# 2. Preview transformation (exclude tests)
rs-hack transform --paths "src/**/*.rs" \
  --exclude "**/tests/**" \
  --node-type macro-call --name eprintln \
  --content-filter "[DEBUG]" --action comment \
  --format summary

# 3. Apply
rs-hack transform --paths "src/**/*.rs" \
  --exclude "**/tests/**" \
  --node-type macro-call --name eprintln \
  --content-filter "[DEBUG]" --action comment --apply

# 4. Revert if needed
rs-hack history
rs-hack revert <run-id>
```

## Operation Semantics

| Command | If Exists | If Not Exists |
|---------|-----------|---------------|
| `add` | Skip (OK) | Create (OK) |
| `update` | Update (OK) | Error |
| `remove` | Remove (OK) | Error |

## Migration from Legacy Commands

| Old Command | New Command |
|-------------|-------------|
| `add-struct-field --struct-name User` | `add --name User --field` |
| `add-enum-variant --enum-name Status` | `add --name Status --variant` |
| `remove-struct-field --struct-name User` | `remove --name User --field-name` |
| `remove-enum-variant --enum-name Status` | `remove --name Status --variant` |
| `update-struct-field --struct-name User` | `update --name User --field` |
| `update-enum-variant --enum-name Status` | `update --name Status --variant` |
| `rename-enum-variant --enum-name Status --old-variant X` | `rename --name Status::X --to Y` |
| `rename-function --old-name func_v2` | `rename --name func_v2 --to func` |
| `add-derive --target-type struct --name User` | `add --name User --derive` |
| `add-impl-method --target User` | `add --name User --method` |
| `add-use --use-path` | `add --use` |
| `add-match-arm --pattern` | `add --match-arm` |
| `update-match-arm --pattern` | `update --match-arm` |
| `remove-match-arm --pattern` | `remove --match-arm` |
| `find --path src --node-type X --name Y` | `find --paths src --node-type X --name Y` |

**Note:** `--path` is now `--paths` (plural) in all unified commands.

## Remember

- Default is **dry-run** (safe)
- Use `--apply` to modify
- Use `--format diff` or `--format summary` to preview
- `add` operations are **idempotent**
- `--where` enables **pattern-based filtering**
- `--exclude` skips unwanted paths
- `--validate` checks for missed references
- `--kind` for **semantic grouping** (struct, function, enum, etc.)
- `--node-type` for **granular control** (struct-literal, function-call, etc.)
- `find` is **better than grep** (AST-aware)
- **`transform`** is the **generic find-and-modify** command
- `--content-filter` for **precise targeting**
- **YAML batch operations** for complex refactors
- **Doc comments** can be added/updated/removed
- State is tracked for `revert`
- **Workflow:** `find` → preview → validate → `apply`
