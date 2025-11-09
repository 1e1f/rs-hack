# rs-hack MCP Server - Project Overview

## What We Built

A Model Context Protocol (MCP) server that wraps rs-hack's AST-aware Rust refactoring tools, making them accessible to any MCP-compatible AI assistant.

## Architecture

```
┌─────────────────────────────────────┐
│   AI Assistant (Claude, etc.)       │
│   - Natural language interface      │
│   - Tool calling capabilities       │
└──────────────┬──────────────────────┘
               │ MCP Protocol (JSON-RPC)
               │ Tool discovery & execution
┌──────────────▼──────────────────────┐
│   rs-hack MCP Server (Python)       │
│   - FastMCP framework               │
│   - 15+ tool wrappers               │
│   - Subprocess management           │
│   - Error handling & validation     │
└──────────────┬──────────────────────┘
               │ subprocess.run()
               │ Command-line interface
┌──────────────▼──────────────────────┐
│   rs-hack CLI (Rust)                │
│   - syn AST parser                  │
│   - Code transformations            │
│   - File system operations          │
└─────────────────────────────────────┘
```

## Files Created

### Core Implementation
- **server.py** (500+ lines)
  - 15 MCP tools covering all major rs-hack operations
  - Structured error handling
  - JSON/text output parsing
  - Dry-run by default with apply flag

### Documentation
- **README.md** - Complete project documentation
- **QUICKSTART.md** - 5-minute setup guide
- **EXAMPLES.md** - Real-world usage scenarios
- **CONFIGURATION.md** - Client-specific setup guides

### Supporting Files
- **pyproject.toml** - Python dependencies
- **test_server.py** - Basic test suite
- **.gitignore** - Python/MCP exclusions
- **LICENSE** - MIT/Apache dual license

## Key Features Implemented

### 1. Inspection Tools
- `inspect_struct_literals` - Find struct initializations
- `inspect_match_arms` - Find match arms
- `inspect_enum_usage` - Find enum variant usages
- `inspect_macro_calls` - Find macro invocations

### 2. Struct Operations
- `add_struct_field` - Add fields to definitions/literals
- `update_struct_field` - Change field properties

### 3. Enum Operations
- `add_enum_variant` - Add new variants
- `rename_enum_variant` - Rename throughout codebase

### 4. Match Operations
- `add_match_arm` - Add match arms (with auto-detect)

### 5. Generic Operations
- `transform` - Comment/remove/replace any AST nodes
- `add_derive` - Add derive macros

### 6. Maintenance
- `show_history` - View recent operations
- `revert_operation` - Undo changes

## Design Decisions

### 1. Dry-Run by Default
All operations require explicit `apply=True` to make changes. This:
- Prevents accidental modifications
- Allows preview before commit
- Matches user expectations from CLI

### 2. Subprocess Execution
We shell out to rs-hack CLI rather than importing as library:
- Simpler maintenance (no version coupling)
- Works with any rs-hack installation
- Clear separation of concerns

### 3. Structured Returns
Tools return dictionaries with `success` and `output`/`error`:
- Consistent error handling
- Easy to parse in different contexts
- JSON support where available

### 4. Rich Documentation
Extensive docstrings on every tool:
- LLMs can understand tool purposes
- Users can inspect tool descriptions
- Examples guide proper usage

## Integration Opportunities

### 1. With Serena (Immediate)
**Status**: Ready to use together

Run both servers:
```json
{
  "serena": {...},  // Multi-language LSP
  "rs-hack": {...}  // Rust specialist
}
```

**Workflow**:
- Serena navigates codebase
- rs-hack performs Rust-specific operations
- Each tool does what it's best at

### 2. With Other MCP Servers (Easy)
**Compatible with**:
- File system servers
- Git servers
- Test runners
- Deployment tools

**Example**: Use rs-hack + filesystem MCP + git MCP for complete workflows

### 3. Custom Extensions (Advanced)
**Add new tools** by:
1. Creating new `@mcp.tool()` functions
2. Calling rs-hack with new arguments
3. Documenting in tool docstring

**Example new tools**:
- `batch_refactor` - Multi-step operations
- `analyze_changes` - Diff analysis
- `suggest_refactorings` - Proactive recommendations

### 4. CI/CD Integration (Future)
**Potential uses**:
- Automated code reviews
- Migration scripts
- Bulk refactoring tasks
- Code quality checks

## Comparison with Alternatives

### vs. Direct CLI Usage
| Aspect | rs-hack MCP | Direct CLI |
|--------|-------------|------------|
| Setup | One config file | Per-project skills |
| Discovery | Automatic | Manual docs |
| UI | Native client UI | Terminal output |
| Composability | With other MCPs | Limited |
| Learning curve | Natural language | CLI syntax |

