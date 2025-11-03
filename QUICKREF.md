# rs-hack Quick Reference

Ultra-concise command reference for AI agents.

## Command Format

```bash
rs-hack <COMMAND> [OPTIONS] --apply
```

**Note:** Omit `--apply` for dry-run (preview only).

## Global Flags

```bash
--path "pattern"       # File, dir, or glob: "src/**/*.rs"
--where "filter"       # Filter: "derives_trait:Clone" or "derives_trait:Clone,Debug"
--format diff          # Preview as git diff
--apply                # Actually modify (default is dry-run)
```

## Struct Commands

```bash
# Add field (idempotent)
rs-hack add-struct-field --path FILE --struct-name NAME \
  --field "name: Type" [--position POS] [--literal-default "value"] \
  [--where "derives_trait:Clone"] --apply

# Update field
rs-hack update-struct-field --path FILE --struct-name NAME \
  --field "name: NewType" [--where "filter"] --apply

# Remove field
rs-hack remove-struct-field --path FILE --struct-name NAME \
  --field-name name [--where "filter"] --apply

# Add to struct literals only
rs-hack add-struct-literal-field --path FILE --struct-name NAME \
  --field "name: value" [--position POS] --apply
```

## Enum Commands

```bash
# Add variant (idempotent)
rs-hack add-enum-variant --path FILE --enum-name NAME \
  --variant "Variant" [--position POS] [--where "filter"] --apply

# Update variant
rs-hack update-enum-variant --path FILE --enum-name NAME \
  --variant "Variant { field: Type }" [--where "filter"] --apply

# Remove variant
rs-hack remove-enum-variant --path FILE --enum-name NAME \
  --variant-name Variant [--where "filter"] --apply
```

## Match Commands

```bash
# Add match arm (idempotent)
rs-hack add-match-arm --path FILE --pattern "Enum::Variant" \
  --body "expr" [--function NAME] --apply

# Auto-detect missing arms
rs-hack add-match-arm --path FILE --auto-detect --enum-name NAME \
  --body "todo!()" [--function NAME] --apply

# Update match arm
rs-hack update-match-arm --path FILE --pattern "Enum::Variant" \
  --body "new_expr" [--function NAME] --apply

# Remove match arm
rs-hack remove-match-arm --path FILE --pattern "Enum::Variant" \
  [--function NAME] --apply
```

## Derive Commands

```bash
# Add derives (idempotent)
rs-hack add-derive --path FILE --target-type struct --name NAME \
  --derives "Clone,Debug" [--where "derives_trait:Serialize"] --apply
```

## Transform - Generic Find & Modify ⭐ NEW

```bash
# Comment out matching nodes
rs-hack transform --path "src/**/*.rs" --node-type macro-call \
  --name eprintln --content-filter "[SHADOW RENDER]" \
  --action comment --apply

# Remove matching nodes
rs-hack transform --path "src/**/*.rs" --node-type method-call \
  --name unwrap --action remove --apply

# Replace matching nodes
rs-hack transform --path "src/**/*.rs" --node-type function-call \
  --name old_handler --action replace --with "new_handler()" --apply

# Actions:
# comment  - Wrap in // comment
# remove   - Delete entirely
# replace  - Replace with provided code (requires --with)

# Node types: macro-call, method-call, function-call, enum-usage,
#             struct-literal, match-arm, identifier, type-ref
```

## Inspection Commands

