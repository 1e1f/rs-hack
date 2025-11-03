#!/bin/bash
# Integration test for rs-hack v0.3.0
# Tests all 16 operations + state management + advanced features

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/.."
WORKSPACE_ROOT="$(cd "$PROJECT_DIR/../../.." && pwd)"
BINARY="$WORKSPACE_ROOT/target/release/rs-hack"
# BINARY="cargo run --release --quiet --"
# BINARY="rs-hack"
INPUT="$PROJECT_DIR/examples/sample.rs"
TEMP_DIR="$PROJECT_DIR/target/test-output"
STATE_DIR="$TEMP_DIR/.rs-hack"  # State in test output directory

# Export environment variable for all commands
export RS_HACK_STATE_DIR="$STATE_DIR"

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# Test counter
PASSED=0
FAILED=0

# Cleanup and setup - clean at START of tests (not end)
echo "Setting up test environment..."
rm -rf "$TEMP_DIR"
mkdir -p "$TEMP_DIR"
# Note: State directory will be created automatically in $STATE_DIR

# Build the binary
echo "Building rs-hack..."
cd "$PROJECT_DIR" && cargo build 2>&1 | grep -v "warning:" || true

echo ""
echo "======================================"
echo "   rs-hack v0.3.0 Integration Tests  "
echo "======================================"
echo ""

# Helper function to run test
run_test() {
    local test_name="$1"
    local command="$2"
    local check_cmd="$3"
    local skip_copy="${4:-false}"

    printf "Testing %s... " "$test_name"

    # Copy input file to temp (unless skip_copy is true)
    if [ "$skip_copy" != "true" ]; then
        cp "$INPUT" "$TEMP_DIR/test.rs"
    fi

    # Run command
    if eval "$command" > "$TEMP_DIR/cmd_output.txt" 2>&1; then
        # Check result
        if eval "$check_cmd"; then
            printf "%bPASSED%b\n" "$GREEN" "$NC"
            ((PASSED++))
            return 0
        else
            printf "%bFAILED%b (check failed)\n" "$RED" "$NC"
            echo "Command output:"
            cat "$TEMP_DIR/cmd_output.txt"
            ((FAILED++))
            return 1
        fi
    else
        printf "%bFAILED%b (command failed)\n" "$RED" "$NC"
        echo "Command output:"
        cat "$TEMP_DIR/cmd_output.txt"
        ((FAILED++))
        return 1
    fi
}

# Section divider
section() {
    echo ""
    printf "${BLUE}=== %s ===${NC}\n" "$1"
    echo ""
}

# ============================================================================
section "STRUCT OPERATIONS"
# ============================================================================

# Test 1: Add struct field
run_test "add-struct-field" \
    "$BINARY add-struct-field --path $TEMP_DIR/test.rs --struct-name User --field 'email: String' --apply" \
    "grep -q 'email: String' $TEMP_DIR/test.rs"

# Test 2: Update struct field
cp "$TEMP_DIR/test.rs" "$TEMP_DIR/test_backup.rs"
run_test "update-struct-field" \
    "$BINARY update-struct-field --path $TEMP_DIR/test.rs --struct-name User --field 'pub email: String' --apply" \
    "grep -q 'pub email: String' $TEMP_DIR/test.rs" \
    "true"

# Test 3: Remove struct field
run_test "remove-struct-field" \
    "$BINARY remove-struct-field --path $TEMP_DIR/test.rs --struct-name User --field-name email --apply" \
    "! grep -q 'email' $TEMP_DIR/test.rs" \
    "true"

# Test 4: Add struct field with literal default (NEW FEATURE)
run_test "add-struct-field-with-literal-default" \
    "$BINARY add-struct-field --path $TEMP_DIR/test.rs --struct-name Config --field 'timeout: u64' --literal-default '5000' --apply" \
    "grep -q 'timeout: u64' $TEMP_DIR/test.rs"

