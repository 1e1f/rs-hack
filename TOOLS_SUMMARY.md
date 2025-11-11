# Comprehensive rs-hack MCP Tool Coverage

## Total: 24 Tools (Full Coverage of rs-hack CLI)

### Inspection (2 tools)
- **`inspect`** - Generic inspection for all 8 node types
  - Supports: struct-literal, match-arm, enum-usage, function-call, method-call, macro-call, identifier, type-ref
  - Replaces 4 specialized tools with one flexible tool
- **`find`** - Find definitions (struct, enum, function)

### Struct Operations (3 tools)
- **`add_struct_field`** - Add fields to definitions and/or literals
- **`update_struct_field`** - Change field type or visibility  
- **`remove_struct_field`** - Remove field from struct

### Enum Operations (4 tools)
- **`add_enum_variant`** - Add variant to enum
- **`update_enum_variant`** - Update existing variant
- **`remove_enum_variant`** - Remove variant from enum
- **`rename_enum_variant`** - Rename variant across codebase

### Match Operations (3 tools)
- **`add_match_arm`** - Add match arm (with auto-detect)
- **`update_match_arm`** - Update existing match arm
- **`remove_match_arm`** - Remove match arm

### Transform (1 tool)
- **`transform`** - Generic find-and-modify for any AST node

### Code Organization (3 tools)
- **`add_derive`** - Add derive macros
- **`add_impl_method`** - Add method to impl block
- **`add_use`** - Add use statement

### Documentation (3 tools)
- **`add_doc_comment`** - Add documentation
- **`update_doc_comment`** - Update documentation
- **`remove_doc_comment`** - Remove documentation

### Refactoring (1 tool)
- **`rename_function`** - Rename function across codebase

### Batch & Utility (4 tools)
- **`batch`** - Run multiple operations from spec file
- **`history`** - Show operation history
- **`revert`** - Undo operation by run ID
- **`clean`** - Clean old state data

## Changes from Original 13 Tools

### Removed (4 specialized inspect wrappers)
- ❌ `inspect_struct_literals`
- ❌ `inspect_match_arms`
- ❌ `inspect_enum_usage`
- ❌ `inspect_macro_calls`

### Replaced With
- ✅ **`inspect`** - One generic tool with `node_type` parameter

### Added (11 new tools for full coverage)
- ✅ `find`
- ✅ `remove_struct_field`
- ✅ `update_enum_variant`
- ✅ `remove_enum_variant`
- ✅ `update_match_arm`
- ✅ `remove_match_arm`
- ✅ `add_impl_method`
- ✅ `add_use`
- ✅ `add_doc_comment`
- ✅ `update_doc_comment`
- ✅ `remove_doc_comment`
- ✅ `rename_function`
- ✅ `batch`
- ✅ `clean`

## Coverage Summary

| rs-hack Command | MCP Tool | Status |
|-----------------|----------|--------|
| `inspect` | `inspect` | ✅ Full |
| `find` | `find` | ✅ Full |
| `transform` | `transform` | ✅ Full |
| `add-struct-field` | `add_struct_field` | ✅ Full |
| `update-struct-field` | `update_struct_field` | ✅ Full |
| `remove-struct-field` | `remove_struct_field` | ✅ Full |
| `add-enum-variant` | `add_enum_variant` | ✅ Full |
| `update-enum-variant` | `update_enum_variant` | ✅ Full |
| `remove-enum-variant` | `remove_enum_variant` | ✅ Full |
| `rename-enum-variant` | `rename_enum_variant` | ✅ Full |
| `add-match-arm` | `add_match_arm` | ✅ Full |
| `update-match-arm` | `update_match_arm` | ✅ Full |
| `remove-match-arm` | `remove_match_arm` | ✅ Full |
| `add-derive` | `add_derive` | ✅ Full |
| `add-impl-method` | `add_impl_method` | ✅ Full |
| `add-use` | `add_use` | ✅ Full |
| `add-doc-comment` | `add_doc_comment` | ✅ Full |
| `update-doc-comment` | `update_doc_comment` | ✅ Full |
| `remove-doc-comment` | `remove_doc_comment` | ✅ Full |
| `rename-function` | `rename_function` | ✅ Full |
| `batch` | `batch` | ✅ Full |
| `history` | `history` | ✅ Full |
| `revert` | `revert` | ✅ Full |
| `clean` | `clean` | ✅ Full |

**Coverage: 24/24 commands (100%)**

## Benefits

1. **Complete Coverage** - All rs-hack functionality exposed
2. **Better inspect** - One flexible tool instead of 4 specialized ones
3. **More Operations** - Add/update/remove for all entity types
4. **Documentation Support** - Full doc comment management
5. **Batch Operations** - Run multiple commands from spec
6. **State Management** - Clean old state with `clean`

## Migration Guide

If you were using the old specialized inspect tools:

**Before:**
```json
{"name": "inspect_struct_literals", "arguments": {"path": "src/**/*.rs"}}
{"name": "inspect_match_arms", "arguments": {"path": "src/**/*.rs"}}
{"name": "inspect_enum_usage", "arguments": {"path": "src/**/*.rs", "name": "Status::Active"}}
{"name": "inspect_macro_calls", "arguments": {"path": "src/**/*.rs", "name": "eprintln"}}
```

**After:**
```json
{"name": "inspect", "arguments": {"paths": "src/**/*.rs", "node_type": "struct-literal"}}
{"name": "inspect", "arguments": {"paths": "src/**/*.rs", "node_type": "match-arm"}}
{"name": "inspect", "arguments": {"paths": "src/**/*.rs", "node_type": "enum-usage", "name": "Status::Active"}}
{"name": "inspect", "arguments": {"paths": "src/**/*.rs", "node_type": "macro-call", "name": "eprintln"}}
```

More flexible! Can now also inspect:
- `function-call`
- `method-call` 
- `identifier`
- `type-ref`
