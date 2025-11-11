#!/usr/bin/env python3
"""
Basic tests for rs-hack MCP server.

Run with: python3 test_basic.py
"""

import subprocess
import json
import sys


def test_imports():
    """Test that all required modules can be imported."""
    print("Testing imports...")
    try:
        from mcp.server.fastmcp import FastMCP
        print("  ✓ FastMCP imported")

        import server
        print("  ✓ server module imported")

        # Check that tools are registered
        tools = [x for x in dir(server.mcp) if not x.startswith('_')]
        print(f"  ✓ Found {len(tools)} registered items")

        return True
    except Exception as e:
        print(f"  ✗ Import failed: {e}")
        return False


def test_rs_hack_available():
    """Test that rs-hack CLI is available."""
    print("\nTesting rs-hack CLI...")
    try:
        result = subprocess.run(
            ["rs-hack", "--version"],
            capture_output=True,
            text=True,
            timeout=5
        )
        if result.returncode == 0:
            version = result.stdout.strip()
            print(f"  ✓ rs-hack available: {version}")
            return True
        else:
            print(f"  ✗ rs-hack returned error: {result.stderr}")
            return False
    except FileNotFoundError:
        print("  ✗ rs-hack not found in PATH")
        print("    Install with: cargo install rs-hack")
        return False
    except Exception as e:
        print(f"  ✗ Error running rs-hack: {e}")
        return False


def test_server_tools():
    """Test that expected tools are available."""
    print("\nTesting server tools...")
    try:
        import server

        expected_tools = [
            "inspect_struct_literals",
            "inspect_match_arms",
            "inspect_enum_usage",
            "inspect_macro_calls",
            "add_struct_field",
            "update_struct_field",
            "add_enum_variant",
            "rename_enum_variant",
            "add_match_arm",
            "transform",
            "add_derive",
            "show_history",
            "revert_operation",
        ]

        # Get all functions decorated with @mcp.tool()
        mcp_obj = server.mcp
        tools_attr = getattr(mcp_obj, '_tools', None) or getattr(mcp_obj, 'tools', [])

        found_count = 0
        for tool_name in expected_tools:
            if hasattr(server, tool_name):
                print(f"  ✓ {tool_name}")
                found_count += 1
            else:
                print(f"  ✗ {tool_name} not found")

        print(f"\n  Found {found_count}/{len(expected_tools)} expected tools")
        return found_count == len(expected_tools)

    except Exception as e:
        print(f"  ✗ Error checking tools: {e}")
        return False


def test_run_rs_hack_helper():
    """Test the run_rs_hack helper function."""
    print("\nTesting run_rs_hack helper...")
    try:
        import server

        # Test with --version
        result = server.run_rs_hack(["--version"])
        if result.get("success"):
            print("  ✓ run_rs_hack() works")
            return True
        else:
            print(f"  ✗ run_rs_hack() failed: {result.get('error')}")
            return False
    except Exception as e:
        print(f"  ✗ Error testing run_rs_hack: {e}")
        return False


def main():
    """Run all tests."""
    print("=" * 60)
    print("rs-hack MCP Server - Basic Tests")
    print("=" * 60)

    tests = [
        ("Imports", test_imports),
        ("rs-hack CLI", test_rs_hack_available),
        ("Server Tools", test_server_tools),
        ("Helper Functions", test_run_rs_hack_helper),
    ]

    results = []
    for test_name, test_func in tests:
        try:
            results.append((test_name, test_func()))
        except Exception as e:
            print(f"\n✗ {test_name} crashed: {e}")
            results.append((test_name, False))

    print("\n" + "=" * 60)
    print("Test Results")
    print("=" * 60)

    passed = sum(1 for _, result in results if result)
    total = len(results)

    for test_name, result in results:
        status = "✓ PASS" if result else "✗ FAIL"
        print(f"{status:8} {test_name}")

    print(f"\nPassed: {passed}/{total}")

    if passed == total:
        print("\n✓ All tests passed!")
        return 0
    else:
        print(f"\n✗ {total - passed} test(s) failed")
        return 1


if __name__ == "__main__":
    sys.exit(main())
