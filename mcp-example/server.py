"""
rs-hack MCP Server

Exposes rs-hack's AST-aware Rust refactoring tools through the Model Context Protocol.
"""

import subprocess
import json
from pathlib import Path
from typing import Any, Literal
from mcp.server.fastmcp import FastMCP

# Initialize MCP server
mcp = FastMCP(
    "rs-hack",
    instructions="""rs-hack provides AST-aware refactoring tools for Rust code.

Key principles:
- All operations are DRY-RUN by default - use apply=True to make actual changes
- Operations are idempotent - safe to run multiple times
- Use inspect tools first to see what will be affected
- All operations support glob patterns for multi-file edits
- Changes are tracked with unique run IDs for easy revert

Workflow:
1. Use inspect_* tools to explore code
2. Preview changes (dry-run)
3. Apply changes with apply=True
4. Use history/revert if needed

Tool Selection Guide:
- Finding code: inspect_struct_literals, inspect_match_arms, inspect_enum_usage
- Adding code: add_struct_field, add_enum_variant, add_match_arm
- Modifying code: transform, update_struct_field, rename_enum_variant
- Bulk operations: Use glob patterns like "src/**/*.rs"
"""
)


def run_rs_hack(args: list[str]) -> dict[str, Any]:
    """Execute rs-hack command and return structured result."""
    try:
        result = subprocess.run(
            ["rs-hack"] + args,
            capture_output=True,
            text=True,
            check=False
        )
        
        # Try to parse JSON output if available
        output = result.stdout.strip()
        if output.startswith('{') or output.startswith('['):
            try:
                return {"success": True, "data": json.loads(output)}
            except json.JSONDecodeError:
                pass
        
        if result.returncode == 0:
            return {"success": True, "output": output}
        else:
            return {
                "success": False,
                "error": result.stderr.strip() or result.stdout.strip()
            }
            
    except FileNotFoundError:
        return {
            "success": False,
            "error": "rs-hack not found. Install with: cargo install rs-hack"
        }
    except Exception as e:
        return {"success": False, "error": str(e)}


# ============================================================================
# INSPECTION TOOLS - Find and explore code
# ============================================================================

@mcp.tool()
def inspect_struct_literals(
    path: str,
    name: str | None = None,
    format: Literal["snippets", "locations", "json"] = "snippets"
) -> str:
    """Inspect struct literal initializations in Rust files.
    
    Args:
        path: File path or glob pattern (e.g., "src/**/*.rs")
        name: Optional struct name to filter (supports patterns like "*::Rectangle")
        format: Output format - snippets (code), locations (file:line:col), or json
        
    Returns:
        List of struct literals with their locations and code
        
    Examples:
        - List all struct literals: inspect_struct_literals("src/main.rs")
        - Find Shadow structs: inspect_struct_literals("src/**/*.rs", "Shadow")
        - Enum variants: inspect_struct_literals("src/**/*.rs", "View::Rectangle")
    """
    args = ["inspect", "--path", path, "--node-type", "struct-literal", "--format", format]
    if name:
        args.extend(["--name", name])
    
    result = run_rs_hack(args)
    if result["success"]:
        return result.get("output", "") or json.dumps(result.get("data"), indent=2)
    return f"Error: {result['error']}"


@mcp.tool()
def inspect_match_arms(
    path: str,
    name: str | None = None,
    format: Literal["snippets", "locations", "json"] = "snippets"
) -> str:
    """Inspect match expression arms in Rust files.
    
    Args:
        path: File path or glob pattern
        name: Optional pattern to match (e.g., "Status::Active")
        format: Output format
        
    Returns:
        List of match arms with their patterns and code
    """
    args = ["inspect", "--path", path, "--node-type", "match-arm", "--format", format]
    if name:
        args.extend(["--name", name])
    
    result = run_rs_hack(args)
    if result["success"]:
        return result.get("output", "") or json.dumps(result.get("data"), indent=2)
    return f"Error: {result['error']}"


