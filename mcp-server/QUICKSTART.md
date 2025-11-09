# Quick Start Guide

Get up and running with rs-hack MCP server in 5 minutes!

## Step 1: Install rs-hack

```bash
cargo install rs-hack
```

Verify installation:
```bash
rs-hack --version
```

## Step 2: Configure Your MCP Client

### For Claude Desktop

1. Find your config file:
   - **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
   - **Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

2. Add this configuration:
   ```json
   {
     "mcpServers": {
       "rs-hack": {
         "command": "uvx",
         "args": [
           "--from",
           "git+https://github.com/1e1f/rs-hack#subdirectory=mcp-server",
           "server.py"
         ]
       }
     }
   }
   ```

3. Restart Claude Desktop

### For Claude Code

```bash
cd your-rust-project
claude mcp add rs-hack -- uvx --from git+https://github.com/1e1f/rs-hack#subdirectory=mcp-server server.py
```

## Step 3: Try It Out!

Open your AI assistant and try these commands:

### Example 1: Explore Your Code
```
"Show me all struct literals in src/main.rs"
```

### Example 2: Add a Field
```
"Add a 'created_at: DateTime<Utc>' field to the User struct in src/models.rs"
```

### Example 3: Find Debug Logs
```
"Find all eprintln! macros in the codebase"
```

### Example 4: Rename Enum Variant
```
"Rename Status::Draft to Status::Pending throughout the codebase"
```

## Common Workflows

### 1. Safe Refactoring Pattern
```
You: "Add a timeout field to the Config struct"
AI: [Shows preview with apply=False]
You: "Looks good, apply it"
AI: [Applies with apply=True]
```

### 2. Bulk Operations
```
You: "Add Clone derive to all structs in src/models/"
AI: [Uses glob pattern "src/models/**/*.rs"]
```

### 3. Experimentation with Revert
```
You: "Try adding Debug to all enums"
AI: [Applies, gets run_id: a05a626]
You: "Actually, revert that"
AI: [Calls revert_operation("a05a626")]
```

## Tips

1. **Always preview first** - Operations are dry-run by default
2. **Use glob patterns** - For multi-file operations use `"src/**/*.rs"`
3. **Check history** - Use `show_history()` to see recent changes
4. **Revert mistakes** - Every operation is tracked and reversible

## Troubleshooting

### Server Not Showing Up
- Restart your MCP client after config changes
- Check the client logs for errors
- Verify rs-hack is in your PATH: `which rs-hack`

### Operations Not Working
- Make sure you're in a Rust project directory
- Check that the file paths are correct
- Try `rs-hack --help` to verify the CLI works

### Permission Issues
- Ensure the MCP server has write permissions
- On some systems, you may need to grant permissions to Python/uvx

## Next Steps

- Read the [full README](README.md) for all available tools
- Check out [examples](EXAMPLES.md) for more use cases
- See [configuration options](CONFIGURATION.md) for advanced setup

## Getting Help

- [rs-hack Issues](https://github.com/1e1f/rs-hack/issues)
- [MCP Documentation](https://modelcontextprotocol.io/)
- [rs-hack Documentation](https://github.com/1e1f/rs-hack#readme)
