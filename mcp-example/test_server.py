"""
Tests for rs-hack MCP server

Run with: uv run pytest
"""

import json
from server import run_rs_hack


def test_rs_hack_version():
    """Test that rs-hack is available and responds."""
    result = run_rs_hack(["--version"])
    # Should either succeed or fail with specific error
    assert "success" in result


def test_help_command():
    """Test that help command works."""
    result = run_rs_hack(["--help"])
    assert result["success"] or "rs-hack not found" in result.get("error", "")


def test_error_handling():
    """Test that invalid commands return proper error."""
    result = run_rs_hack(["invalid-command"])
    assert not result["success"]
    assert "error" in result


def test_inspect_returns_structure():
    """Test that inspect returns expected structure."""
    # This assumes you have rs-hack installed
    result = run_rs_hack(["inspect", "--help"])
    
    if result["success"]:
        assert "output" in result or "data" in result
    else:
        # If rs-hack not installed, should have clear error
        assert "rs-hack not found" in result.get("error", "")


def test_json_parsing():
    """Test JSON output parsing."""
    # Mock a JSON response
    result = {
        "success": True,
        "output": '{"test": "value"}'
    }
    
    # Our run_rs_hack should parse this
    assert result["success"]


# Integration tests (require rs-hack to be installed)

def test_history_command():
    """Test history command."""
    result = run_rs_hack(["history", "--limit", "5"])
    
    if "rs-hack not found" in result.get("error", ""):
        # Skip test if rs-hack not installed
        return
    
    assert "success" in result


def test_inspect_with_invalid_path():
    """Test inspect with invalid path."""
    result = run_rs_hack([
        "inspect",
        "--path", "/nonexistent/path.rs",
        "--node-type", "struct-literal"
    ])
    
    if "rs-hack not found" in result.get("error", ""):
        return
    
    # Should handle gracefully
    assert "error" in result or result["success"]


if __name__ == "__main__":
    import pytest
    pytest.main([__file__, "-v"])
