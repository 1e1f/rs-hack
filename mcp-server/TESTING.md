# Testing Guide for rs-hack MCP Server

This guide shows you how to test the MCP server to verify it works correctly.

## Quick Test (30 seconds)

```bash
# 1. Install dependencies
pip install mcp

# 2. Run basic tests
python3 test_basic.py
```

Expected output:
```
✓ All tests passed!
```

## Testing Methods

### Method 1: Basic Python Tests (Fastest)

Run the included test script:

```bash
cd mcp-server
python3 test_basic.py
```

This tests:
- ✅ Python imports work
- ✅ rs-hack CLI is available
- ✅ All 13 tools are registered
- ✅ Helper functions work

### Method 2: MCP Inspector (Recommended - Interactive)

The [MCP Inspector](https://github.com/modelcontextprotocol/inspector) provides a web UI to test all tools interactively.

#### Install and Run

```bash
# Install inspector
npm install -g @modelcontextprotocol/inspector

# Run with your server
npx @modelcontextprotocol/inspector python3 server.py
```

This opens a web interface where you can:
- See all available tools
- Test each tool with different parameters
- View responses in real-time
- Debug errors

#### Example Session

1. Open the web UI (usually http://localhost:5173)
2. Click on "inspect_struct_literals"
3. Enter parameters:
   - path: `"../src/lib.rs"`
   - format: `"snippets"`
4. Click "Call Tool"
5. See the results!

### Method 3: MCP Dev Mode

Use the official MCP development tool:

```bash
# Install if needed
pip install mcp

# Run in dev mode
mcp dev server.py
```

This provides:
- Tool listing
- Parameter validation
- Error messages
- JSON output

### Method 4: Direct Server Test

Test that the server starts without errors:

```bash
# Should start and wait for input (Ctrl+C to stop)
python3 server.py
```

If you see no errors, the server is working!

### Method 5: Integration Test with Claude Desktop

The ultimate test - use it with an actual MCP client:

#### 1. Configure Claude Desktop

Edit `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "rs-hack-test": {
      "command": "python3",
      "args": ["/absolute/path/to/rs-hack/mcp-server/server.py"]
    }
  }
}
```

#### 2. Restart Claude Desktop

#### 3. Test in Chat

Ask Claude:
```
Can you list the rs-hack tools available?
```

Claude should show all 13 tools!

#### 4. Test a Real Operation

In a Rust project, ask:
```
Show me all struct literals in src/main.rs
```

Claude will use `inspect_struct_literals` and show results.

## Troubleshooting Tests

### Test Fails: "rs-hack not found"

**Problem**: rs-hack CLI is not in PATH

**Solution**:
```bash
# Install rs-hack
cargo install rs-hack

# Or add local build to PATH
export PATH="/path/to/rs-hack/target/release:$PATH"

# Verify
rs-hack --version
```

### Test Fails: "ModuleNotFoundError: No module named 'mcp'"

**Problem**: MCP package not installed

**Solution**:
```bash
pip install mcp
```

### Test Fails: Import errors

**Problem**: Python version too old

**Solution**: Use Python 3.10+
```bash
python3 --version  # Should be 3.10 or higher
```

### MCP Inspector Won't Start

**Problem**: Port already in use or npm not installed

**Solution**:
```bash
# Check npm is installed
npm --version

# If not, install Node.js from nodejs.org

# Try different port
npx @modelcontextprotocol/inspector --port 5174 python3 server.py
```

### Server Starts But No Tools Appear

**Problem**: Incorrect server path or configuration

**Solution**: Check that you're using absolute paths in config files

## Testing Individual Tools

### Test inspect_struct_literals

Create a test file:

```rust
// test.rs
struct User {
    name: String,
}

fn main() {
    let user = User { name: "Alice".to_string() };
}
```

Then test:

```python
import server
result = server.inspect_struct_literals("test.rs", "User", "snippets")
print(result)
```

Should show the `User { ... }` literal.

### Test add_struct_field (dry-run)

```python
import server
result = server.add_struct_field(
    "test.rs",
    "User",
    "age: u32",
    apply=False  # Dry-run
)
print(result)
```

Should show preview without modifying files.

### Test transform

```python
import server

# Create test file with debug print
with open("test.rs", "w") as f:
    f.write('fn main() { eprintln!("[DEBUG] test"); }')

# Test transform
result = server.transform(
    "test.rs",
    "macro-call",
    "comment",
    name="eprintln",
    apply=True
)
print(result)
```

Should comment out the eprintln!

## Automated Testing

### Run Tests in CI

```bash
# Install dependencies
pip install mcp pytest

# Run tests
python3 test_basic.py

# Exit code 0 = success, 1 = failure
echo $?
```

### GitHub Actions Example

```yaml
name: Test MCP Server

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Install rs-hack
        run: cargo install --path .

      - name: Install Python deps
        run: pip install mcp
        working-directory: mcp-server

      - name: Run tests
        run: python3 test_basic.py
        working-directory: mcp-server
```

## Performance Testing

### Test Response Time

```python
import time
import server

# Time an operation
start = time.time()
result = server.inspect_struct_literals("src/**/*.rs", None, "locations")
elapsed = time.time() - start

print(f"Took {elapsed:.2f} seconds")
```

### Test Large Codebase

```bash
# Test on a real project
cd /path/to/large/rust/project

python3 <<EOF
import sys
sys.path.insert(0, '/path/to/mcp-server')
import server

# This should handle many files
result = server.inspect_struct_literals("src/**/*.rs", None, "locations")
print(f"Found {len(result.split('\\n'))} locations")
EOF
```

## What Good Output Looks Like

### ✅ Successful Test Output

```
============================================================
rs-hack MCP Server - Basic Tests
============================================================
Testing imports...
  ✓ FastMCP imported
  ✓ server module imported
  ✓ Found 30 registered items

Testing rs-hack CLI...
  ✓ rs-hack available: rs-hack 0.4.3

Testing server tools...
  ✓ inspect_struct_literals
  ✓ inspect_match_arms
  [... all tools ...]

  Found 13/13 expected tools

Testing run_rs_hack helper...
  ✓ run_rs_hack() works

============================================================
Test Results
============================================================
✓ PASS   Imports
✓ PASS   rs-hack CLI
✓ PASS   Server Tools
✓ PASS   Helper Functions

Passed: 4/4

✓ All tests passed!
```

### ✅ Successful MCP Inspector

When you open the inspector, you should see:
- Server Status: Connected
- Tools: 13 tools listed
- Each tool has documentation
- Parameters are shown with types

### ✅ Successful Claude Desktop Integration

In Claude Desktop, when you ask about rs-hack:
- Claude lists all available tools
- Claude can call tools and get results
- No error messages in logs

## Common Test Scenarios

### Scenario 1: First-Time Setup

```bash
# 1. Clone repo
git clone https://github.com/1e1f/rs-hack
cd rs-hack/mcp-server

# 2. Install dependencies
cargo install --path ..
pip install mcp

# 3. Run tests
python3 test_basic.py

# Expected: All tests pass ✓
```

### Scenario 2: After Making Changes

```bash
# 1. Edit server.py
# 2. Check syntax
python3 -m py_compile server.py

# 3. Run tests
python3 test_basic.py

# 4. Test with inspector
npx @modelcontextprotocol/inspector python3 server.py
```

### Scenario 3: Before Publishing

```bash
# 1. Run all tests
python3 test_basic.py

# 2. Test with real Rust project
cd /path/to/rust/project
python3 /path/to/mcp-server/test_basic.py

# 3. Test with inspector
npx @modelcontextprotocol/inspector python3 /path/to/mcp-server/server.py

# 4. Test with Claude Desktop (see above)
```

## Continuous Testing

While developing, use a test loop:

```bash
# Watch for changes and rerun tests
while true; do
    clear
    python3 test_basic.py
    echo "\n\nWaiting for changes... (Ctrl+C to stop)"
    sleep 5
done
```

## Summary

**Quick Check**: Run `python3 test_basic.py` - should see ✓ All tests passed!

**Full Verification**:
1. ✅ Basic tests pass
2. ✅ MCP Inspector shows all tools
3. ✅ Claude Desktop recognizes the server
4. ✅ Can call tools and get results

If all four pass, your MCP server is working correctly! 🎉

## Next Steps

Once tests pass:
1. Try it with Claude Desktop (see QUICKSTART.md)
2. Test real operations on your Rust projects
3. Check out EXAMPLES.md for usage patterns

## Getting Help

If tests fail:
1. Check the error message carefully
2. Review [Troubleshooting](#troubleshooting-tests) above
3. Open an issue: https://github.com/1e1f/rs-hack/issues
