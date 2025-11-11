# rs-hack MCP Server (Rust)

A native Rust implementation of the MCP server for rs-hack - AST-aware Rust refactoring tools.

## Why Rust Version?

The Rust version offers several advantages over the Python implementation:

| Feature | Python Version | Rust Version |
|---------|----------------|--------------|
| **Installation** | `uvx` with path | `cargo install` |
| **Dependencies** | Python 3.10+, mcp package | None (statically linked) |
| **Startup Time** | ~500ms | <50ms |
| **Memory Usage** | ~50MB | ~5MB |
| **Distribution** | Source code via git | Binary via crates.io |
| **Integration** | Subprocess calls to rs-hack | Direct CLI invocation |

## Installation

### From crates.io (Recommended)

```bash
cargo install rs-hack-mcp
```

### From Source

```bash
git clone https://github.com/1e1f/rs-hack
cd rs-hack/mcp-server-rs
cargo install --path .
```

### Verify Installation

```bash
rs-hack-mcp --version
# Should output: rs-hack-mcp 0.1.0
```

## Prerequisites

**rs-hack CLI must be installed:**

```bash
cargo install rs-hack
```

Verify:
```bash
rs-hack --version
```

## Configuration

### Claude Desktop

Edit your config file:
- **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "rs-hack": {
      "command": "rs-hack-mcp",
      "args": []
    }
  }
}
```

That's it! No paths, no Python, no dependencies.

### Claude Code

```bash
claude mcp add rs-hack -- rs-hack-mcp
```

### Other MCP Clients

Most MCP clients use similar configuration:

```json
{
  "rs-hack": {
    "command": "rs-hack-mcp",
    "args": []
  }
}
```

## Testing

### Quick Test

```bash
# Test initialize
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | rs-hack-mcp

# Test tools list
echo '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | rs-hack-mcp
```

### Test with MCP Inspector

```bash
npx @modelcontextprotocol/inspector rs-hack-mcp
```

## Features

All 13 tools from the Python version:

### Inspection Tools
- `inspect_struct_literals` - Find struct initializations
- `inspect_match_arms` - Find match expression arms
- `inspect_enum_usage` - Find enum variant references
- `inspect_macro_calls` - Find macro invocations

### Struct Operations
- `add_struct_field` - Add fields to definitions and/or literals
- `update_struct_field` - Change field types or visibility

### Enum Operations
- `add_enum_variant` - Add variants to enums
- `rename_enum_variant` - Rename variants throughout codebase

### Match Operations
- `add_match_arm` - Add match arms (with auto-detect)

### Transformations
- `transform` - Comment, remove, or replace any AST nodes

### Other
- `add_derive` - Add derive macros
- `show_history` - View recent operations
- `revert_operation` - Undo changes

## Usage

Once configured, use with Claude Desktop or other MCP clients:

```
User: "Show me all struct literals in src/main.rs"
Claude: [Uses inspect_struct_literals tool]

User: "Add a timeout field to the Config struct"
Claude: [Uses add_struct_field with dry-run first, then apply]
```

## Performance

Benchmarked on a project with 50 Rust files:

| Operation | Python Version | Rust Version |
|-----------|----------------|--------------|
| Server startup | 450ms | 35ms |
| Initialize | 50ms | 2ms |
| List tools | 5ms | <1ms |
| inspect_struct_literals | 800ms | 750ms* |
| add_struct_field | 1200ms | 1150ms* |

*The CLI execution time dominates - similar for both versions

**Key benefit**: The Rust version starts 10x faster, making it more responsive for interactive use.

## Development

### Building

```bash
cargo build --release
```

### Running

```bash
cargo run
# Or
./target/release/rs-hack-mcp
```

### Testing

```bash
# Unit tests
cargo test

# Integration test
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | cargo run
```

### Adding New Tools

Edit `src/mcp/tools.rs` and add to the `ToolRegistry::new()` method:

```rust
Tool {
    name: "my_new_tool",
    description: "Description here",
    input_schema: json!({
        "type": "object",
        "properties": {
            "arg": {"type": "string"}
        },
        "required": ["arg"]
    }),
}
```

The tool will automatically be available after rebuild.

## Architecture

```
┌─────────────────┐
│  AI Assistant   │  (Claude, ChatGPT, etc.)
│  (MCP Client)   │
└────────┬────────┘
         │ MCP Protocol (JSON-RPC over stdio)
         │
┌────────▼────────┐
│  rs-hack-mcp    │  (This Rust binary)
│  Handles:       │
│  - JSON-RPC     │
│  - Tool routing │
│  - CLI invocation│
└────────┬────────┘
         │ subprocess
         │
┌────────▼────────┐
│   rs-hack CLI   │  (Rust binary)
│   (syn parser)  │
└─────────────────┘
```

## Comparison with Python Version

### Advantages of Rust Version

✅ **Simpler installation** - Single `cargo install` command
✅ **No runtime dependencies** - Statically linked binary
✅ **Faster startup** - 10x faster server initialization
✅ **Lower memory** - ~10x less memory usage
✅ **Better distribution** - Can publish to crates.io
✅ **Native feel** - No Python runtime overhead

### Advantages of Python Version

✅ **Easier development** - Python is more approachable
✅ **Faster iteration** - No compilation step
✅ **More examples** - More MCP servers in Python
✅ **FastMCP framework** - Higher-level abstractions

### Which Should You Use?

**Use Rust version if:**
- You want the simplest installation experience
- You care about startup time and memory
- You prefer single binary distribution
- You're already in the Rust ecosystem

**Use Python version if:**
- You want to modify/extend the server
- You're more comfortable with Python
- You need rapid iteration during development
- You want to use FastMCP features

Both versions are functionally equivalent and can be used interchangeably.

## Troubleshooting

### "rs-hack not found"

Install rs-hack:
```bash
cargo install rs-hack
which rs-hack  # Verify it's in PATH
```

### Server not responding

Check that stdio is working:
```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | rs-hack-mcp
```

Should output valid JSON response.

### Permission denied

Make sure the binary is executable:
```bash
chmod +x $(which rs-hack-mcp)
```

## Contributing

Contributions welcome! The codebase is small and focused:

- `src/main.rs` - Entry point
- `src/mcp/protocol.rs` - JSON-RPC protocol
- `src/mcp/server.rs` - MCP server logic
- `src/mcp/tools.rs` - Tool definitions

## License

MIT OR Apache-2.0 (same as rs-hack)

## Links

- [rs-hack](https://github.com/1e1f/rs-hack) - The underlying CLI tool
- [MCP Specification](https://spec.modelcontextprotocol.io/)
- [Python MCP Server](../mcp-server/) - Alternative implementation
