# rs-hack Human Reference

Ultra-concise command reference for quick lookup.

> **For Claude Code:** See [.claude/skills/rs-hack.md](.claude/skills/rs-hack.md)
> **For Complete Docs:** See [README.md](README.md)

## Command Format

```bash
rs-hack <COMMAND> [OPTIONS] --apply
```

**Note:** Omit `--apply` for dry-run (preview only).

## Global Flags

```bash
--paths "pattern"      # File, dir, or glob: "src/**/*.rs"
--where "filter"       # Filter: "derives_trait:Clone" or "derives_trait:Clone,Debug"
--exclude "pattern"    # Exclude paths: "**/tests/**" (can use multiple) ⭐ v0.4.3
--format diff          # Preview as git diff
--format summary       # Show only changed lines (cleaner) ⭐ v0.4.3
--apply                # Actually modify (default is dry-run)
```

## Struct Commands

```bash
# Add field (idempotent)
rs-hack add-struct-field --paths FILE --struct-name NAME \
  --field "name: Type" [--position POS] [--literal-default "value"] \
  [--where "derives_trait:Clone"] --apply

# Update field
rs-hack update-struct-field --paths FILE --struct-name NAME \
  --field "name: NewType" [--where "filter"] --apply

# Remove field (from BOTH definition AND all literals)
rs-hack remove-struct-field --paths FILE --struct-name NAME \
  --field-name name [--where "filter"] --apply

# Remove field from enum variant (use EnumName::VariantName syntax)
rs-hack remove-struct-field --paths FILE --struct-name "View::Rectangle" \
  --field-name field_name --apply

# Add to struct literals only (v0.4.0+)
# Simply omit the type - no ':' means literals only
rs-hack add-struct-field --paths FILE --struct-name NAME \
  --field "name" --literal-default "value" [--position POS] --apply

# Pattern matching for --struct-name (v0.4.0+):
# "Rectangle"         → Only Rectangle { ... } (pure struct literals)
# "*::Rectangle"      → Any path ending with Rectangle (enum variants too)
# "View::Rectangle"   → Exact match only View::Rectangle { ... }
```

## Enum Commands

```bash
# Add variant (idempotent)
rs-hack add-enum-variant --paths FILE --enum-name NAME \
  --variant "Variant" [--position POS] [--where "filter"] --apply

# Update variant
rs-hack update-enum-variant --paths FILE --enum-name NAME \
  --variant "Variant { field: Type }" [--where "filter"] --apply

# Remove variant
rs-hack remove-enum-variant --paths FILE --enum-name NAME \
  --variant-name Variant [--where "filter"] --apply

# Rename variant across codebase (AST-aware, type-safe) ⭐ v0.4.0+
rs-hack rename-enum-variant --paths "src/**/*.rs" --enum-name NAME \
  --old-variant OldName --new-variant NewName \
  [--validate] [--format summary] --apply

# Validate rename (check for remaining references) ⭐ v0.4.3
rs-hack rename-enum-variant --paths "src/**/*.rs" --enum-name NAME \
  --old-variant OldName --new-variant NewName --validate
```

## Function Commands

```bash
# Rename function across codebase ⭐ v0.4.0+
rs-hack rename-function --paths "src/**/*.rs" \
  --old-name old_func --new-name new_func \
  [--validate] [--format summary] --apply
```

## Doc Comment Operations ⭐ v0.4.3

```bash
# Add documentation comment
rs-hack add-doc-comment --paths "src/**/*.rs" \
  --target-type struct --name User \
  --doc-comment "Represents a user in the system" \
  [--style line|block] --apply

# Update existing documentation
rs-hack update-doc-comment --paths "src/**/*.rs" \
  --target-type function --name process \
  --doc-comment "Updated documentation" --apply

# Remove documentation
rs-hack remove-doc-comment --paths "src/**/*.rs" \
  --target-type enum --name Status --apply

# Supported targets: struct, enum, function
# Styles: line (///), block (/** */)
```

## Match Commands

```bash
# Add match arm (idempotent)
rs-hack add-match-arm --paths FILE --pattern "Enum::Variant" \
  --body "expr" [--function NAME] --apply

# Auto-detect missing arms
rs-hack add-match-arm --paths FILE --auto-detect --enum-name NAME \
  --body "todo!()" [--function NAME] --apply

# Update match arm
rs-hack update-match-arm --paths FILE --pattern "Enum::Variant" \
  --body "new_expr" [--function NAME] --apply

# Remove match arm
rs-hack remove-match-arm --paths FILE --pattern "Enum::Variant" \
  [--function NAME] --apply
```

## Derive Commands

```bash
# Add derives (idempotent)
rs-hack add-derive --paths FILE --target-type struct --name NAME \
  --derives "Clone,Debug" [--where "derives_trait:Serialize"] --apply
```

