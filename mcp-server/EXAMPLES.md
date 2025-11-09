# Usage Examples

Comprehensive examples of using rs-hack MCP server for common tasks.

## Inspection Examples

### Find All Struct Literals

```
User: "Show me all Shadow struct initializations"

AI uses: inspect_struct_literals("src/**/*.rs", "Shadow", "snippets")

Output:
// src/ui/shadow.rs:42:18 - Shadow
Shadow { offset: Vec2::new(2.0, 2.0), blur: 4.0, color: Color32::BLACK }

// src/renderer/mod.rs:156:25 - Shadow
Shadow { offset: Vec2::ZERO, blur: 0.0, color: Color32::WHITE }
```

### Find Enum Variant Usage

```
User: "Where is Status::Active used?"

AI uses: inspect_enum_usage("src/**/*.rs", "Status::Active", "snippets")

Output shows all places where Status::Active appears:
- Match patterns
- Constructor calls
- Comparisons
- Return statements
```

### Find Debug Macros

```
User: "Find all eprintln! statements with [DEBUG]"

AI uses: inspect_macro_calls("src/**/*.rs", "eprintln", "[DEBUG]", "locations")

Output:
src/network/client.rs:45:4
src/network/server.rs:123:8
src/handlers/websocket.rs:67:12
```

## Struct Operation Examples

### Add Field to Struct Definition

```
User: "Add an email field to the User struct"

AI uses: add_struct_field("src/models/user.rs", "User", "email: String", apply=False)

Preview:
pub struct User {
    id: u64,
    name: String,
+   email: String,
}

User: "Apply it"

AI uses: add_struct_field("src/models/user.rs", "User", "email: String", apply=True)
```

### Add Field to Both Definition and Literals

```
User: "Add a timeout field with default 30 to Config struct and all its initializations"

AI uses: add_struct_field(
    "src/**/*.rs",
    "Config",
    "timeout_ms: u64",
    literal_default="30",
    apply=True
)

This updates BOTH the struct definition AND all struct literals in one go!
```

### Add Field with Positioning

```
User: "Add created_at after the name field in User"

AI uses: add_struct_field(
    "src/models/user.rs",
    "User",
    "created_at: DateTime<Utc>",
    position="after:name",
    apply=True
)
```

### Update Field Visibility

```
User: "Make the age field in User public"

AI uses: update_struct_field("src/models/user.rs", "User", "pub age: u32", apply=True)
```

## Enum Operation Examples

### Add Enum Variant

```
User: "Add an Archived variant to the Status enum"

AI uses: add_enum_variant("src/types/status.rs", "Status", "Archived", apply=True)
```

### Add Variant with Data

```
User: "Add an Error variant to Message with code and msg fields"

AI uses: add_enum_variant(
    "src/types/message.rs",
    "Message",
    "Error { code: i32, msg: String }",
    apply=True
)
```

### Rename Enum Variant

```
User: "Rename Status::Draft to Status::Pending everywhere"

AI uses: rename_enum_variant(
    "src/**/*.rs",
    "Status",
    "Draft",
    "Pending",
    apply=False  # Preview first
)

Shows diff of all changes...

User: "Looks good, apply it"

AI uses: rename_enum_variant(
    "src/**/*.rs",
    "Status",
    "Draft",
    "Pending",
    apply=True
)

Output: Successfully renamed across 23 files:
- 1 enum definition
- 15 match arms
- 42 constructor calls
- 8 other references
```

## Match Operation Examples

### Add Single Match Arm

```
User: "Add a match arm for Status::Archived in the handle_status function"

AI uses: add_match_arm(
    "src/handlers/status.rs",
    "Status::Archived",
    '"archived".to_string()',
    function="handle_status",
    apply=True
)
```

### Auto-Detect Missing Variants

```
User: "Add match arms for all missing Status variants in handle_status"

AI uses: add_match_arm(
    "src/handlers/status.rs",
    "",
    "todo!()",
    function="handle_status",
    enum_name="Status",
    auto_detect=True,
    apply=True
)

Output: Added 3 missing variants:
- Status::Pending => todo!()
- Status::Archived => todo!()
- Status::Deleted => todo!()
```

## Transform Examples

### Comment Out Debug Logs

```
User: "Comment out all eprintln! macros with [DEBUG]"

AI uses: transform(
    "src/**/*.rs",
    "macro-call",
    "comment",
    name="eprintln",
    content_filter="[DEBUG]",
    apply=True
)

Before:
eprintln!("[DEBUG] Connection established");

After:
// eprintln!("[DEBUG] Connection established");
```

