# Configuration Examples

## Claude Desktop Configuration

### macOS
Location: `~/Library/Application Support/Claude/claude_desktop_config.json`

### Windows  
Location: `%APPDATA%\Claude\claude_desktop_config.json`

### Linux (Community Version)
Location: `~/.config/Claude/claude_desktop_config.json`

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

### Running from Git (No Local Clone)

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

## Claude Code Configuration

From your Rust project directory:

```bash
# Using local installation
claude mcp add rs-hack -- uvx --from /path/to/rs-hack-mcp server.py

# Using git
claude mcp add rs-hack -- uvx --from git+https://github.com/YOUR_USERNAME/rs-hack-mcp server.py
```

## Cursor Configuration

Add to `.cursor/config.json` in your project:

```json
{
  "mcp": {
    "servers": {
      "rs-hack": {
        "command": "uvx",
        "args": ["--from", "/path/to/rs-hack-mcp", "server.py"]
      }
    }
  }
}
```

## Cline Configuration

In VSCode, open Cline settings and add:

```json
{
  "cline.mcpServers": {
    "rs-hack": {
      "command": "uvx",
      "args": ["--from", "/path/to/rs-hack-mcp", "server.py"]
    }
  }
}
```

## Windsurf Configuration

Add to your Windsurf MCP settings:

```json
{
  "rs-hack": {
    "command": "uvx",
    "args": ["--from", "/path/to/rs-hack-mcp", "server.py"]
  }
}
```

## Combined Configuration (With Serena)

Example showing rs-hack alongside Serena for multi-language + Rust-specialist workflow:

```json
{
  "mcpServers": {
    "serena": {
      "command": "uvx",
      "args": [
        "--from",
        "git+https://github.com/oraios/serena",
        "serena",
        "start-mcp-server",
        "--context",
        "ide-assistant"
      ]
    },
    "rs-hack": {
      "command": "uvx",
      "args": [
        "--from",
        "/path/to/rs-hack-mcp",
        "server.py"
      ]
    }
  }
}
```

## Streamable HTTP Mode (For Web Clients)

Start the server:
```bash
cd rs-hack-mcp
uv run server.py --transport streamable-http --port 9000
```

Then configure your client to connect to: `http://localhost:9000/mcp`

### For ChatGPT (via mcpo)

1. Start rs-hack MCP server in HTTP mode (above)
2. Use [mcpo](https://github.com/modelcontextprotocol/servers) to expose it:
   ```bash
   npx @mcpo/server http://localhost:9000/mcp
   ```
3. Configure ChatGPT with the OpenAPI spec from mcpo

## Docker Configuration (Advanced)

If you want to run in Docker for isolation:

```dockerfile
FROM python:3.11-slim

# Install Rust and rs-hack
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"
RUN cargo install rs-hack

# Install Python dependencies
WORKDIR /app
COPY pyproject.toml .
RUN pip install uv && uv sync

# Copy server
COPY server.py .

CMD ["uv", "run", "server.py"]
```

Then configure client:
```json
{
  "mcpServers": {
    "rs-hack": {
      "command": "docker",
      "args": [
        "run",
        "--rm",
        "-i",
        "-v",
        "${workspaceFolder}:/workspace",
        "rs-hack-mcp"
      ]
    }
  }
}
```

## Environment Variables (Optional)

You can set these in your shell config or MCP client environment:

```bash
# Use a specific rs-hack binary
export RS_HACK_BIN=/path/to/custom/rs-hack

# Enable debug logging
export MCP_DEBUG=1
```

## Verification

After configuration, verify the server is working:

1. Restart your MCP client
2. Look for rs-hack tools in the tools list
3. Try a simple inspection command:
   ```
   "List all struct literals in src/main.rs"
   ```

Check client logs if tools don't appear:
- Claude Desktop: `~/Library/Logs/Claude/mcp*.log` (macOS)
- Claude Code: Check command output
- Others: Consult client documentation
