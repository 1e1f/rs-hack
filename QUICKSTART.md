# rs-hack Quick Start

**rs-hack** is now installed system-wide at `/Users/leif/.cargo/bin/rs-hack` ✅

## Installation

Already installed! To reinstall or update:

```bash
cd /Users/leif/ss/noisetable/crates/util/rust-ast-edit
cargo install --path . --force
```

## Quick Examples

### Add Derive Macros
```bash
rs-hack add-derive \
  --path src/models.rs \
  --target-type struct \
  --name User \
  --derives "Clone,Debug,Serialize" \
  --apply
```

### Add Method to Impl Block
```bash
rs-hack add-impl-method \
  --path src/user.rs \
  --target User \
  --method 'pub fn get_id(&self) -> u64 { self.id }' \
  --apply
```

### Add Use Statement
```bash
rs-hack add-use \
  --path src/lib.rs \
  --use-path "serde::Serialize" \
  --apply
```

### Add Struct Field
```bash
rs-hack add-struct-field \
  --path src/models.rs \
  --struct-name User \
  --field "email: String" \
  --apply
```

### Add Enum Variant
```bash
rs-hack add-enum-variant \
  --path src/types.rs \
  --enum-name Status \
  --variant "Archived" \
  --apply
```

### Add Match Arm
```bash
rs-hack add-match-arm \
  --path src/handler.rs \
  --pattern "Status::Archived" \
  --body '"archived".to_string()' \
  --function handle_status \
  --apply
```

## Available Commands

Run `rs-hack --help` to see all 15 commands:

- **Struct operations**: add/update/remove fields
- **Enum operations**: add/update/remove variants
- **Match operations**: add/update/remove arms
- **Derive macros**: add derives to structs/enums
- **Impl methods**: add methods to impl blocks
- **Use statements**: add imports
- **Batch operations**: apply multiple changes from JSON
- **Find**: locate AST nodes

## Usage with Claude Code

Claude Code can now use `rs-hack` directly! Just ask Claude to use it:

> "Use rs-hack to add Clone and Debug derives to the User struct"

> "Use rs-hack to add a get_name method to the User impl block"

## Publishing to crates.io

See [PUBLISHING_GUIDE.md](PUBLISHING_GUIDE.md) for instructions on publishing to crates.io.

Once published, anyone can install with:
```bash
cargo install rs-hack
```

## Next Steps

- Run tests: `./tests/integration_test.sh`
- Update README: Add new operations documentation
- Publish to crates.io: Follow PUBLISHING_GUIDE.md
