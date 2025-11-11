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
1. Use `inspect` to find what will be affected
2. Run command without `--apply` to see diff
3. Apply changes with `--apply`
4. Check with `cargo check` or `cargo test`
5. Revert if needed: `rs-hack revert <run-id>`

## Command Patterns

### Rename Enum Variant (NEW!)

Perfect for large-scale refactoring across many files:

```bash
# Step 1: Check what exists
rs-hack inspect --paths "src/**/*.rs" --node-type enum-usage \
  --name "EnumName::OldVariant" --format locations

# Step 2: Preview changes
rs-hack rename-enum-variant \
  --paths "src/**/*.rs" \
  --enum-name EnumName \
  --old-variant OldVariant \
  --new-variant NewVariant \
  --format diff

# Step 3: Apply
rs-hack rename-enum-variant \
  --paths "src/**/*.rs" \
  --enum-name EnumName \
  --old-variant OldVariant \
  --new-variant NewVariant \
  --apply
```

**What it renames:**
- Enum variant definitions
- Match arm patterns
- Constructor calls
- Reference patterns (`&Enum::Variant`)
- All usages across the codebase

### Add Struct Field

```bash
# Add to definition AND all literals in one command
rs-hack add-struct-field \
  --paths "src/**/*.rs" \
  --struct-name StructName \
  --field "field_name: Type" \
  --literal-default "default_value" \
  --position "after:existing_field" \
  --apply

# Add to literals only (field already in definition)
rs-hack add-struct-field \
  --paths "src/**/*.rs" \
  --struct-name StructName \
  --field "field_name" \
  --literal-default "default_value" \
  --apply
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

### Inspect: Better than grep

```bash
# Find all struct literals
rs-hack inspect --paths "src/**/*.rs" \
  --node-type struct-literal --name StructName \
  --format snippets

# Find all enum usages
rs-hack inspect --paths "src/**/*.rs" \
  --node-type enum-usage --name "Enum::Variant" \
  --format locations

# Find all match arms
rs-hack inspect --paths "src/**/*.rs" \
  --node-type match-arm --name "Enum::Variant" \
  --format snippets
```

### Add Match Arms

```bash
# Add specific arm
rs-hack add-match-arm \
  --paths src/handler.rs \
  --pattern "Status::NewVariant" \
  --body "todo!()" \
  --function handle_status \
  --apply

# Auto-detect all missing variants
rs-hack add-match-arm \
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

# 1. Inspect current usage
rs-hack inspect --paths "src/**/*.rs" \
  --node-type enum-usage --name "IRValue::HashMapV2" \
  --format locations | wc -l

# 2. Preview changes
rs-hack rename-enum-variant \
  --paths "src/**/*.rs" \
  --enum-name IRValue \
  --old-variant HashMapV2 \
  --new-variant HashMap \
  --format diff --summary

# 3. Apply changes
rs-hack rename-enum-variant \
  --paths "src/**/*.rs" \
  --enum-name IRValue \
  --old-variant HashMapV2 \
  --new-variant HashMap \
  --apply

# 4. Verify
cargo check
```

### Workflow 2: Add Field to Struct Everywhere

```bash
# Add field to both definition and all initialization sites

# 1. Inspect struct literals
rs-hack inspect --paths "src/**/*.rs" \
  --node-type struct-literal --name IRCtx \
  --format locations

# 2. Preview
rs-hack add-struct-field \
  --paths "src/**/*.rs" \
  --struct-name IRCtx \
  --field "return_type: Option<Type>" \
  --literal-default "None" \
  --format diff

# 3. Apply
rs-hack add-struct-field \
  --paths "src/**/*.rs" \
  --struct-name IRCtx \
  --field "return_type: Option<Type>" \
  --literal-default "None" \
  --apply

# 4. Verify
cargo check
```

### Workflow 3: Remove Struct Field Everywhere

```bash
# Remove field from BOTH definitions AND all literal expressions
# This is what confused users - remove-struct-field does both automatically!

# 1. Inspect where field is used
rs-hack inspect --paths "src/**/*.rs" \
  --node-type struct-literal --name Config \
  --content-filter "debug_mode" \
  --format locations

# 2. Preview removal (removes from definition AND literals)
rs-hack remove-struct-field \
  --paths "src/**/*.rs" \
  --struct-name Config \
  --field-name debug_mode \
  --format diff

# 3. Apply
rs-hack remove-struct-field \
  --paths "src/**/*.rs" \
  --struct-name Config \
  --field-name debug_mode \
  --apply

# For enum variant fields, use EnumName::VariantName syntax:
rs-hack remove-struct-field \
  --paths "src/**/*.rs" \
  --struct-name "View::Rectangle" \
  --field-name immediate_mode \
  --apply
```

### Workflow 4: Clean Up Debug Code

```bash
# 1. Find debug macros
rs-hack inspect --paths "src/**/*.rs" \
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
- Renaming enum variants across multiple files
- Adding fields to struct definitions and literals
- Removing fields from struct definitions AND literals (both happen automatically!)
- Removing fields from enum variant fields (use `EnumName::VariantName` syntax)
- Adding match arms for enum variants
- Commenting out debug macros
- Any bulk AST-level transformation
- Multi-file refactoring (glob patterns)

❌ **DON'T use rs-hack for:**
- Single-line edits in one file → use Edit tool
- Simple text replacements → use Edit tool
- Non-Rust files
- Changes requiring semantic/type analysis

## Key Flags

```bash
--paths "pattern"      # File/dir/glob: "src/**/*.rs"
--format diff          # Preview as git diff
--summary              # Show stats (files/lines changed)
--apply                # Actually modify (dry-run by default)
--where "filter"       # Filter by traits: "derives_trait:Clone"
```

## Best Practices

1. **Always inspect first** - Know what you're changing
2. **Always dry-run** - Preview with `--format diff`
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

**When in doubt:** `inspect` → preview → `apply` → `cargo check` → revert if needed