```bash
# Find definition location
rs-hack find --path FILE --node-type struct --name User

# Inspect struct literals (better than grep)
rs-hack inspect --path "tests/*.rs" --node-type struct-literal \
  --name Shadow [--format snippets|locations|json]

# Inspect match arms (find enum variant handling)
rs-hack inspect --path "src/**/*.rs" --node-type match-arm \
  --name "Operator::AssertSome" [--format snippets|locations|json]

# Inspect enum usages (find ALL references to enum variants)
rs-hack inspect --path "src/**/*.rs" --node-type enum-usage \
  --name "Operator::PropagateError" [--format snippets|locations|json]

# Inspect function calls
rs-hack inspect --path "src/**/*.rs" --node-type function-call \
  --name "handle_error" [--format snippets|locations|json]

# Inspect method calls (great for auditing .unwrap())
rs-hack inspect --path "src/**/*.rs" --node-type method-call \
  --name "unwrap" [--format snippets|locations|json]

# Inspect macro calls ⭐ NEW
rs-hack inspect --path "src/**/*.rs" --node-type macro-call \
  --name "eprintln" [--content-filter "[DEBUG]"] [--format snippets|locations|json]

# Inspect identifier references
rs-hack inspect --path "src/**/*.rs" --node-type identifier \
  --name "config" [--format snippets|locations|json]

# Inspect type usages
rs-hack inspect --path "src/**/*.rs" --node-type type-ref \
  --name "Vec" [--format snippets|locations|json]

# Output formats:
# snippets  - Full code on single line (default)
# locations - file:line:col (grep-like)
# json      - Structured data

# Content filter (NEW):
# --content-filter "text"  - Only show nodes containing this text
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
--path "src/**/*.rs"        # All .rs files recursively
--path "src/models/*.rs"    # Files in specific dir
--path "tests/shadow_*.rs"  # Wildcard matching

# Filter by traits (OR logic)
--where "derives_trait:Clone"           # Has Clone
--where "derives_trait:Clone,Debug"     # Has Clone OR Debug

# Preview before applying
--format diff               # Show git-style diff
--apply                     # Then apply when ready

# Combine for power
rs-hack add-struct-field \
  --path "src/**/*.rs" \
  --struct-name Config \
  --field "version: u32" \
  --where "derives_trait:Serialize" \
  --format diff
```

## Position Options

```
first           # Start of list
last            # End of list (default)
after:name      # After specific field/variant/method
before:name     # Before specific field/variant/method
```

## Common Workflows

```bash
# Workflow 1: Inspect + Transform (NEW!)
# 1. Find what you want to change
rs-hack inspect --path "src/**/*.rs" --node-type macro-call \
  --name eprintln --content-filter "[DEBUG]" --format locations

# 2. Preview transformation
rs-hack transform --path "src/**/*.rs" --node-type macro-call \
  --name eprintln --content-filter "[DEBUG]" --action comment

# 3. Apply
rs-hack transform --path "src/**/*.rs" --node-type macro-call \
  --name eprintln --content-filter "[DEBUG]" --action comment --apply

# 4. Revert if needed
rs-hack history
rs-hack revert <run-id>

# Workflow 2: Struct field modification
# 1. Inspect first
rs-hack inspect --path "tests/*.rs" --node-type struct-literal \
  --name Shadow --format snippets

# 2. Preview changes
rs-hack add-struct-literal-field --path "tests/*.rs" \
  --struct-name Shadow --field "mode: None" --format diff

# 3. Apply
rs-hack add-struct-literal-field --path "tests/*.rs" \
  --struct-name Shadow --field "mode: None" --apply

# 4. Check history
rs-hack history

# 5. Revert if needed
rs-hack revert <run-id>
```

## Operation Semantics

| Command | If Exists | If Not Exists |
|---------|-----------|---------------|
| `add-*` | Skip (OK) | Create (OK) |
| `update-*` | Update (OK) | Error |
| `remove-*` | Remove (OK) | Error |

## Field/Variant Examples

```rust
// Fields
field: u32
pub field: String
email: Option<String>
tags: Vec<String>
pub(crate) data: Arc<Mutex<T>>

// Enum variants
Pending
Error(String)
User { id: u64, name: String }
```

## Remember

- Default is **dry-run** (safe)
- Use `--apply` to modify
- Use `--format diff` to preview
- `add-*` operations are **idempotent**
- `--where` enables **pattern-based filtering**
- `inspect` is **better than grep** (AST-aware)
- **`transform`** is the **generic find-and-modify** command (⭐ NEW)
- `--content-filter` for **precise targeting** (⭐ NEW)
- State is tracked for `revert`
- **Workflow:** `inspect` → preview → `transform --apply`
