# rs-hack Dogfooding: Quick Findings Summary

## What I Did
Analyzed rs-hack's own codebase (~6000 LOC) to identify refactoring opportunities and stress-test the tool's capabilities.

## Key Findings

### 🔴 Critical Issues

1. **No bulk operations**: Adding `PartialEq` to 15 structs requires 15 separate commands
   - **Impact**: Tedious and error-prone for real-world refactoring
   - **Solution**: Add wildcard pattern matching: `--name "*Op"`

2. **Enum updates break code**: Updating enum variants doesn't update usage sites
   - **Impact**: Produces non-compiling code
   - **Solution**: Add `--check-compile` flag or semantic rename support

3. **No semantic refactoring**: Cannot rename with usage updates
   - **Impact**: Limited usefulness for complex refactoring
   - **Solution**: Integrate with rust-analyzer

### 🟡 Significant Shortcomings

1. **No expression normalization**: Cannot standardize `vec![]` vs `Vec::new()`
2. **Limited pattern matching**: Only basic `--where "derives_trait:X"` support
3. **Verbose CLI**: Too many flags and repetitive commands
4. **No dead code detection**: Cannot identify or remove unused items
5. **String-based matching**: Should use AST equality instead
6. **No batch optimization**: 15 sequential commands is slow

### 🟢 Opportunities Discovered

1. **Type-aware filtering**: `--where "all_fields:Clone"` would enable safe bulk derives
2. **Interactive TUI mode**: Better UX for exploring and selecting targets
3. **Doc comment generation**: Auto-generate from struct/field names
4. **Extract common patterns**: Detect repeated fields, suggest trait extraction
5. **AI-assisted analysis**: LLM-powered refactoring suggestions
6. **Query language**: More concise than flags: `rs-hack query "struct(*Op) in src/"`

### ⚠️ Footguns Found

1. **Glob patterns** can accidentally include `target/` or vendored code
2. **Idempotency edge cases** with partial field presence
3. **State directory confusion** with 3 different modes
4. **Pattern matching ambiguity** despite normalization
5. **Silent failures** when operations skip targets

## Refactoring Opportunities in rs-hack Itself

Found **12 concrete improvements** rs-hack could make to its own codebase:

1. ✅ Add `PartialEq` derives to 15+ operation structs
2. ✅ Add `Eq` derives where appropriate
3. ✅ Add `Default` derives to structs with `new()` methods
4. ⚠️ Remove dead `visitor.rs` module (275 LOC unused)
5. ⚠️ Extract common fields from 5 visitor structs into trait
6. ⚠️ Standardize error handling patterns
7. ⚠️ Fix cloning in hot paths (performance)
8. ⚠️ Make struct field visibility consistent
9. ⚠️ Normalize expression styles (`vec![]` vs `Vec::new()`)
10. ⚠️ Add doc comments to public structs
11. ⚠️ Reduce string-based pattern matching
12. ⚠️ Fix enum variant data inconsistencies

Legend: ✅ = rs-hack can do, ⚠️ = rs-hack cannot do (yet)

## Most Impactful Improvements

If rs-hack added these 4 features, it would **10x its usefulness**:

1. **Wildcard pattern matching**: `--name "*Op"` for bulk operations
2. **Semantic rename**: Rename with automatic usage updates
3. **Type-aware filtering**: `--where "all_fields:Clone"` for safe derives
4. **Batch optimization**: Single AST pass for multiple operations

## Real-World Example

**Current workflow** (adding PartialEq to all Op structs):
```bash
# 15 commands, ~5 minutes
rs-hack add-derive --path src/operations.rs --target-type struct --name AddStructFieldOp --derives "PartialEq" --apply
rs-hack add-derive --path src/operations.rs --target-type struct --name UpdateStructFieldOp --derives "PartialEq" --apply
# ... 13 more times
```

**Proposed workflow**:
```bash
# 1 command, ~10 seconds
rs-hack add-derive --path "src/**/*.rs" --target-type struct --name "*Op" --derives "PartialEq,Eq" --apply
```

**Time saved**: 95% reduction in effort

## Bottom Line

rs-hack is **excellent** for:
- ✅ Simple, structural, repetitive refactoring
- ✅ Adding derives/fields/variants in bulk (with manual scripting)
- ✅ Safe, reviewable changes (dry-run + revert)
- ✅ AI agent automation

rs-hack **struggles** with:
- ❌ Complex, semantic refactoring (renames, ownership changes)
- ❌ One-off, context-dependent changes
- ❌ Performance-oriented refactoring
- ❌ Bulk operations without scripting

**Verdict**: Solid foundation, but needs 3-5 key features to become indispensable for real-world Rust refactoring.

## See Also

- Full analysis: [REFACTORING_ANALYSIS.md](./REFACTORING_ANALYSIS.md)
- Original README: [README.md](./README.md)