### vs. Serena
| Aspect | rs-hack MCP | Serena |
|--------|-------------|--------|
| Languages | Rust only | 30+ languages |
| Depth | Deep Rust semantics | Broad coverage |
| Approach | AST manipulation | LSP queries |
| Precision | Highly precise | LSP-limited |
| Speed | Fast (syn parser) | LSP-dependent |

**Best used together**: Serena for navigation, rs-hack for operations

### vs. rust-analyzer
| Aspect | rs-hack MCP | rust-analyzer |
|--------|-------------|---------------|
| Purpose | Refactoring | IDE features |
| Operations | Batch transformations | Single-file refactors |
| Precision | AST-level | Token-level |
| Automation | Full automation | IDE-driven |

## Success Metrics

### For rs-hack Project
- **Adoption**: More users discover rs-hack via MCP
- **Feedback**: Issues and feature requests from new use cases
- **Contributions**: Community helps extend MCP wrapper

### For Users
- **Time saved**: 10x faster than manual edits
- **Error reduction**: AST-awareness prevents mistakes
- **Confidence**: Preview before apply builds trust

### For Ecosystem
- **Integration**: Used alongside other MCP servers
- **Standards**: Demonstrates MCP best practices
- **Innovation**: Inspires similar tool wrappers

## Next Steps

### Immediate (1-2 days)
1. **Test with real projects** - Validate all tools work
2. **Publish to GitHub** - Make available publicly
3. **Document edge cases** - Add troubleshooting guide

### Short-term (1-2 weeks)
1. **Add remaining rs-hack operations**
   - `add_use` statements
   - `add_impl_method`
   - `remove_*` operations
   
2. **Improve error messages**
   - Parse rs-hack errors
   - Provide helpful suggestions
   
3. **Add integration tests**
   - Test against real Rust projects
   - Verify all tools work end-to-end

### Medium-term (1-2 months)
1. **Streamable HTTP mode**
   - For web-based clients
   - Better scalability
   
2. **Caching and optimization**
   - Avoid repeated parsing
   - Batch operations
   
3. **Advanced features**
   - Multi-step refactorings
   - Code analysis tools
   - Suggestion system

### Long-term (3+ months)
1. **Web UI dashboard**
   - Visual diff preview
   - Operation history browser
   - Batch operation builder
   
2. **Plugin system**
   - Custom tool registration
   - User-defined refactorings
   
3. **Integration with rust-analyzer**
   - Use semantic info for smarter operations
   - Combine strengths of both

## Marketing & Communication

### Target Audiences
1. **Current rs-hack users** - "Easier integration with AI assistants"
2. **Claude users** - "Powerful Rust refactoring for Claude"
3. **Rust developers** - "AI-powered refactoring tools"
4. **MCP community** - "Example of tool wrapping"

### Key Messages
- "rs-hack + Claude = AST-aware Rust refactoring via natural language"
- "One config file, 15+ powerful tools"
- "Preview before apply - safe by default"
- "Works alongside Serena for multi-language projects"

### Launch Plan
1. **Announce on rs-hack repo** - Primary audience
2. **Post to /r/rust** - Rust community
3. **Share in MCP Discord** - Protocol community
4. **Demo video** - Show it working with Claude

## Technical Challenges & Solutions

### Challenge 1: subprocess.run() overhead
**Solution**: For now, acceptable. Future: persistent rs-hack process

### Challenge 2: Path handling across OSes
**Solution**: Use pathlib, document absolute paths required

### Challenge 3: Error message clarity
**Solution**: Parse rs-hack output, provide context

### Challenge 4: Large file handling
**Solution**: rs-hack uses streaming, MCP chunking for output

## Maintenance Plan

### Regular Updates
- Keep dependencies updated
- Track rs-hack releases
- Update tools when CLI changes

### Community Management
- Respond to issues within 48h
- Accept PRs with tests
- Maintain documentation

### Quality Assurance
- Run tests before releases
- Verify with multiple clients
- Test on different OSes

## Conclusion

This MCP server successfully bridges rs-hack's powerful CLI with the MCP ecosystem, enabling:

1. **Accessibility** - Natural language interface
2. **Discovery** - Automatic tool exposure
3. **Safety** - Preview-before-apply workflow
4. **Composability** - Works with other MCP servers
5. **Maintainability** - Clean separation from rs-hack core

**The opportunity**: This positions rs-hack as the "Rust specialist" tool that complements general coding assistants, potentially driving significant adoption.

**Next action**: Test, polish, and release publicly!
