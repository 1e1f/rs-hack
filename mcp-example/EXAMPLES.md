# Example Interactions

This guide shows real-world examples of using rs-hack through the MCP server with Claude or other AI assistants.

## Basic Workflow Example

### Scenario: Adding a field to a struct

```
You: "I need to add an 'updated_at' field of type DateTime<Utc> to the User struct in src/models.rs"

Claude: I'll help you add that field. Let me first preview the change to make sure it looks correct.

[Claude calls: inspect_struct_literals("src/models.rs", "User")]

Claude: I can see the User struct. Let me add the updated_at field:

[Claude calls: add_struct_field(
    path="src/models.rs",
    struct_name="User", 
    field="updated_at: DateTime<Utc>",
    position="after:created_at",
    apply=False
)]

Claude: Here's what the change will look like:

```diff
pub struct User {
    id: i64,
    name: String,
    email: String,
    created_at: DateTime<Utc>,
+   updated_at: DateTime<Utc>,
}
```

Should I apply this change?

You: "Yes, and also update all the struct literals"

Claude: I'll apply the change and update the struct literals with a default value.

[Claude calls: add_struct_field(
    path="src/**/*.rs",
    struct_name="User",
    field="updated_at: DateTime<Utc>",
    position="after:created_at", 
    literal_default="Utc::now()",
    apply=True
)]

Claude: ✅ Done! I've added the field to the struct definition and updated 8 struct literals with the default value Utc::now().
```

## Advanced Workflow: Refactoring an Enum

### Scenario: Renaming and adding variants

```
You: "The Status enum has a 'Draft' variant that should be called 'Pending'. Also add a new 'Archived' variant."

Claude: I'll help you with both changes. Let me start by checking the current enum:

[Claude calls: inspect_enum_usage("src/**/*.rs", "Status::")]

Claude: I can see Status is used in 15 places across 6 files. Let me:
1. First add the Archived variant
2. Then rename Draft to Pending throughout the codebase

[Claude calls: add_enum_variant(
    path="src/types.rs",
    enum_name="Status",
    variant="Archived",
    apply=True
)]

Claude: ✅ Added the Archived variant. Now let me rename Draft to Pending:

[Claude calls: rename_enum_variant(
    path="src/**/*.rs",
    enum_name="Status",
    old_variant="Draft",
    new_variant="Pending",
    apply=True
)]

Claude: ✅ Successfully renamed Draft to Pending in:
- Enum definition (src/types.rs)
- 12 match arms across 4 files
- 3 constructor calls

Would you like me to check if all match expressions now handle the new Archived variant?

You: "Yes please"

Claude: Let me find any match expressions that might be missing the new variant:

[Claude calls: inspect_match_arms("src/**/*.rs", "Status::")]

Claude: I found 3 match expressions that don't handle Archived. Would you like me to add it with a todo!() placeholder?

You: "Yes, add them with a placeholder"

[Claude calls: add_match_arm(
    path="src/handlers/*.rs",
    pattern="",
    body="todo!(\"Handle archived status\")",
    enum_name="Status",
    auto_detect=True,
    apply=True
)]

Claude: ✅ Added Archived handling to 3 match expressions. All matches are now exhaustive!
```

## Debugging Workflow

### Scenario: Finding and removing debug code

```
You: "Find all the debug eprintln! statements in the renderer code"

Claude: I'll search for eprintln macros in the renderer:

[Claude calls: inspect_macro_calls(
    path="src/renderer/**/*.rs",
    name="eprintln",
    format="snippets"
)]

Claude: I found 23 eprintln! calls in the renderer code. Here are some examples:

```rust
// src/renderer/shadow.rs:42
eprintln!("[DEBUG] Rendering shadow with offset {:?}", offset);

// src/renderer/layout.rs:156  
eprintln!("[DEBUG] Layout bounds: {:?}", bounds);
```

Would you like me to filter for just the DEBUG messages?

You: "Yes, show me only the [DEBUG] ones"

[Claude calls: inspect_macro_calls(
    path="src/renderer/**/*.rs",
    name="eprintln",
    content_filter="[DEBUG]",
    format="locations"
)]

Claude: Found 15 DEBUG print statements:
- src/renderer/shadow.rs:42
- src/renderer/layout.rs:156
- src/renderer/paint.rs:89
- [... 12 more]

Would you like me to comment them out or remove them?

You: "Comment them out for now"

[Claude calls: transform(
    path="src/renderer/**/*.rs",
    node_type="macro-call",
    action="comment",
    name="eprintln",
    content_filter="[DEBUG]",
    apply=True
)]

Claude: ✅ Commented out 15 DEBUG print statements. They're preserved in case you need them later.
```

## Testing Workflow

### Scenario: Adding match arms for test coverage

