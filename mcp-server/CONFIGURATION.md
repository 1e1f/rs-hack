# Configuration Guide

Detailed configuration instructions for different MCP clients and use cases.

## Table of Contents

- [Claude Desktop](#claude-desktop)
- [Claude Code](#claude-code)
- [Cursor](#cursor)
- [Cline](#cline)
- [Windsurf](#windsurf)
- [Custom MCP Clients](#custom-mcp-clients)
- [HTTP Mode](#http-mode)
- [Environment Variables](#environment-variables)
- [Advanced Configuration](#advanced-configuration)

## Claude Desktop

### Location

Config file location:
- **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Windows**: `%APPDATA%\Claude\claude_desktop_config.json`
- **Linux**: `~/.config/Claude/claude_desktop_config.json`

### Basic Configuration (Recommended)

Using uvx to run directly from git (no local installation required):

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

### Local Installation

If you have the repository cloned locally:

```json
{
  "mcpServers": {
    "rs-hack": {
      "command": "uvx",
      "args": [
        "--from",
        "/absolute/path/to/rs-hack/mcp-server",
        "server.py"
      ]
    }
  }
}
```

### With Environment Variables

```json
{
  "mcpServers": {
    "rs-hack": {
      "command": "uvx",
      "args": [
        "--from",
        "git+https://github.com/1e1f/rs-hack#subdirectory=mcp-server",
        "server.py"
      ],
      "env": {
        "RS_HACK_STATE_DIR": "/custom/state/directory",
        "RUST_LOG": "debug"
      }
    }
  }
}
```

### Multiple Servers

You can run rs-hack alongside other MCP servers:

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
    },
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/project"]
    },
    "git": {
      "command": "uvx",
      "args": ["mcp-server-git", "--repository", "/path/to/repo"]
    }
  }
}
```

## Claude Code

### Basic Setup

From your Rust project directory:

```bash
claude mcp add rs-hack -- uvx --from git+https://github.com/1e1f/rs-hack#subdirectory=mcp-server server.py
```

### Local Installation

```bash
claude mcp add rs-hack -- uvx --from /path/to/rs-hack/mcp-server server.py
```

### With Environment Variables

```bash
RS_HACK_STATE_DIR=/tmp/rs-hack-state claude mcp add rs-hack -- uvx --from git+https://github.com/1e1f/rs-hack#subdirectory=mcp-server server.py
```

### List Configured Servers

```bash
claude mcp list
```

### Remove Server

```bash
claude mcp remove rs-hack
```

## Cursor

### Configuration

Add to Cursor's MCP settings (usually in `.cursor/mcp.json`):

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

## Cline

### Configuration

Add to Cline's settings (`.cline/mcp.json`):

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

## Windsurf

### Configuration

Add to Windsurf's MCP configuration:

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

## Custom MCP Clients

For any MCP-compatible client, you can use:

### Using uvx (Recommended)

```json
{
  "command": "uvx",
  "args": [
    "--from",
    "git+https://github.com/1e1f/rs-hack#subdirectory=mcp-server",
    "server.py"
  ]
}
```

### Using Python Directly

```json
{
  "command": "python",
  "args": ["/path/to/rs-hack/mcp-server/server.py"]
}
```

### Using uv run

```json
{
  "command": "uv",
  "args": [
    "run",
    "--directory",
    "/path/to/rs-hack/mcp-server",
    "server.py"
  ]
}
```

## HTTP Mode

For web-based clients or remote access:

### Start HTTP Server

```bash
cd mcp-server
uv run server.py --transport streamable-http --port 9000
```

### Configuration

Connect your client to:
```
http://localhost:9000/mcp
```

### Custom Port

```bash
uv run server.py --transport streamable-http --port 8080
```

### With SSL (Production)

Use a reverse proxy like nginx or caddy:

```nginx
server {
    listen 443 ssl;
    server_name mcp.example.com;

    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;

    location /mcp {
        proxy_pass http://localhost:9000;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

## Environment Variables

### RS_HACK_STATE_DIR

Control where operation history is stored:

```bash
export RS_HACK_STATE_DIR=/custom/state/directory
```

Or in MCP config:

```json
{
  "env": {
    "RS_HACK_STATE_DIR": "/custom/state/directory"
  }
}
```

### RUST_LOG

Enable debug logging from rs-hack CLI:

```bash
export RUST_LOG=debug
```

### PATH

Ensure rs-hack is in PATH:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

## Advanced Configuration

### Using Specific rs-hack Version

Install a specific version:

```bash
cargo install rs-hack --version 0.4.3
```

Then configure normally.

### Custom Python Environment

If you need specific Python versions or dependencies:

```bash
# Create virtual environment
cd mcp-server
python3.11 -m venv venv
source venv/bin/activate
pip install -e .
```

Then use in config:

```json
{
  "command": "/path/to/mcp-server/venv/bin/python",
  "args": ["/path/to/mcp-server/server.py"]
}
```

### Multiple Project Configurations

You can run different instances for different projects:

```json
{
  "mcpServers": {
    "rs-hack-project-a": {
      "command": "uvx",
      "args": ["--from", "git+https://github.com/1e1f/rs-hack#subdirectory=mcp-server", "server.py"],
      "env": {
        "RS_HACK_STATE_DIR": "/projects/a/.rs-hack-state"
      }
    },
    "rs-hack-project-b": {
      "command": "uvx",
      "args": ["--from", "git+https://github.com/1e1f/rs-hack#subdirectory=mcp-server", "server.py"],
      "env": {
        "RS_HACK_STATE_DIR": "/projects/b/.rs-hack-state"
      }
    }
  }
}
```

### Development Mode

For development with auto-reload:

```bash
cd mcp-server
uv run mcp dev server.py
```

Or use the MCP Inspector:

```bash
npx @modelcontextprotocol/inspector uv run server.py
```

## Troubleshooting

### Server Not Starting

Check logs in your MCP client. Common issues:

1. **rs-hack not found**
   ```bash
   which rs-hack  # Should show ~/.cargo/bin/rs-hack
   cargo install rs-hack
   ```

2. **Python version mismatch**
   ```bash
   python --version  # Should be 3.10+
   ```

3. **Permission issues**
   ```bash
   chmod +x /path/to/server.py
   ```

### Server Not Appearing

1. Restart your MCP client after config changes
2. Check config file syntax (use a JSON validator)
3. Verify file paths are absolute, not relative
4. Check client logs for errors

### Operations Failing

1. Verify you're in a Rust project directory
2. Check file permissions
3. Ensure rs-hack CLI works:
   ```bash
   rs-hack --help
   ```

### State Directory Issues

```bash
# Check state directory
ls -la ~/.rs-hack  # or your custom RS_HACK_STATE_DIR

# Clean old state
rs-hack clean --keep-days 7
```

## Testing Configuration

### Test rs-hack CLI

```bash
rs-hack --version
rs-hack inspect --path src/main.rs --node-type struct-literal
```

### Test MCP Server

```bash
cd mcp-server
uv run server.py
# Should start without errors
# Ctrl+C to stop
```

### Test with MCP Inspector

```bash
npx @modelcontextprotocol/inspector uv run server.py
```

This opens a web interface to test all tools.

## Getting Help

- [rs-hack Issues](https://github.com/1e1f/rs-hack/issues)
- [MCP Documentation](https://modelcontextprotocol.io/)
- [FastMCP Documentation](https://github.com/jlowin/fastmcp)