### Remove Unwrap Calls

```
User: "Comment out all .unwrap() calls in the network module"

AI uses: transform(
    "src/network/**/*.rs",
    "method-call",
    "comment",
    name="unwrap",
    apply=True
)
```

### Replace Deprecated Function

```
User: "Replace all old_api_call() with new_api_call()"

AI uses: transform(
    "src/**/*.rs",
    "function-call",
    "replace",
    name="old_api_call",
    replacement="new_api_call",
    apply=True
)
```

## Derive Examples

### Add Derives to Struct

```
User: "Add Clone, Debug, and Serialize to the User struct"

AI uses: add_derive(
    "src/models/user.rs",
    "struct",
    "User",
    "Clone,Debug,Serialize",
    apply=True
)
```

### Conditional Derives

```
User: "Add Serialize to all structs that already derive Clone"

AI uses: add_derive(
    "src/**/*.rs",
    "struct",
    "Config",
    "Serialize",
    where_filter="derives_trait:Clone",
    apply=True
)
```

## History and Revert Examples

### View Recent Operations

```
User: "Show me the last 5 operations"

AI uses: show_history(5)

Output:
Recent runs (showing up to 5):

a05a626  2025-11-09 14:30  AddStructField        3 files     [can revert]
b12c789  2025-11-09 13:45  rename-enum-variant   12 files    [can revert]
c34d890  2025-11-09 12:15  add-derive            1 file      [can revert]
```

### Revert Operation

```
User: "Revert the last struct field addition"

AI uses: revert_operation("a05a626")

Output: ✓ Run a05a626 reverted successfully
```

### Force Revert

```
User: "Revert b12c789 even if files changed"

AI uses: revert_operation("b12c789", force=True)
```

## Complex Workflow Examples

### Migration Workflow

```
User: "I need to migrate from Config v1 to v2. Here's what needs to happen:
1. Rename Config to ConfigV1
2. Add new fields to Config
3. Update all initializations"

AI: I'll break this into steps and preview each:

Step 1: [Uses transform to rename]
Step 2: [Uses add_struct_field]
Step 3: [Uses add_struct_field with literal_default]

Each step shows preview, then applies after confirmation.
```

### Cleanup Workflow

```
User: "Clean up the codebase:
1. Remove all todo!() macros
2. Comment out debug prints
3. Add documentation derives"

AI: I'll handle this systematically:

1. Finding todo!() calls...
   [Uses inspect_macro_calls]
   Found 12 instances. Should I comment them out?

2. Finding debug prints...
   [Uses inspect_macro_calls with filter]
   Found 8 instances. Commenting out...

3. Adding derives...
   [Uses add_derive for each type]
```

### Refactoring Workflow

```
User: "Refactor the Status enum:
1. Add new variants: Processing, Queued
2. Update all match statements
3. Add derives for serialization"

AI:
1. Adding variants...
   [Uses add_enum_variant twice]

2. Updating matches...
   [Uses add_match_arm with auto_detect]
   Added arms to 5 functions

3. Adding derives...
   [Uses add_derive]
   Added Serialize, Deserialize to Status
```

## Error Handling Examples

### Handling Missing Files

```
User: "Add field to nonexistent.rs"

AI: Error: File not found: src/nonexistent.rs
Would you like me to:
1. List available files in src/?
2. Create the file first?
```

### Handling Syntax Errors

```
User: "Add invalid field syntax"

AI: Error: Invalid field definition: "invalid syntax here"
Field should be in format: "name: Type"
Examples:
- email: String
- age: u32
- id: Option<Uuid>
```

## Best Practices

1. **Always preview first**
   ```
   apply=False  # See changes first
   apply=True   # Apply after review
   ```

2. **Use glob patterns for bulk ops**
   ```
   "src/**/*.rs"          # All Rust files in src
   "src/models/*.rs"      # Just models
   "tests/**/*.rs"        # All test files
   ```

3. **Check history before big changes**
   ```
   show_history()  # See what was done recently
   ```

4. **Revert if something goes wrong**
   ```
   show_history()           # Find the run_id
   revert_operation("abc123")  # Undo it
   ```

5. **Use inspect before transform**
   ```
   inspect_macro_calls(...)  # See what matches
   transform(...)            # Then modify
   ```