@mcp.tool()
def inspect_enum_usage(
    path: str,
    name: str,
    format: Literal["snippets", "locations", "json"] = "snippets"
) -> str:
    """Find all usages of an enum variant in Rust files.
    
    Args:
        path: File path or glob pattern
        name: Enum variant to find (e.g., "Operator::PropagateError")
        format: Output format
        
    Returns:
        All places where the enum variant is referenced
        
    Example:
        Find all uses of Status::Active in the codebase
    """
    args = ["inspect", "--path", path, "--node-type", "enum-usage", 
            "--name", name, "--format", format]
    
    result = run_rs_hack(args)
    if result["success"]:
        return result.get("output", "") or json.dumps(result.get("data"), indent=2)
    return f"Error: {result['error']}"


@mcp.tool()
def inspect_macro_calls(
    path: str,
    name: str,
    content_filter: str | None = None,
    format: Literal["snippets", "locations", "json"] = "snippets"
) -> str:
    """Find macro invocations in Rust files.
    
    Args:
        path: File path or glob pattern
        name: Macro name (e.g., "eprintln", "todo")
        content_filter: Optional content to filter by
        format: Output format
        
    Returns:
        All macro calls matching the criteria
        
    Example:
        Find all debug prints: inspect_macro_calls("src/**/*.rs", "eprintln", "[DEBUG]")
    """
    args = ["inspect", "--path", path, "--node-type", "macro-call", 
            "--name", name, "--format", format]
    if content_filter:
        args.extend(["--content-filter", content_filter])
    
    result = run_rs_hack(args)
    if result["success"]:
        return result.get("output", "") or json.dumps(result.get("data"), indent=2)
    return f"Error: {result['error']}"


# ============================================================================
# STRUCT OPERATIONS
# ============================================================================

@mcp.tool()
def add_struct_field(
    path: str,
    struct_name: str,
    field: str,
    position: str | None = None,
    literal_default: str | None = None,
    apply: bool = False
) -> str:
    """Add a field to Rust struct definitions and/or literals.
    
    Args:
        path: File path or glob pattern
        struct_name: Name of the struct (supports patterns like "*::Rectangle")
        field: Field definition (e.g., "email: String" or just "email" for literals only)
        position: Where to add ("after:field_name", "before:field_name", or "Last")
        literal_default: If provided, also update struct literals with this default value
        apply: If True, make actual changes. If False, show preview only.
        
    Returns:
        Preview of changes or confirmation of applied changes
        
    Examples:
        - Add to definition: add_struct_field("src/user.rs", "User", "age: u32", apply=True)
        - Add with position: add_struct_field("src/user.rs", "User", "email: String", "after:name")
        - Add to definition AND literals: add_struct_field("src/**/*.rs", "Config", 
                                          "timeout: u64", literal_default="30")
    """
    args = ["add-struct-field", "--path", path, "--struct-name", struct_name, "--field", field]
    
    if position:
        args.extend(["--position", position])
    if literal_default:
        args.extend(["--literal-default", literal_default])
    if apply:
        args.append("--apply")
    
    result = run_rs_hack(args)
    if result["success"]:
        output = result.get("output", "")
        if not apply:
            return f"DRY RUN - Preview of changes:\n{output}\n\nUse apply=True to make changes."
        return output or "Successfully added struct field"
    return f"Error: {result['error']}"


@mcp.tool()
def update_struct_field(
    path: str,
    struct_name: str,
    field: str,
    apply: bool = False
) -> str:
    """Update an existing struct field (change type or visibility).
    
    Args:
        path: File path or glob pattern
        struct_name: Name of the struct
        field: New field definition (e.g., "pub email: String" or "id: i64")
        apply: If True, make actual changes
        
    Example:
        Make field public: update_struct_field("src/user.rs", "User", "pub age: u32", True)
    """
    args = ["update-struct-field", "--path", path, "--struct-name", struct_name, "--field", field]
    if apply:
        args.append("--apply")
    
    result = run_rs_hack(args)
    if result["success"]:
        output = result.get("output", "")
        if not apply:
            return f"DRY RUN - Preview:\n{output}\n\nUse apply=True to make changes."
        return output or "Successfully updated struct field"
    return f"Error: {result['error']}"


# ============================================================================
# ENUM OPERATIONS
# ============================================================================