```
You: "In the eval module, find the handle_operator function and add match arms for any missing Operator variants"

Claude: I'll find the function and auto-detect missing variants:

[Claude calls: add_match_arm(
    path="src/eval.rs",
    pattern="",
    body="todo!(\"Implement operator\")",
    function="handle_operator",
    enum_name="Operator",
    auto_detect=True,
    apply=False
)]

Claude: I found 4 missing Operator variants in handle_operator:
- Operator::BitwiseXor
- Operator::BitwiseAnd  
- Operator::LeftShift
- Operator::RightShift

Here's the preview of adding todo!() placeholders for them. Should I apply?

You: "Yes, that will help me track what needs implementing"

[Claude calls: add_match_arm(
    path="src/eval.rs",
    pattern="",
    body="todo!(\"Implement operator\")",
    function="handle_operator",
    enum_name="Operator",
    auto_detect=True,
    apply=True
)]

Claude: ✅ Added 4 match arms with todo!() placeholders. Now all Operator variants are covered.
```

## Bulk Operations

### Scenario: Adding derives across multiple structs

```
You: "Add Serialize and Deserialize derives to all Response structs in src/api/"

Claude: I'll add those derives to all structs with 'Response' in the name:

[Claude calls: inspect_struct_literals("src/api/**/*.rs", format="json")]

Claude: I found 8 Response structs:
- UserResponse
- ErrorResponse
- DataResponse
- ListResponse
- CreateResponse
- UpdateResponse
- DeleteResponse
- StatusResponse

Let me add the derives to all of them:

[Claude calls: add_derive(
    path="src/api/**/*.rs",
    target_type="struct",
    name="*Response",
    derives="Serialize,Deserialize",
    apply=True
)]

Claude: ✅ Added Serialize and Deserialize to 8 Response structs.

You: "Actually, only add them to structs that already have Debug"

Claude: I can filter by existing derives. Let me redo that:

[Claude calls: add_derive(
    path="src/api/**/*.rs",
    target_type="struct",
    name="*Response",
    derives="Serialize,Deserialize",
    where_filter="derives_trait:Debug",
    apply=True
)]

Claude: ✅ Added to 6 structs (2 were skipped because they don't derive Debug).
```

## Mistake Recovery

### Scenario: Reverting a bad change

```
You: "Add a 'cache' field to all Config structs"

Claude: [Adds field to multiple structs]

You: "Wait, I didn't want it in TestConfig, only AppConfig"

Claude: No problem, I can revert that change. Let me check the history:

[Claude calls: show_history()]

Claude: The last operation was run ID a05a626. I can revert it:

[Claude calls: revert_operation("a05a626")]

Claude: ✅ Reverted the change. Now let me add the field only to AppConfig:

[Claude calls: add_struct_field(
    path="src/config.rs",
    struct_name="AppConfig",
    field="cache: Cache",
    apply=True
)]

Claude: ✅ Added cache field only to AppConfig.
```

## Complex Refactoring

### Scenario: Migrating from String to custom error type

```
You: "I want to change all Result<T, String> to Result<T, AppError> throughout the codebase"

Claude: That's a significant refactoring. Let me help you do it systematically:

1. First, let me find all the places where Result<T, String> appears:

[Claude calls: inspect_struct_literals("src/**/*.rs", format="snippets")]

Claude: I found Result<T, String> used in:
- 23 function signatures
- 8 type aliases
- 15 impl blocks

This is complex enough that I recommend we:
1. First add AppError to the crate
2. Create a From<String> impl for AppError (for gradual migration)
3. Change function signatures one module at a time
4. Test after each module

Would you like me to start, or would you prefer to do this manually?

You: "Let's start with the user module"

Claude: I'll help guide the migration module by module. For the user module, you'll want to:
1. Update function signatures (you may need to do this manually as it's complex)
2. Change Result type aliases
3. Update error handling

Let me show you what needs changing:

[Claude calls: inspect_struct_literals("src/user.rs")]

[Continues with step-by-step guidance...]
```

## Tips for Effective Use

### Always Preview First
```
❌ Don't: Immediately apply changes
✅ Do: Preview first (apply=False), then apply

You: "Add field X"
Claude: [Shows preview first]
You: "Looks good, apply it"
Claude: [Applies]
```

### Use Glob Patterns for Bulk Ops
```
❌ Don't: Change files one by one
✅ Do: Use "src/**/*.rs" for bulk operations

You: "Add Debug to all structs in src/"
Claude: [Uses glob pattern to change all at once]
```

### Combine Inspection with Action
```
❌ Don't: Blindly apply changes
✅ Do: Inspect first, then act

You: "Add a field to Config"
Claude: 
  1. [Inspects to see current structure]
  2. [Adds field in right position]
  3. [Updates literals if needed]
```

### Use History for Safety
```
You: "Try adding X"
Claude: [Adds X]
You: "That broke tests, undo it"
Claude: [Uses revert_operation]
```

### Leverage Auto-Detect
```
❌ Don't: Manually specify each match arm
✅ Do: Use auto_detect for exhaustiveness

You: "Make sure all enum variants are handled"
Claude: [Uses add_match_arm with auto_detect=True]
```
