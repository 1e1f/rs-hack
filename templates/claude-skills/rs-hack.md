# rs-hack: Claude Code Skill

This skill teaches Claude Code how to use rs-hack for Rust refactoring.

**Installation:** Copy this file to `.claude/skills/rs-hack.md` in your Rust project.

```bash
mkdir -p .claude/skills
curl -o .claude/skills/rs-hack.md \
  https://raw.githubusercontent.com/1e1f/rs-hack/main/templates/claude-skills/rs-hack.md
```

> **For Humans:** See [HUMAN.md](https://github.com/1e1f/rs-hack/blob/main/HUMAN.md)
> **For Complete Docs:** See [README.md](https://github.com/1e1f/rs-hack/blob/main/README.md)

---

Use rs-hack for all Rust code transformations. It's type-safe and AST-aware—never use sed/awk/perl for Rust code.

## Core Philosophy

**Always preview, then apply:**
1. Use `find` to discover what will be affected
2. Run command without `--apply` to see diff (or use `--format diff`)
3. Apply changes with `--apply`
4. Check with `cargo check` or `cargo test`
5. Revert if needed: `rs-hack revert <run-id>`

## Command Patterns

### Rename (Enum Variants, Functions, Trait Methods)

Perfect for large-scale refactoring across many files:

```bash
# Step 1: Check what exists
rs-hack find --paths "src/**/*.rs" --node-type enum-usage \
  --name "EnumName::OldVariant" --format locations

# Step 2: Preview changes (dry-run by default)
rs-hack rename \
  --paths "src/**/*.rs" \
  --name "EnumName::OldVariant" \
  --to NewVariant

# Step 3: Apply
rs-hack rename \
  --paths "src/**/*.rs" \
  --name "EnumName::OldVariant" \
  --to NewVariant \
  --apply

# Rename a function (includes trait methods in v0.5.1+)
rs-hack rename \
  --paths "src/**/*.rs" \
  --name old_function_name \
  --to new_function_name \
  --kind function \
  --apply
```

**What it renames:**
- Enum variant definitions
- Match arm patterns
- Constructor calls
- Reference patterns (`&Enum::Variant`)
- Function definitions and calls
- Trait method definitions and implementations
- All usages across the codebase

### Add Struct Field

```bash
# Add to struct definition AND all literals (v0.5.1+)
rs-hack add \
  --name StructName \
  --field-name "field_name" \
  --field-type "Type" \
  --field-value "default_value" \
  --kind struct \
  --paths "src/**/*.rs" \
  --apply

# Add to literals only (field already exists in definition)
rs-hack add \
  --name StructName \
  --field-name "field_name" \
  --field-value "default_value" \
  --kind struct \
  --paths "src/**/*.rs" \
  --apply

# Add field to ENUM VARIANT struct literals (use Enum::Variant syntax)
rs-hack add \
  --name "View::Grid" \
  --field-name "drag_clip_behavior" \
  --field-value "None" \
  --kind struct \
  --paths "src/**/*.rs" \
  --apply

# ⚠️  IMPORTANT: --variant is for adding a NEW variant to an enum, NOT for adding fields!
# WRONG:  rs-hack add --name View --variant Grid --field-name foo
# RIGHT:  rs-hack add --name "View::Grid" --field-name foo --field-value bar --kind struct
```

### Transform: Generic Find & Modify

```bash
# Comment out debug macros
rs-hack transform \
  --paths "src/**/*.rs" \
  --node-type macro-call \
  --name eprintln \
  --content-filter "[DEBUG]" \
  --action comment \
  --apply

# Remove all .unwrap() calls
rs-hack transform \
  --paths "src/**/*.rs" \
  --node-type method-call \
  --name unwrap \
  --action comment \
  --apply
```

### Find: Better than grep

```bash
# Find all struct literals
rs-hack find --paths "src/**/*.rs" \
  --node-type struct-literal --name StructName \
  --format snippets

# Find all enum usages
rs-hack find --paths "src/**/*.rs" \
  --node-type enum-usage --name "Enum::Variant" \
  --format locations

# Find all match arms
rs-hack find --paths "src/**/*.rs" \
  --node-type match-arm --name "Enum::Variant" \
  --format snippets

# Discovery mode: omit --node-type to search ALL types (auto-grouped!)
rs-hack find --paths "src/**/*.rs" \
  --name Rectangle \
  --format snippets
```

### Add Match Arms

```bash
# Add specific arm
rs-hack add \
  --paths src/handler.rs \
  --match-arm "Status::NewVariant" \
  --body "todo!()" \
  --function handle_status \
  --apply

# Auto-detect all missing variants
rs-hack add \
  --paths src/handler.rs \
  --auto-detect \
  --enum-name Status \
  --body "todo!()" \
  --function handle_status \
  --apply
```

### State Management

```bash
# View history
rs-hack history --limit 10

# Revert a change
rs-hack revert <run-id>

# Force revert (even if files changed)
rs-hack revert <run-id> --force
```

## Common Workflows

### Workflow 1: Large-Scale Enum Renaming

```bash
# Example: Rename IRValue::HashMapV2 → HashMap
# (This is what the tool was designed for!)

# 1. Find current usage
rs-hack find --paths "src/**/*.rs" \
  --node-type enum-usage --name "IRValue::HashMapV2" \
  --format locations | wc -l

# 2. Preview changes (dry-run by default)
rs-hack rename \
  --paths "src/**/*.rs" \
  --name "IRValue::HashMapV2" \
  --to HashMap

# 3. Apply changes
rs-hack rename \
  --paths "src/**/*.rs" \
  --name "IRValue::HashMapV2" \
  --to HashMap \
  --apply

# 4. Verify
cargo check
```

### Workflow 2: Add Field to Struct Everywhere

```bash
# Add field to both definition and all initialization sites

# 1. Find struct literals
rs-hack find --paths "src/**/*.rs" \
  --node-type struct-literal --name IRCtx \
  --format locations

# 2. Preview (uses v0.5.1 field API)
rs-hack add \
  --paths "src/**/*.rs" \
  --name IRCtx \
  --field-name "return_type" \
  --field-type "Option<Type>" \
  --field-value "None" \
  --kind struct

# 3. Apply
rs-hack add \
  --paths "src/**/*.rs" \
  --name IRCtx \
  --field-name "return_type" \
  --field-type "Option<Type>" \
  --field-value "None" \
  --kind struct \
  --apply

# 4. Verify
cargo check
```

### Workflow 3: Remove Struct Field Everywhere

```bash
# Remove field from BOTH definitions AND all literal expressions
# The unified remove command does both automatically!

# 1. Find where field is used
rs-hack find --paths "src/**/*.rs" \
  --node-type struct-literal --name Config \
  --content-filter "debug_mode" \
  --format locations

# 2. Preview removal (removes from definition AND literals)
rs-hack remove \
  --paths "src/**/*.rs" \
  --name Config \
  --field-name debug_mode \
  --kind struct

# 3. Apply
rs-hack remove \
  --paths "src/**/*.rs" \
  --name Config \
  --field-name debug_mode \
  --kind struct \
  --apply

# For enum variant fields, use EnumName::VariantName syntax:
rs-hack remove \
  --paths "src/**/*.rs" \
  --name "View::Rectangle" \
  --field-name immediate_mode \
  --kind struct \
  --apply
```

### Workflow 4: Clean Up Debug Code

```bash
# 1. Find debug macros
rs-hack find --paths "src/**/*.rs" \
  --node-type macro-call --name eprintln \
  --content-filter "[DEBUG]" \
  --format locations

# 2. Preview commenting them out
rs-hack transform \
  --paths "src/**/*.rs" \
  --node-type macro-call --name eprintln \
  --content-filter "[DEBUG]" \
  --action comment \
  --format diff

# 3. Apply
rs-hack transform \
  --paths "src/**/*.rs" \
  --node-type macro-call --name eprintln \
  --content-filter "[DEBUG]" \
  --action comment \
  --apply
```

## When to Use rs-hack

✅ **DO use rs-hack for:**
- Renaming enum variants or functions across multiple files (`rename`)
- Adding fields to struct definitions and/or literals (`add --field-name --field-type --field-value`)
- Adding fields to enum variant struct literals (`add --name "Enum::Variant" --field-name --field-value`)
- Removing fields from structs or enum variants (`remove --field-name`)
- Adding match arms for enum variants (`add --match-arm`)
- Commenting out debug macros (`transform --action comment`)
- Any bulk AST-level transformation
- Multi-file refactoring (glob patterns like `"src/**/*.rs"`)

❌ **DON'T use rs-hack for:**
- Single-line edits in one file → use Edit tool
- Simple text replacements → use Edit tool
- Non-Rust files
- Changes requiring semantic/type analysis

## Common Pitfall: --variant vs Enum::Variant

**⚠️ The #1 mistake: Confusing adding a variant vs adding fields to a variant**

```bash
# ❌ WRONG - Trying to add field to enum variant using --variant
rs-hack add --name "View" --variant "Grid" --field-name "foo" --field-value "bar" --paths src
# Error: Cannot combine --variant with --field-name

# ✅ RIGHT - Add a NEW variant to an enum definition
rs-hack add --name "View" --variant "Grid { columns: u32, rows: u32 }" --paths src --apply

# ✅ RIGHT - Add field to existing enum variant struct literals
rs-hack add --name "View::Grid" --field-name "foo" --field-value "bar" --kind struct --paths src --apply
```

**Remember:**
- `--variant` = Add a **new** variant to an enum definition
- `--name "Enum::Variant"` = Target existing enum variant **literals** (for adding/removing fields)

## Common Pitfall: Qualified Paths (v0.5.3+)

**⚠️ Missing struct literals because they use qualified paths**

```bash
# You try this and only get partial matches:
rs-hack add --name "TouchableProps" --field-name "on_long_press" \
  --field-value "None" --kind struct --paths src

# rs-hack shows a hint:
# ⚠️  Note: Some instances were not matched:
# 💡 Hint: Found 6 struct literal(s) with fully qualified paths:
#    crate::view::builder::TouchableProps (6 instances)
# To match all, use: --name "*::TouchableProps"

# ✅ FIX - Use wildcard to match all qualified paths
rs-hack add --name "*::TouchableProps" --field-name "on_long_press" \
  --field-value "None" --kind struct --paths src --apply
```

**Remember:**
- Simple names only match unqualified literals: `StructName { ... }`
- Use `*::StructName` to match ALL paths: `mod::StructName`, `crate::path::StructName`, etc.

## Quick Reference

### Field API (v0.5.1+)

| What You Want | Flags to Use | Example |
|--------------|--------------|---------|
| Add to struct **definition** only | `--field-name` + `--field-type` | `--field-name email --field-type String` |
| Add to struct **literals** only | `--field-name` + `--field-value` | `--field-name email --field-value "\"\"" ` |
| Add to **both** definition + literals | `--field-name` + `--field-type` + `--field-value` | `--field-name email --field-type String --field-value "\"\"" ` |
| Add to enum variant **literals** | `--name "Enum::Variant"` + `--field-name` + `--field-value` + `--kind struct` | `--name "View::Grid" --field-name gap --field-value None --kind struct` |

### --kind vs --node-type

| Flag | Purpose | When to Use |
|------|---------|-------------|
| `--kind struct` | Semantic grouping: includes struct definitions AND struct literals | When you want to operate on all struct-related nodes |
| `--node-type struct` | Granular control: ONLY struct definitions | When you want to target just definitions |
| `--node-type struct-literal` | Granular control: ONLY struct initialization expressions | When you want to target just literals |

**Rule of thumb:** Use `--kind` for broad operations, `--node-type` for surgical precision.

### Glob Patterns

```bash
"src/**/*.rs"          # All .rs files in src and subdirectories (most common)
"src/models/*.rs"      # All .rs files directly in src/models
"src/**/handler.rs"    # All handler.rs files anywhere under src
"**/*.rs"              # All .rs files in entire project (careful!)
```

### Key Flags

```bash
--paths "pattern"      # File/dir/glob: "src/**/*.rs"
--format diff          # Preview as git diff
--summary              # Show stats (files/lines changed)
--apply                # Actually modify (dry-run by default)
--where "filter"       # Filter by traits: "derives_trait:Clone"
--kind <type>          # Semantic grouping (struct, enum, function)
--node-type <type>     # Granular AST node type (struct-literal, enum-usage)
```

## Best Practices

1. **Always find first** - Know what you're changing with `rs-hack find`
2. **Always dry-run** - Preview with `--format diff` before `--apply`
3. **Use glob patterns** - Target multiple files: `"src/**/*.rs"`
4. **Check after apply** - Run `cargo check` or tests
5. **Save run IDs** - `rs-hack history` shows recent changes
6. **Revert when needed** - `rs-hack revert <run-id>` is your safety net

## Error Recovery

If something goes wrong:

```bash
# Check what was done
rs-hack history

# Revert the last change
rs-hack revert <run-id>

# Force revert if files changed since
rs-hack revert <run-id> --force
```

## Remember

**rs-hack is type-safe and AST-aware.** It will:
- ✅ Only rename actual Rust code structures
- ✅ Preserve formatting and comments
- ✅ Work across any number of files
- ✅ Track changes for revert
- ❌ Never corrupt strings or comments
- ❌ Never make partial matches like sed

**When in doubt:** `find` → preview with `--format diff` → `apply` → `cargo check` → revert if needed