@mcp.tool()
def add_enum_variant(
    path: str,
    enum_name: str,
    variant: str,
    apply: bool = False
) -> str:
    """Add a variant to a Rust enum.
    
    Args:
        path: File path or glob pattern
        enum_name: Name of the enum
        variant: Variant definition (e.g., "Pending" or "Error { code: i32, msg: String }")
        apply: If True, make actual changes
        
    Examples:
        - Simple: add_enum_variant("src/types.rs", "Status", "Archived", True)
        - With data: add_enum_variant("src/types.rs", "Message", "Error { code: i32 }", True)
    """
    args = ["add-enum-variant", "--path", path, "--enum-name", enum_name, "--variant", variant]
    if apply:
        args.append("--apply")
    
    result = run_rs_hack(args)
    if result["success"]:
        output = result.get("output", "")
        if not apply:
            return f"DRY RUN - Preview:\n{output}\n\nUse apply=True to make changes."
        return output or "Successfully added enum variant"
    return f"Error: {result['error']}"


@mcp.tool()
def rename_enum_variant(
    path: str,
    enum_name: str,
    old_variant: str,
    new_variant: str,
    apply: bool = False
) -> str:
    """Rename an enum variant throughout the entire codebase.
    
    This is a powerful operation that updates:
    - Enum variant definitions
    - Match arm patterns
    - Constructor calls
    - All references
    
    Args:
        path: File path or glob pattern (usually "src/**/*.rs")
        enum_name: Name of the enum
        old_variant: Current variant name
        new_variant: New variant name
        apply: If True, make actual changes
        
    Example:
        Rename across codebase: rename_enum_variant("src/**/*.rs", "Status", 
                                                   "Draft", "Pending", True)
    """
    args = ["rename-enum-variant", "--paths", path, "--enum-name", enum_name,
            "--old-variant", old_variant, "--new-variant", new_variant]
    if apply:
        args.append("--apply")
    
    result = run_rs_hack(args)
    if result["success"]:
        output = result.get("output", "")
        if not apply:
            return f"DRY RUN - Preview:\n{output}\n\nUse apply=True to make changes."
        return output or f"Successfully renamed {old_variant} to {new_variant}"
    return f"Error: {result['error']}"


# ============================================================================
# MATCH OPERATIONS
# ============================================================================

@mcp.tool()
def add_match_arm(
    path: str,
    pattern: str,
    body: str,
    function: str | None = None,
    enum_name: str | None = None,
    auto_detect: bool = False,
    apply: bool = False
) -> str:
    """Add a match arm to match expressions.
    
    Args:
        path: File path
        pattern: Match pattern (e.g., "Status::Archived")
        body: Match arm body (e.g., '"archived".to_string()')
        function: Function name containing the match
        enum_name: Enum name for auto-detection
        auto_detect: If True, automatically add all missing enum variants
        apply: If True, make actual changes
        
    Examples:
        - Add single arm: add_match_arm("src/handler.rs", "Status::Archived", 
                                       '"archived"', "handle_status", apply=True)
        - Auto-detect missing: add_match_arm("src/handler.rs", "", "todo!()", 
                                            "handle_status", "Status", auto_detect=True, apply=True)
    """
    args = ["add-match-arm", "--path", path]
    
    if auto_detect:
        args.extend(["--auto-detect", "--enum-name", enum_name or "", 
                    "--body", body])
        if function:
            args.extend(["--function", function])
    else:
        args.extend(["--pattern", pattern, "--body", body])
        if function:
            args.extend(["--function", function])
    
    if apply:
        args.append("--apply")
    
    result = run_rs_hack(args)
    if result["success"]:
        output = result.get("output", "")
        if not apply:
            return f"DRY RUN - Preview:\n{output}\n\nUse apply=True to make changes."
        return output or "Successfully added match arm(s)"
    return f"Error: {result['error']}"


# ============================================================================
# TRANSFORM - Generic find and modify
# ============================================================================

