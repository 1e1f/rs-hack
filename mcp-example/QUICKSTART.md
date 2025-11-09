# Quick Start Guide

Get rs-hack working with Claude in 5 minutes!

## Prerequisites

```bash
# Install rs-hack
cargo install rs-hack

# Verify installation
rs-hack --version
```

## Installation

### Option 1: Run Directly (Easiest)

No installation needed! Just configure your client to run:

```bash
uvx --from /path/to/rs-hack-mcp server.py
```

### Option 2: Local Development

```bash
git clone <this-repo>
cd rs-hack-mcp
uv sync
```

## Configuration

### Claude Desktop (Most Common)

1. Open config file:
   - **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
   - **Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

2. Add this:

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

3. Restart Claude Desktop

### Claude Code

From your Rust project:

```bash
claude mcp add rs-hack -- uvx --from /path/to/rs-hack-mcp server.py
```

## Verify It Works

After restart, try asking Claude:

```
"List all struct literals in src/main.rs"
```

You should see Claude using the `inspect_struct_literals` tool!

## First Real Use

Try this workflow:

```
You: "Add an 'updated_at: DateTime<Utc>' field to the User struct"

Claude will:
1. Preview the change (dry-run)
2. Show you the diff
3. Ask permission
4. Apply it when you approve
```

## Common Operations

### Add a struct field
```
"Add field X to struct Y"
```

### Rename an enum variant  
```
"Rename Status::Draft to Status::Pending everywhere"
```

### Find debug code
```
"Find all eprintln macros with [DEBUG]"
```

### Add missing match arms
```
"Add match arms for all missing Status variants"
```

### Add derives
```
"Add Serialize and Deserialize to all Response structs"
```

## Key Tips

1. **Always preview first** - All operations are dry-run by default
2. **Use glob patterns** - `"src/**/*.rs"` for bulk operations  
3. **Inspect before acting** - Use inspect tools to explore first
4. **Revert mistakes** - Use show_history and revert_operation

## Troubleshooting

### "rs-hack not found"
```bash
cargo install rs-hack
```

### Server not appearing
- Check you used absolute paths in config
- Restart your MCP client
- Check client logs

### Changes not applied
- Make sure you confirmed with `apply=True`
- Operations are dry-run by default

## Next Steps

- Read [EXAMPLES.md](EXAMPLES.md) for detailed workflows
- Check [CONFIGURATION.md](CONFIGURATION.md) for advanced setup
- See [README.md](README.md) for complete documentation

## Using with Serena

For maximum power, use both:

```json
{
  "mcpServers": {
    "serena": {
      "command": "uvx",
      "args": [
        "--from", "git+https://github.com/oraios/serena",
        "serena", "start-mcp-server"
      ]
    },
    "rs-hack": {
      "command": "uvx",
      "args": [
        "--from", "/path/to/rs-hack-mcp",
        "server.py"
      ]
    }
  }
}
```

**Serena** for multi-language navigation, **rs-hack** for Rust precision!

## Help

- Issues: Open on GitHub
- Questions: Check examples and docs
- Contributing: PRs welcome!

---

**That's it!** You now have AST-aware Rust refactoring available in Claude. 🎉