# Test 5: Add struct literal field only (NEW OPERATION)
# First, create a file with struct literals
cat > "$TEMP_DIR/literal_test.rs" << 'EOF'
pub struct Point {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

fn create_point() -> Point {
    Point { x: 1, y: 2, z: 3 }
}

fn create_origin() -> Point {
    Point { x: 0, y: 0, z: 0 }
}
EOF

run_test "add-struct-literal-field" \
    "$BINARY add-struct-literal-field --path $TEMP_DIR/literal_test.rs --struct-name Point --field 'w: 0' --apply" \
    "grep -q 'w: 0' $TEMP_DIR/literal_test.rs" \
    "true"

# ============================================================================
section "ENUM OPERATIONS"
# ============================================================================

# Test 6: Add enum variant
run_test "add-enum-variant" \
    "$BINARY add-enum-variant --path $INPUT --enum-name Status --variant 'Archived' --output $TEMP_DIR/enum_test.rs --apply" \
    "grep -q 'Archived' $TEMP_DIR/enum_test.rs"

# Test 7: Update enum variant
cp "$TEMP_DIR/enum_test.rs" "$TEMP_DIR/test.rs"
run_test "update-enum-variant" \
    "$BINARY update-enum-variant --path $TEMP_DIR/test.rs --enum-name Status --variant 'Draft { created_at: u64 }' --apply" \
    "grep -q 'Draft.*created_at' $TEMP_DIR/test.rs" \
    "true"

# Test 8: Remove enum variant
run_test "remove-enum-variant" \
    "$BINARY remove-enum-variant --path $TEMP_DIR/test.rs --enum-name Status --variant-name Archived --apply" \
    "! grep -q '^[[:space:]]*Archived' $TEMP_DIR/test.rs" \
    "true"

# ============================================================================
section "MATCH OPERATIONS"
# ============================================================================

# Test 9: Add match arm
cp "$INPUT" "$TEMP_DIR/match_test.rs"
run_test "add-match-arm" \
    "$BINARY add-match-arm --path $TEMP_DIR/match_test.rs --pattern 'Status::Archived' --body '\"archived\".to_string()' --function handle_status --apply" \
    "grep -q 'Archived' $TEMP_DIR/match_test.rs" \
    "true"

# Test 10: Update match arm
run_test "update-match-arm" \
    "$BINARY update-match-arm --path $TEMP_DIR/match_test.rs --pattern 'Status::Draft' --body '\"pending\".to_string()' --function handle_status --apply" \
    "grep -q 'pending' $TEMP_DIR/match_test.rs" \
    "true"

# Test 11: Remove match arm
run_test "remove-match-arm" \
    "$BINARY remove-match-arm --path $TEMP_DIR/match_test.rs --pattern 'Status::Deleted' --function handle_status --apply" \
    "! grep -q 'Status.*::.*Deleted.*=>' $TEMP_DIR/match_test.rs" \
    "true"

# Test 12: Auto-detect missing match arms (NEW FEATURE)
cat > "$TEMP_DIR/auto_detect_test.rs" << 'EOF'
pub enum Status {
    Draft,
    Published,
    Archived,
    Pending,
}

pub fn handle_status(status: Status) -> String {
    match status {
        Status::Draft => "draft".to_string(),
    }
}
EOF

run_test "auto-detect-missing-match-arms" \
    "$BINARY add-match-arm --path $TEMP_DIR/auto_detect_test.rs --auto-detect --enum-name Status --body 'todo!()' --function handle_status --apply" \
    "grep -q 'Status::Published' $TEMP_DIR/auto_detect_test.rs && grep -q 'Status::Archived' $TEMP_DIR/auto_detect_test.rs && grep -q 'Status::Pending' $TEMP_DIR/auto_detect_test.rs" \
    "true"

# ============================================================================
section "CODE ORGANIZATION"
# ============================================================================

# Test 13: Add derive macros
cp "$INPUT" "$TEMP_DIR/derive_test.rs"
run_test "add-derive" \
    "$BINARY add-derive --path $TEMP_DIR/derive_test.rs --target-type struct --name User --derives 'Clone,Serialize' --apply" \
    "grep -q 'Clone' $TEMP_DIR/derive_test.rs && grep -q 'Serialize' $TEMP_DIR/derive_test.rs" \
    "true"

# Test 14: Add impl method
cp "$INPUT" "$TEMP_DIR/impl_test.rs"
run_test "add-impl-method" \
    "$BINARY add-impl-method --path $TEMP_DIR/impl_test.rs --target User --method 'pub fn get_id(&self) -> u64 { self.id }' --apply" \
    "grep -q 'get_id' $TEMP_DIR/impl_test.rs" \
    "true"

# Test 15: Add use statement
run_test "add-use" \
    "$BINARY add-use --path $TEMP_DIR/impl_test.rs --use-path 'serde::Serialize' --apply" \
    "grep -q 'use serde::Serialize' $TEMP_DIR/impl_test.rs" \
    "true"

# ============================================================================
section "DIFF OUTPUT FORMAT"
# ============================================================================

# Test 16: Diff output format (NEW FEATURE)
run_test "diff-output-format" \
    "$BINARY add-struct-field --path $INPUT --struct-name User --field 'created_at: u64' --format diff > $TEMP_DIR/diff.patch" \
    "grep -q '^---' $TEMP_DIR/diff.patch && grep -q '^+++' $TEMP_DIR/diff.patch && grep -q '+.*created_at: u64' $TEMP_DIR/diff.patch" \
    "true"

# Test 17: Diff output with multiple changes
run_test "diff-output-enum-variant" \
    "$BINARY add-enum-variant --path $INPUT --enum-name Status --variant 'Pending' --format diff > $TEMP_DIR/enum_diff.patch" \
    "grep -q '^+.*Pending' $TEMP_DIR/enum_diff.patch" \
    "true"

# ============================================================================
section "STATE MANAGEMENT & REVERT"
# ============================================================================

# Test 18: State tracking (using RS_HACK_STATE_DIR environment variable)
run_test "state-tracking" \
    "$BINARY add-struct-field --path $TEMP_DIR/test.rs --struct-name User --field 'tracked_field: bool' --apply" \
    "[ -d $STATE_DIR ] && [ -f $STATE_DIR/runs.json ]" \
    "true"

# Test 19: History command
run_test "history-display" \
    "$BINARY history > $TEMP_DIR/history_output.txt" \
    "grep -q 'AddStructField' $TEMP_DIR/history_output.txt" \
    "true"

# Test 20: Revert operation
# First, add a field that we'll revert
cp "$INPUT" "$TEMP_DIR/revert_test.rs"
$BINARY add-struct-field --path $TEMP_DIR/revert_test.rs --struct-name User --field 'temp_field: String' --apply > "$TEMP_DIR/revert_run_output.txt"

# Extract run ID from output (BSD grep compatible)
RUN_ID=$(grep -o 'Run ID: [a-z0-9]*' "$TEMP_DIR/revert_run_output.txt" | sed 's/Run ID: //' || echo "")

if [ -n "$RUN_ID" ]; then
    run_test "revert-operation" \
        "$BINARY revert $RUN_ID" \
        "! grep -q 'temp_field: String' $TEMP_DIR/revert_test.rs" \
        "true"
else
    printf "Testing revert-operation... %bSKIPPED%b (could not extract run ID)\n" "$YELLOW" "$NC"
fi

# Test 21: Clean old state
run_test "clean-old-state" \
    "$BINARY clean --keep-days 0 > $TEMP_DIR/clean_output.txt" \
    "true" \
    "true"

# ============================================================================
section "ADVANCED FEATURES"
# ============================================================================

# Test 22: Glob pattern support (NEW FEATURE)
mkdir -p "$TEMP_DIR/src/models"
cp "$INPUT" "$TEMP_DIR/src/models/user.rs"
cp "$INPUT" "$TEMP_DIR/src/models/post.rs"

run_test "glob-pattern-support" \
    "$BINARY add-derive --path '$TEMP_DIR/src/**/*.rs' --target-type struct --name User --derives 'Clone' --apply" \
    "grep -q '#\[derive.*Clone' $TEMP_DIR/src/models/user.rs && grep -q '#\[derive.*Clone' $TEMP_DIR/src/models/post.rs" \
    "true"

# Test 23: Idempotency - running same command twice should not error
cp "$INPUT" "$TEMP_DIR/idempotent_test.rs"
$BINARY add-struct-field --path $TEMP_DIR/idempotent_test.rs --struct-name User --field 'test_field: i32' --apply > /dev/null 2>&1

run_test "idempotency-add-struct-field" \
    "$BINARY add-struct-field --path $TEMP_DIR/idempotent_test.rs --struct-name User --field 'test_field: i32' --apply" \
    "grep -q 'test_field: i32' $TEMP_DIR/idempotent_test.rs" \
    "true"

# Test 24: Idempotency for enum variants
cp "$INPUT" "$TEMP_DIR/idempotent_enum_test.rs"
$BINARY add-enum-variant --path $TEMP_DIR/idempotent_enum_test.rs --enum-name Status --variant 'Testing' --apply > /dev/null 2>&1

run_test "idempotency-add-enum-variant" \
    "$BINARY add-enum-variant --path $TEMP_DIR/idempotent_enum_test.rs --enum-name Status --variant 'Testing' --apply" \
    "grep -q 'Testing' $TEMP_DIR/idempotent_enum_test.rs" \
    "true"

# Test 25: Position control - after
cp "$INPUT" "$TEMP_DIR/position_test.rs"
$BINARY add-struct-field --path $TEMP_DIR/position_test.rs --struct-name User --field 'middle_name: Option<String>' --position 'after:id' --apply > /dev/null 2>&1

run_test "position-control-after" \
    "grep -A 1 'id: u64' $TEMP_DIR/position_test.rs | grep -q 'middle_name'" \
    "true" \
    "true"

# Test 26: Find operation (utility)
run_test "find-struct-location" \
    "$BINARY find --path $INPUT --node-type struct --name User > $TEMP_DIR/find_output.json" \
    "grep -q 'line' $TEMP_DIR/find_output.json" \
    "true"

# Test 27: Batch operations (utility)
# Create a test directory with the file
mkdir -p "$TEMP_DIR/batch_dir"
cp "$INPUT" "$TEMP_DIR/batch_dir/test.rs"

# Create a batch spec
cat > "$TEMP_DIR/batch_spec.json" <<EOF
{
  "base_path": "$TEMP_DIR/batch_dir",
  "operations": [
    {
      "type": "AddStructField",
      "struct_name": "User",
      "field_def": "batch_test: bool",
      "position": "Last"
    },
    {
      "type": "AddEnumVariant",
      "enum_name": "Status",
      "variant_def": "BatchTesting",
      "position": "Last"
    }
  ]
}
EOF

run_test "batch-operations" \
    "$BINARY batch --spec $TEMP_DIR/batch_spec.json --apply" \
    "grep -q 'batch_test: bool' $TEMP_DIR/batch_dir/test.rs && grep -q 'BatchTesting' $TEMP_DIR/batch_dir/test.rs" \
    "true"

# ============================================================================
section "INSPECT & TRANSFORM OPERATIONS (NEW)"
# ============================================================================

# Test 28: Inspect macro-call (NEW FEATURE)
cat > "$TEMP_DIR/inspect_macro_test.rs" << 'EOF'
fn main() {
    println!("Hello");
    eprintln!("[DEBUG] Some debug message");
    eprintln!("[SHADOW RENDER] Drawing shadow");
    eprintln!("[SHADOW RENDER] Shadow blur");
    todo!("Implement this");
}
EOF

run_test "inspect-macro-call" \
    "$BINARY inspect --path $TEMP_DIR/inspect_macro_test.rs --node-type macro-call --name eprintln --format locations" \
    "grep -q 'inspect_macro_test.rs:3:4' $TEMP_DIR/cmd_output.txt && grep -q 'inspect_macro_test.rs:4:4' $TEMP_DIR/cmd_output.txt && grep -q 'inspect_macro_test.rs:5:4' $TEMP_DIR/cmd_output.txt" \
    "true"

# Test 29: Inspect with content filter (NEW FEATURE)
run_test "inspect-content-filter" \
    "$BINARY inspect --path $TEMP_DIR/inspect_macro_test.rs --node-type macro-call --name eprintln --content-filter '[SHADOW RENDER]' --format locations" \
    "grep -q 'inspect_macro_test.rs:4:4' $TEMP_DIR/cmd_output.txt && grep -q 'inspect_macro_test.rs:5:4' $TEMP_DIR/cmd_output.txt && ! grep -q 'inspect_macro_test.rs:3:4' $TEMP_DIR/cmd_output.txt" \
    "true"

# Test 30: Transform comment action (NEW FEATURE)
cat > "$TEMP_DIR/transform_comment_test.rs" << 'EOF'
fn main() {
    eprintln!("[DEBUG] Debug message");
    eprintln!("[SHADOW RENDER] Shadow 1");
    println!("Keep this");
    eprintln!("[SHADOW RENDER] Shadow 2");
}
EOF

run_test "transform-comment-action" \
    "$BINARY transform --path $TEMP_DIR/transform_comment_test.rs --node-type macro-call --name eprintln --content-filter '[SHADOW RENDER]' --action comment --apply" \
    "grep -q '// eprintln!.*SHADOW RENDER' $TEMP_DIR/transform_comment_test.rs && grep -q 'eprintln!.*DEBUG' $TEMP_DIR/transform_comment_test.rs && ! grep -q '// eprintln!.*DEBUG' $TEMP_DIR/transform_comment_test.rs" \
    "true"

# Test 31: Transform remove action (NEW FEATURE)
cat > "$TEMP_DIR/transform_remove_test.rs" << 'EOF'
fn main() {
    eprintln!("[DEBUG] Debug message");
    eprintln!("[SHADOW RENDER] Shadow 1");
    println!("Keep this");
    eprintln!("[SHADOW RENDER] Shadow 2");
}
EOF

run_test "transform-remove-action" \
    "$BINARY transform --path $TEMP_DIR/transform_remove_test.rs --node-type macro-call --name eprintln --content-filter '[SHADOW RENDER]' --action remove --apply" \
    "! grep -q '\[SHADOW RENDER\]' $TEMP_DIR/transform_remove_test.rs && grep -q '\[DEBUG\]' $TEMP_DIR/transform_remove_test.rs" \
    "true"

# Test 32: Transform replace action (NEW FEATURE)
cat > "$TEMP_DIR/transform_replace_test.rs" << 'EOF'
fn process() {
    old_function();
    old_function();
    some_other_function();
}
EOF

run_test "transform-replace-action" \
    "$BINARY transform --path $TEMP_DIR/transform_replace_test.rs --node-type function-call --name old_function --action replace --with 'new_function()' --apply" \
    "grep -q 'new_function()' $TEMP_DIR/transform_replace_test.rs && ! grep -q 'old_function()' $TEMP_DIR/transform_replace_test.rs && grep -q 'some_other_function()' $TEMP_DIR/transform_replace_test.rs" \
    "true"

# Test 33: Transform with method-call node type (NEW FEATURE)
cat > "$TEMP_DIR/transform_method_test.rs" << 'EOF'
fn risky_code() {
    let x = maybe_value.unwrap();
    let y = safe_value.clone();
    let z = another_value.unwrap();
}
EOF

run_test "transform-method-call-comment" \
    "$BINARY transform --path $TEMP_DIR/transform_method_test.rs --node-type method-call --name unwrap --action comment --apply" \
    "grep -q '// .*unwrap()' $TEMP_DIR/transform_method_test.rs && ! grep -q '// .*clone()' $TEMP_DIR/transform_method_test.rs" \
    "true"

# ============================================================================
# SUMMARY
# ============================================================================

echo ""
echo "======================================"
echo "          Test Summary                "
echo "======================================"
printf "Passed: %b%s%b\n" "$GREEN" "$PASSED" "$NC"
printf "Failed: %b%s%b\n" "$RED" "$FAILED" "$NC"
echo "======================================"
echo ""

if [ $FAILED -eq 0 ]; then
    printf "%bAll tests passed! 🎉%b\n" "$GREEN" "$NC"
    echo ""
    echo "Test coverage:"
    echo "  ✅ 5 Struct operations (add, update, remove, literal, literal-default)"
    echo "  ✅ 3 Enum operations (add, update, remove)"
    echo "  ✅ 4 Match operations (add, update, remove, auto-detect)"
    echo "  ✅ 3 Code organization (derive, impl, use)"
    echo "  ✅ 2 Diff output tests"
    echo "  ✅ 4 State management (tracking, history, revert, clean)"
    echo "  ✅ 2 Idempotency tests"
    echo "  ✅ 1 Position control test"
    echo "  ✅ 1 Glob pattern test"
    echo "  ✅ 2 Utility operations (find, batch)"
    echo "  ✅ 6 Inspect & Transform operations (inspect, filter, comment, remove, replace, method-call) ⭐ NEW"
    echo ""
    printf "Total: %b33 tests%b\n" "$BLUE" "$NC"

    # STATE AUDIT
    section "STATE AUDIT"
    echo "State artifacts saved in: $STATE_DIR"
    echo ""
    if [ -f "$STATE_DIR/runs.json" ]; then
        echo "You can inspect state after tests:"
        echo "  # View runs.json"
        echo "  cat $STATE_DIR/runs.json"
        echo ""
        echo "  # View history with rs-hack"
        echo "  RS_HACK_STATE_DIR=$STATE_DIR cargo run --quiet -- history"
        echo ""
        echo "  # List backup files"
        echo "  ls -la $STATE_DIR/*/"
    else
        echo "No state artifacts were created during tests."
    fi

    exit 0
else
    printf "%bSome tests failed. Please review the output above.%b\n" "$RED" "$NC"
    exit 1
fi