@mcp.tool()
def transform(
    path: str,
    node_type: Literal["macro-call", "method-call", "function-call", "enum-usage", 
                      "struct-literal", "match-arm", "identifier", "type-ref"],
    action: Literal["comment", "remove", "replace"],
    name: str | None = None,
    content_filter: str | None = None,
    replacement: str | None = None,
    apply: bool = False
) -> str:
    """Generic transformation tool - find and modify any AST nodes.
    
    This is the most flexible tool - use it when you need to:
    - Comment out debug code
    - Remove unwanted calls
    - Replace deprecated functions
    
    Args:
        path: File path or glob pattern
        node_type: Type of AST node to find
        action: What to do (comment, remove, or replace)
        name: Name filter (e.g., "eprintln" for macros)
        content_filter: Filter by content (e.g., "[DEBUG]")
        replacement: New code when action="replace"
        apply: If True, make actual changes
        
    Examples:
        - Comment debug logs: transform("src/**/*.rs", "macro-call", "comment", 
                                       "eprintln", "[DEBUG]", apply=True)
        - Remove unwraps: transform("src/**/*.rs", "method-call", "comment", 
                                   "unwrap", apply=True)
        - Replace function: transform("src/**/*.rs", "function-call", "replace", 
                                     "old_fn", replacement="new_fn", apply=True)
    """
    args = ["transform", "--path", path, "--node-type", node_type, "--action", action]
    
    if name:
        args.extend(["--name", name])
    if content_filter:
        args.extend(["--content-filter", content_filter])
    if action == "replace" and replacement:
        args.extend(["--with", replacement])
    if apply:
        args.append("--apply")
    
    result = run_rs_hack(args)
    if result["success"]:
        output = result.get("output", "")
        if not apply:
            return f"DRY RUN - Preview:\n{output}\n\nUse apply=True to make changes."
        return output or f"Successfully performed {action} operation"
    return f"Error: {result['error']}"


# ============================================================================
# DERIVE AND OTHER OPERATIONS
# ============================================================================

@mcp.tool()
def add_derive(
    path: str,
    target_type: Literal["struct", "enum"],
    name: str,
    derives: str,
    where_filter: str | None = None,
    apply: bool = False
) -> str:
    """Add derive macros to structs or enums.
    
    Args:
        path: File path or glob pattern
        target_type: "struct" or "enum"
        name: Name of the type
        derives: Comma-separated derives (e.g., "Clone,Debug,Serialize")
        where_filter: Optional filter (e.g., "derives_trait:Clone")
        apply: If True, make actual changes
        
    Examples:
        - Add derives: add_derive("src/models.rs", "struct", "User", 
                                 "Clone,Debug,Serialize", apply=True)
        - Conditional: add_derive("src/**/*.rs", "struct", "Config", "Serialize",
                                 "derives_trait:Clone", apply=True)
    """
    args = ["add-derive", "--path", path, "--target-type", target_type,
            "--name", name, "--derives", derives]
    
    if where_filter:
        args.extend(["--where", where_filter])
    if apply:
        args.append("--apply")
    
    result = run_rs_hack(args)
    if result["success"]:
        output = result.get("output", "")
        if not apply:
            return f"DRY RUN - Preview:\n{output}\n\nUse apply=True to make changes."
        return output or "Successfully added derives"
    return f"Error: {result['error']}"


# ============================================================================
# HISTORY AND REVERT
# ============================================================================

@mcp.tool()
def show_history(limit: int = 10) -> str:
    """Show recent rs-hack operations.
    
    Args:
        limit: Number of recent operations to show
        
    Returns:
        List of operations with their run IDs and status
    """
    result = run_rs_hack(["history", "--limit", str(limit)])
    if result["success"]:
        return result.get("output", "") or "No history available"
    return f"Error: {result['error']}"


@mcp.tool()
def revert_operation(run_id: str, force: bool = False) -> str:
    """Revert a previous rs-hack operation.
    
    Args:
        run_id: The run ID from history (7-character hash)
        force: If True, revert even if files have changed since
        
    Returns:
        Confirmation of revert or error message
        
    Example:
        Undo last change: revert_operation("a05a626")
    """
    args = ["revert", run_id]
    if force:
        args.append("--force")
    
    result = run_rs_hack(args)
    if result["success"]:
        return result.get("output", "") or f"Successfully reverted operation {run_id}"
    return f"Error: {result['error']}"


if __name__ == "__main__":
    # Run the MCP server
    mcp.run()
