# rust-ast-edit Quick Reference

Ultra-concise command reference for AI agents.

## Command Format

```bash
rust-ast-edit <COMMAND> [OPTIONS] --apply
```

**Note:** Omit `--apply` for dry-run (preview only).

## Struct Commands

```bash
# Add field (skips if exists)
rust-ast-edit add-struct-field \
  --path FILE --struct-name NAME --field "name: Type" [--position POS] --apply

# Update field (must exist)
rust-ast-edit update-struct-field \
  --path FILE --struct-name NAME --field "name: NewType" --apply

# Remove field
rust-ast-edit remove-struct-field \
  --path FILE --struct-name NAME --field-name name --apply
```

## Enum Commands

```bash
# Add variant (skips if exists)
rust-ast-edit add-enum-variant \
  --path FILE --enum-name NAME --variant "Variant" [--position POS] --apply

# Update variant (must exist)
rust-ast-edit update-enum-variant \
  --path FILE --enum-name NAME --variant "Variant { field: Type }" --apply

# Remove variant
rust-ast-edit remove-enum-variant \
  --path FILE --enum-name NAME --variant-name Variant --apply
```

## Match Arm Commands

```bash
# Add match arm (skips if exists)
rust-ast-edit add-match-arm \
  --path FILE --pattern "Enum::Variant" --body "expr" [--function NAME] --apply

# Update match arm (must exist)
rust-ast-edit update-match-arm \
  --path FILE --pattern "Enum::Variant" --body "new_expr" [--function NAME] --apply

# Remove match arm
rust-ast-edit remove-match-arm \
  --path FILE --pattern "Enum::Variant" [--function NAME] --apply
```

**Note:** Match operations regenerate code, which may alter formatting. Run `rustfmt` afterward.

## Common Options

```bash
--path FILE|DIR        # Target file or directory
--output FILE          # Write to different file (non-destructive)
--position POS         # first | last | after:name | before:name
--apply                # Actually modify (default is dry-run)
```

## Field Type Examples

```rust
// Primitives
field: u32, field: i64, field: bool, field: f64

// Strings
name: String, label: &str, text: &'static str

// Optional
email: Option<String>, age: Option<u32>

// Collections
tags: Vec<String>, meta: HashMap<String, Value>

// Result
result: Result<T, E>

// Smart pointers
data: Arc<Mutex<State>>, cache: Box<HashMap<K, V>>

// Visibility
field: Type              // private
pub field: Type          // public
pub(crate) field: Type   // crate-visible
```

## Variant Examples

```rust
// Unit
Pending

// Tuple
Error(String), Value(i32, String)

// Struct
User { id: u64, name: String }
Complete { timestamp: u64, data: Vec<u8> }
```

## Common Workflows

```bash
# Add field
rust-ast-edit add-struct-field --path src/user.rs --struct-name User \
  --field "email: String" --apply

# Make field public
rust-ast-edit update-struct-field --path src/user.rs --struct-name User \
  --field "pub email: String" --apply

# Make field optional
rust-ast-edit update-struct-field --path src/user.rs --struct-name User \
  --field "email: Option<String>" --apply

# Remove field
rust-ast-edit remove-struct-field --path src/user.rs --struct-name User \
  --field-name email --apply

# Add enum variant
rust-ast-edit add-enum-variant --path src/types.rs --enum-name Status \
  --variant "Archived" --apply

# Add fields to variant
rust-ast-edit update-enum-variant --path src/types.rs --enum-name Status \
  --variant "Draft { created_at: u64 }" --apply
```

## Testing Pattern

```bash
# 1. Preview
rust-ast-edit COMMAND [options]

# 2. Test on copy
rust-ast-edit COMMAND [options] --output /tmp/test.rs --apply

# 3. Apply
rust-ast-edit COMMAND [options] --apply
```

## Operation Semantics

| Command | If Exists | If Not Exists |
|---------|-----------|---------------|
| `add-*` | Skip (OK) | Create (OK) |
| `update-*` | Update (OK) | Error |
| `remove-*` | Remove (OK) | Error |

## Exit Codes

- `0` - Success
- `Non-zero` - Error (check stderr)

## Files

- `README.md` - Full documentation
- `PATTERNS.md` - Rust patterns guide (for learning)
- `QUICKREF.md` - This file (for quick lookup)
- `tests/integration_test.sh` - Test suite

---

**Remember:**
- Default is **dry-run** (safe)
- Use `--apply` to actually modify
- Use `--output` to test first
- `add-*` operations are idempotent (safe to retry)