## Transform - Generic Find & Modify

```bash
# Comment out matching nodes
rs-hack transform --paths "src/**/*.rs" --node-type macro-call \
  --name eprintln --content-filter "[SHADOW RENDER]" \
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

## Batch Operations ⭐ v0.4.3

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
EOF

rs-hack batch --spec migrations.yaml \
  [--exclude "**/tests/**"] --apply

# JSON format also supported (backward compatible)
rs-hack batch --spec migrations.json --apply
```

## Inspection Commands

```bash
# Find definition location
rs-hack find --paths FILE --node-type struct --name User

# Inspect struct literals (better than grep)
rs-hack inspect --paths "tests/*.rs" --node-type struct-literal \
  --name Shadow [--format snippets|locations|json]

# Inspect match arms (find enum variant handling)
rs-hack inspect --paths "src/**/*.rs" --node-type match-arm \
  --name "Operator::AssertSome" [--format snippets|locations|json]

# Inspect enum usages (find ALL references to enum variants)
rs-hack inspect --paths "src/**/*.rs" --node-type enum-usage \
  --name "Operator::PropagateError" [--format snippets|locations|json]

# Inspect function calls
rs-hack inspect --paths "src/**/*.rs" --node-type function-call \
  --name "handle_error" [--format snippets|locations|json]

# Inspect method calls (great for auditing .unwrap())
rs-hack inspect --paths "src/**/*.rs" --node-type method-call \
  --name "unwrap" [--format snippets|locations|json]

# Inspect macro calls
rs-hack inspect --paths "src/**/*.rs" --node-type macro-call \
  --name "eprintln" [--content-filter "[DEBUG]"] [--format snippets|locations|json]

# Output formats:
# snippets  - Full code on single line (default)
# locations - file:line:col (grep-like)
# json      - Structured data

# Content filter:
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
--paths "src/**/*.rs"        # All .rs files recursively
--paths "src/models/*.rs"    # Files in specific dir
--paths "tests/shadow_*.rs"  # Wildcard matching

# Exclude patterns ⭐ v0.4.3
--exclude "**/tests/**"           # Skip all test directories
--exclude "**/fixtures/**"        # Skip fixtures
--exclude "**/deprecated/**"      # Skip deprecated code
# Multiple excludes: use --exclude multiple times

# Filter by traits (OR logic)
--where "derives_trait:Clone"           # Has Clone
--where "derives_trait:Clone,Debug"     # Has Clone OR Debug

# Preview before applying
--format diff               # Show git-style diff
--format summary            # Show only changed lines ⭐ v0.4.3
--apply                     # Then apply when ready

# Validation ⭐ v0.4.3
--validate                  # Check for remaining references (rename ops)

# Combine for power
rs-hack add-struct-field \
  --paths "src/**/*.rs" \
  --exclude "**/tests/**" \
  --struct-name Config \
  --field "version: u32" \
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

```bash
# Workflow 1: Safe Rename with Validation ⭐ NEW
# 1. Validate what will be renamed
rs-hack rename-enum-variant --paths "src/**/*.rs" \
  --enum-name Status --old-variant Draft --new-variant Pending \
  --validate

# 2. Preview with summary format
rs-hack rename-enum-variant --paths "src/**/*.rs" \
  --enum-name Status --old-variant Draft --new-variant Pending \
  --format summary

# 3. Apply
rs-hack rename-enum-variant --paths "src/**/*.rs" \
  --enum-name Status --old-variant Draft --new-variant Pending \
  --apply

# 4. Validate again to check for missed references
rs-hack rename-enum-variant --paths "src/**/*.rs" \
  --enum-name Status --old-variant Draft --new-variant Pending \
  --validate

# Workflow 2: Batch Operations with Exclusions ⭐ NEW
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

# Workflow 3: Inspect + Transform with Exclusions
# 1. Find what you want to change
rs-hack inspect --paths "src/**/*.rs" \
  --node-type macro-call --name eprintln \
  --content-filter "[DEBUG]" --format locations

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
- Use `--format diff` or `--format summary` to preview
- `add-*` operations are **idempotent**
- `--where` enables **pattern-based filtering**
- `--exclude` skips unwanted paths ⭐ v0.4.3
- `--validate` checks for missed references ⭐ v0.4.3
- `inspect` is **better than grep** (AST-aware)
- **`transform`** is the **generic find-and-modify** command
- `--content-filter` for **precise targeting**
- **YAML batch operations** for complex refactors ⭐ v0.4.3
- **Doc comments** can be added/updated/removed ⭐ v0.4.3
- State is tracked for `revert`
- **Workflow:** `inspect` → preview → validate → `apply`
