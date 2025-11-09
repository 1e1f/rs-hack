# rs-hack MCP Server

An MCP (Model Context Protocol) server that exposes [rs-hack](https://github.com/1e1f/rs-hack)'s powerful AST-aware Rust refactoring tools to AI assistants like Claude, ChatGPT, and others.

## What This Provides

This MCP server makes rs-hack's capabilities available through a standardized protocol, enabling:

- **One-click integration** with Claude Desktop, Claude Code, Cursor, Cline, Windsurf, etc.
- **Automatic tool discovery** - AI assistants see all available operations
- **Better UX** - Preview changes before applying, structured error messages
- **Composability** - Run alongside other MCP servers (like Serena for multi-language support)

## Features

### Inspection Tools (Explore Code)
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
- `add_match_arm` - Add match arms (with auto-detect for missing variants)

### Generic Transformations
- `transform` - Comment, remove, or replace any AST nodes

### Other Operations
- `add_derive` - Add derive macros
- `show_history` - View recent operations
- `revert_operation` - Undo changes

## Prerequisites

1. **rs-hack CLI tool** must be installed and available in PATH:
   ```bash
   cargo install rs-hack
   ```

2. **Python 3.10+** and **uv** (recommended) or pip

## Installation

### Using uvx (Recommended - No Local Installation)

You can run the MCP server directly without installation:

```bash
uvx --from . server.py
```

### Local Installation with uv

1. Clone this repository:
   ```bash
   git clone <your-repo-url>
   cd rs-hack-mcp
   ```

2. Install dependencies:
   ```bash
   uv sync
   ```

3. Run the server:
   ```bash
   uv run server.py
   ```

### Using pip

```bash
pip install mcp
python server.py
```

## Configuration

### Claude Desktop

Edit your Claude Desktop config file:
- **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

Add the rs-hack MCP server:

```json
{
  "mcpServers": {
    "rs-hack": {
      "command": "uvx",
      "args": [
        "--from",
        "/absolute/path/to/rs-hack-mcp",
        "server.py"
      ]
    }
  }
}
```

Or if you want to run directly from a git repository:

```json
{
  "mcpServers": {
    "rs-hack": {
      "command": "uvx",
      "args": [
        "--from",
        "git+https://github.com/YOUR_USERNAME/rs-hack-mcp",
        "server.py"
      ]
    }
  }
}
```

Restart Claude Desktop and you should see rs-hack tools available!

### Claude Code

From your Rust project directory:

```bash
claude mcp add rs-hack -- uvx --from /path/to/rs-hack-mcp server.py
```

Or directly from git:

```bash
claude mcp add rs-hack -- uvx --from git+https://github.com/YOUR_USERNAME/rs-hack-mcp server.py
```

### Cursor / Cline / Other MCP Clients

Most MCP clients use similar configuration. Add to your MCP settings:

```json
{
  "rs-hack": {
    "command": "uvx",
    "args": ["--from", "/path/to/rs-hack-mcp", "server.py"]
  }
}
```

### Streamable HTTP Mode (For Web Clients)

Run the server in HTTP mode:

```bash
uv run server.py --transport streamable-http --port 9000
```

Then connect your client to `http://localhost:9000/mcp`

## Usage Examples

### Example 1: Add a Field to Structs

```
User: "Add an 'updated_at: DateTime<Utc>' field to the User struct"

Claude: I'll add that field to the User struct. Let me first preview the change.
[Calls add_struct_field with apply=False]

Claude: Here's what will change. Should I apply it?

User: "Yes"

Claude: Applied successfully!
[Calls add_struct_field with apply=True]
```

### Example 2: Find and Remove Debug Logs

```
User: "Find all eprintln! macros with [DEBUG] and comment them out"

Claude: Let me first find them...
[Calls inspect_macro_calls]

Claude: I found 15 debug print statements. Should I comment them out?

User: "Yes"

Claude: Done!
[Calls transform with action="comment"]
```

### Example 3: Rename an Enum Variant

```
User: "Rename the Status::Draft variant to Status::Pending throughout the codebase"

Claude: I'll rename it across all files. This will update:
- The enum definition
- All match arms
- All constructor calls
- All references

[Calls rename_enum_variant with apply=True]

Claude: Successfully renamed across 12 files!
```

### Example 4: Auto-Complete Match Arms

```
User: "Add match arms for all missing Status variants in handle_status"

Claude: I'll auto-detect the missing variants and add them.
[Calls add_match_arm with auto_detect=True]

Claude: Added 3 missing variants with todo!() placeholders.
```

## Key Features

### Dry-Run by Default
All operations preview changes by default. Set `apply=True` to make actual changes.

### Glob Pattern Support
Most operations support glob patterns:
```python
# Operate on all Rust files in src/
add_struct_field("src/**/*.rs", ...)
```

### Revert Operations
Every operation gets a unique run ID. You can undo changes:
```python
show_history()  # See recent operations
revert_operation("a05a626")  # Undo specific change
```

### Idempotent Operations
Safe to run multiple times - rs-hack checks if changes already exist.

## Workflow Recommendations

1. **Explore First**: Use `inspect_*` tools to understand the codebase
2. **Preview Changes**: Run operations without `apply=True` first
3. **Apply Changes**: Set `apply=True` after reviewing preview
4. **Check History**: Use `show_history()` to track operations
5. **Revert if Needed**: Use `revert_operation()` to undo mistakes

## Using with Serena

rs-hack and Serena complement each other beautifully:

- **Serena**: Multi-language navigation, LSP-based understanding
- **rs-hack**: Rust-specific precision operations

Example workflow:
```
1. Use Serena to navigate: "Find all API handler functions"
2. Use rs-hack for Rust ops: "Add Serialize derive to all response structs"
```

Configure both in Claude Desktop:
```json
{
  "mcpServers": {
    "serena": {
      "command": "uvx",
      "args": ["--from", "git+https://github.com/oraios/serena", "serena", "start-mcp-server"]
    },
    "rs-hack": {
      "command": "uvx", 
      "args": ["--from", "/path/to/rs-hack-mcp", "server.py"]
    }
  }
}
```

## Troubleshooting

### "rs-hack not found"
Install rs-hack: `cargo install rs-hack`

### Changes Not Applied
Make sure you set `apply=True` in the tool call. By default, operations only preview.

### Permission Errors
Ensure the MCP server has write access to your project directory.

### MCP Server Not Appearing
- Restart your MCP client after config changes
- Check client logs for errors
- Verify the command path is absolute

## Development

### Running Tests
```bash
uv run pytest
```

### Adding New Tools
Edit `server.py` and add new `@mcp.tool()` decorated functions.

### Local Testing
Use the MCP Inspector:
```bash
uv run mcp dev server.py
```

## Architecture

```
┌─────────────────┐
│  AI Assistant   │  (Claude, ChatGPT, etc.)
│  (MCP Client)   │
└────────┬────────┘
         │ MCP Protocol
         │
┌────────▼────────┐
│  rs-hack-mcp    │  (This server)
│  server.py      │
└────────┬────────┘
         │ subprocess
         │
┌────────▼────────┐
│   rs-hack CLI   │  (Rust binary)
│   (syn parser)  │
└─────────────────┘
```

## Contributing

Contributions welcome! Please:
1. Keep tools focused and well-documented
2. Add type hints and docstrings
3. Test with at least Claude Desktop
4. Update this README for new features

## License

MIT OR Apache-2.0 (same as rs-hack)

## Links

- [rs-hack](https://github.com/1e1f/rs-hack) - The underlying CLI tool
- [MCP](https://modelcontextprotocol.io/) - Model Context Protocol docs
- [Serena](https://github.com/oraios/serena) - Complementary multi-language MCP server
