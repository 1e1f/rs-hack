# rs-hack MCP Server (Rust)

A native Rust implementation of the MCP server for rs-hack - AST-aware Rust refactoring tools.

## Why Rust?

This native Rust implementation provides excellent performance and ease of use:

- **Simple Installation**: Single `cargo install` command
- **Zero Dependencies**: Statically linked binary
- **Fast Startup**: <50ms server initialization
- **Low Memory**: ~5MB memory footprint
- **Easy Distribution**: Binary available via crates.io
- **Direct Integration**: Efficient CLI invocation

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

That's it! No paths, no dependencies.

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

All 24 rs-hack CLI commands exposed as MCP tools:

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

| Operation | Time |
|-----------|------|
| Server startup | 35ms |
| Initialize | 2ms |
| List tools | <1ms |
| inspect (struct literals) | 750ms* |
| add_struct_field | 1150ms* |

*CLI execution time dominates these operations

**Key benefit**: Fast startup makes the server highly responsive for interactive use.

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

## Benefits

✅ **Simpler installation** - Single `cargo install` command
✅ **No runtime dependencies** - Statically linked binary
✅ **Fast startup** - Server initializes in <50ms
✅ **Low memory usage** - ~5MB memory footprint
✅ **Easy distribution** - Published to crates.io
✅ **Native performance** - No runtime overhead
✅ **Complete coverage** - All 24 rs-hack commands available

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
