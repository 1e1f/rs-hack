#!/bin/bash
# Integration test for rs-hack v0.5.0
# Tests all operations + state management + advanced features + transform/inspect + rename + path resolution + discovery mode + variant filtering

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/.."
BINARY="$PROJECT_DIR/target/release/rs-hack"
# Alternative: BINARY="cargo run --release --quiet --"
# Alternative: BINARY="rs-hack"  # if installed globally
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
echo "   rs-hack v0.5.0 Integration Tests  "
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
    "$BINARY add-struct-field --paths $TEMP_DIR/test.rs --struct-name User --field 'email: String' --apply" \
    "grep -q 'email: String' $TEMP_DIR/test.rs"

# Test 2: Update struct field
cp "$TEMP_DIR/test.rs" "$TEMP_DIR/test_backup.rs"
run_test "update-struct-field" \
    "$BINARY update-struct-field --paths $TEMP_DIR/test.rs --struct-name User --field 'pub email: String' --apply" \
    "grep -q 'pub email: String' $TEMP_DIR/test.rs" \
    "true"

# Test 3: Remove struct field
run_test "remove-struct-field" \
    "$BINARY remove-struct-field --paths $TEMP_DIR/test.rs --struct-name User --field-name email --apply" \
    "! grep -q 'email' $TEMP_DIR/test.rs" \
    "true"

# Test 4: Add struct field with literal default (NEW FEATURE)
run_test "add-struct-field-with-literal-default" \
    "$BINARY add-struct-field --paths $TEMP_DIR/test.rs --struct-name Config --field 'timeout: u64' --literal-default '5000' --apply" \
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

run_test "add-struct-field-literal-only" \
    "$BINARY add-struct-field --paths $TEMP_DIR/literal_test.rs --struct-name Point --field 'w' --literal-default '0' --apply" \
    "grep -q 'w: 0' $TEMP_DIR/literal_test.rs && ! (grep -A 5 'pub struct Point' $TEMP_DIR/literal_test.rs | grep -q 'w:')" \
    "true"

# ============================================================================
section "ENUM OPERATIONS"
# ============================================================================

# Test 6: Add enum variant
run_test "add-enum-variant" \
    "$BINARY add-enum-variant --paths $INPUT --enum-name Status --variant 'Archived' --output $TEMP_DIR/enum_test.rs --apply" \
    "grep -q 'Archived' $TEMP_DIR/enum_test.rs"

# Test 7: Update enum variant
cp "$TEMP_DIR/enum_test.rs" "$TEMP_DIR/test.rs"
run_test "update-enum-variant" \
    "$BINARY update-enum-variant --paths $TEMP_DIR/test.rs --enum-name Status --variant 'Draft { created_at: u64 }' --apply" \
    "grep -q 'Draft.*created_at' $TEMP_DIR/test.rs" \
    "true"

# Test 8: Remove enum variant
run_test "remove-enum-variant" \
    "$BINARY remove-enum-variant --paths $TEMP_DIR/test.rs --enum-name Status --variant-name Archived --apply" \
    "! grep -q '^[[:space:]]*Archived' $TEMP_DIR/test.rs" \
    "true"

# ============================================================================
section "ENUM RENAME OPERATIONS (NEW v0.4.2)"
# ============================================================================

# Test 8a: Rename enum variant - basic
cat > "$TEMP_DIR/rename_basic.rs" << 'EOF'
enum Status {
    Draft,
    Published,
}

fn process(s: Status) {
    match s {
        Status::Draft => println!("draft"),
        Status::Published => println!("pub"),
    }
}
EOF

run_test "rename-enum-variant-basic" \
    "$BINARY rename-enum-variant --paths $TEMP_DIR/rename_basic.rs --enum-name Status --old-variant Draft --new-variant Pending --apply" \
    "grep -q 'Pending' $TEMP_DIR/rename_basic.rs && grep -q 'Status::Pending' $TEMP_DIR/rename_basic.rs && ! grep -q 'Draft' $TEMP_DIR/rename_basic.rs" \
    "true"

# Test 8b: Rename enum variant with qualified paths (NEW --enum-path flag)
cat > "$TEMP_DIR/rename_qualified.rs" << 'EOF'
pub mod types {
    pub enum Status {
        Draft,
        Published,
    }
}

use types::Status;

fn process(s: Status) {
    let x = types::Status::Draft;
    match s {
        Status::Draft => println!("draft"),
        Status::Published => println!("pub"),
    }
}
EOF

run_test "rename-enum-variant-with-enum-path" \
    "$BINARY rename-enum-variant --paths $TEMP_DIR/rename_qualified.rs --enum-name Status --old-variant Draft --new-variant Pending --enum-path 'types::Status' --apply" \
    "grep -q 'types::Status::Pending' $TEMP_DIR/rename_qualified.rs && grep -q 'Status::Pending =>' $TEMP_DIR/rename_qualified.rs && ! grep -q 'Draft' $TEMP_DIR/rename_qualified.rs" \
    "true"

# Test 8c: Surgical edit mode preserves formatting (NEW --edit-mode flag)
cat > "$TEMP_DIR/rename_surgical.rs" << 'EOF'
enum Status {
    Draft,
    Published,
}

fn process(s: Status) {
    match s {
        Status::Draft => println!("draft"),


        Status::Published => println!("pub"),
    }
}
EOF

# Save original for comparison
cp "$TEMP_DIR/rename_surgical.rs" "$TEMP_DIR/rename_surgical_orig.rs"

run_test "rename-enum-variant-surgical-mode" \
    "$BINARY rename-enum-variant --paths $TEMP_DIR/rename_surgical.rs --enum-name Status --old-variant Draft --new-variant Pending --edit-mode surgical --apply" \
    "grep -q 'Pending' $TEMP_DIR/rename_surgical.rs && grep -A 4 'Status::Pending =>' $TEMP_DIR/rename_surgical.rs | grep -c '^$' | grep -q '[2-9]'" \
    "true"

# Test 8d: Reformat mode changes formatting
cat > "$TEMP_DIR/rename_reformat.rs" << 'EOF'
enum Status {
    Draft,
    Published,
}

fn process(s: Status) {
    match s {
        Status::Draft => println!("draft"),


        Status::Published => println!("pub"),
    }
}
EOF

run_test "rename-enum-variant-reformat-mode" \
    "$BINARY rename-enum-variant --paths $TEMP_DIR/rename_reformat.rs --enum-name Status --old-variant Draft --new-variant Pending --edit-mode reformat --apply" \
    "grep -q 'Pending' $TEMP_DIR/rename_reformat.rs && ! (grep -A 3 'Status::Pending =>' $TEMP_DIR/rename_reformat.rs | grep -q '^$')" \
    "true"

# ============================================================================
section "FUNCTION RENAME OPERATIONS (NEW v0.4.2)"
# ============================================================================

# Test 8e: Rename function - basic
cat > "$TEMP_DIR/rename_func_basic.rs" << 'EOF'
fn process_v2(x: i32) -> i32 {
    x * 2
}

fn main() {
    let result = process_v2(5);
    let f = process_v2;
    println!("{}", result);
}
EOF

run_test "rename-function-basic" \
    "$BINARY rename-function --paths $TEMP_DIR/rename_func_basic.rs --old-name process_v2 --new-name process --apply" \
    "grep -q 'fn process' $TEMP_DIR/rename_func_basic.rs && grep -q 'process(5)' $TEMP_DIR/rename_func_basic.rs && ! grep -q 'process_v2' $TEMP_DIR/rename_func_basic.rs" \
    "true"

# Test 8f: Rename function with surgical mode preserves formatting
cat > "$TEMP_DIR/rename_func_surgical.rs" << 'EOF'
fn helper_v2() {
    println!("help");
}

fn main() {


    helper_v2();
}
EOF

run_test "rename-function-surgical-mode" \
    "$BINARY rename-function --paths $TEMP_DIR/rename_func_surgical.rs --old-name helper_v2 --new-name helper --edit-mode surgical --apply" \
    "grep -q 'fn helper' $TEMP_DIR/rename_func_surgical.rs && grep -A 4 'fn main' $TEMP_DIR/rename_func_surgical.rs | grep -c '^$' | grep -q '[2-9]'" \
    "true"

# ============================================================================
section "MATCH OPERATIONS"
# ============================================================================

# Test 9: Add match arm
cp "$INPUT" "$TEMP_DIR/match_test.rs"
run_test "add-match-arm" \
    "$BINARY add-match-arm --paths $TEMP_DIR/match_test.rs --pattern 'Status::Archived' --body '\"archived\".to_string()' --function handle_status --apply" \
    "grep -q 'Archived' $TEMP_DIR/match_test.rs" \
    "true"

# Test 10: Update match arm
run_test "update-match-arm" \
    "$BINARY update-match-arm --paths $TEMP_DIR/match_test.rs --pattern 'Status::Draft' --body '\"pending\".to_string()' --function handle_status --apply" \
    "grep -q 'pending' $TEMP_DIR/match_test.rs" \
    "true"

# Test 11: Remove match arm
run_test "remove-match-arm" \
    "$BINARY remove-match-arm --paths $TEMP_DIR/match_test.rs --pattern 'Status::Deleted' --function handle_status --apply" \
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
    "$BINARY add-match-arm --paths $TEMP_DIR/auto_detect_test.rs --auto-detect --enum-name Status --body 'todo!()' --function handle_status --apply" \
    "grep -q 'Status::Published' $TEMP_DIR/auto_detect_test.rs && grep -q 'Status::Archived' $TEMP_DIR/auto_detect_test.rs && grep -q 'Status::Pending' $TEMP_DIR/auto_detect_test.rs" \
    "true"

# ============================================================================
section "CODE ORGANIZATION"
# ============================================================================

# Test 13: Add derive macros
cp "$INPUT" "$TEMP_DIR/derive_test.rs"
run_test "add-derive" \
    "$BINARY add-derive --paths $TEMP_DIR/derive_test.rs --target-type struct --name User --derives 'Clone,Serialize' --apply" \
    "grep -q 'Clone' $TEMP_DIR/derive_test.rs && grep -q 'Serialize' $TEMP_DIR/derive_test.rs" \
    "true"

# Test 14: Add impl method
cp "$INPUT" "$TEMP_DIR/impl_test.rs"
run_test "add-impl-method" \
    "$BINARY add-impl-method --paths $TEMP_DIR/impl_test.rs --target User --method 'pub fn get_id(&self) -> u64 { self.id }' --apply" \
    "grep -q 'get_id' $TEMP_DIR/impl_test.rs" \
    "true"

# Test 15: Add use statement
run_test "add-use" \
    "$BINARY add-use --paths $TEMP_DIR/impl_test.rs --use-path 'serde::Serialize' --apply" \
    "grep -q 'use serde::Serialize' $TEMP_DIR/impl_test.rs" \
    "true"

# ============================================================================
section "DIFF OUTPUT FORMAT"
# ============================================================================

# Test 16: Diff output format (NEW FEATURE)
run_test "diff-output-format" \
    "$BINARY add-struct-field --paths $INPUT --struct-name User --field 'created_at: u64' --format diff > $TEMP_DIR/diff.patch" \
    "grep -q '^---' $TEMP_DIR/diff.patch && grep -q '^+++' $TEMP_DIR/diff.patch && grep -q '+.*created_at: u64' $TEMP_DIR/diff.patch" \
    "true"

# Test 17: Diff output with multiple changes
run_test "diff-output-enum-variant" \
    "$BINARY add-enum-variant --paths $INPUT --enum-name Status --variant 'Pending' --format diff > $TEMP_DIR/enum_diff.patch" \
    "grep -q '^+.*Pending' $TEMP_DIR/enum_diff.patch" \
    "true"

# ============================================================================
section "STATE MANAGEMENT & REVERT"
# ============================================================================

# Test 18: State tracking (using RS_HACK_STATE_DIR environment variable)
run_test "state-tracking" \
    "$BINARY add-struct-field --paths $TEMP_DIR/test.rs --struct-name User --field 'tracked_field: bool' --apply" \
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
$BINARY add-struct-field --paths $TEMP_DIR/revert_test.rs --struct-name User --field 'temp_field: String' --apply > "$TEMP_DIR/revert_run_output.txt"

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
    "$BINARY add-derive --paths '$TEMP_DIR/src/**/*.rs' --target-type struct --name User --derives 'Clone' --apply" \
    "grep -q '#\[derive.*Clone' $TEMP_DIR/src/models/user.rs && grep -q '#\[derive.*Clone' $TEMP_DIR/src/models/post.rs" \
    "true"

# Test 23: Idempotency - running same command twice should not error
cp "$INPUT" "$TEMP_DIR/idempotent_test.rs"
$BINARY add-struct-field --paths $TEMP_DIR/idempotent_test.rs --struct-name User --field 'test_field: i32' --apply > /dev/null 2>&1

run_test "idempotency-add-struct-field" \
    "$BINARY add-struct-field --paths $TEMP_DIR/idempotent_test.rs --struct-name User --field 'test_field: i32' --apply" \
    "grep -q 'test_field: i32' $TEMP_DIR/idempotent_test.rs" \
    "true"

# Test 24: Idempotency for enum variants
cp "$INPUT" "$TEMP_DIR/idempotent_enum_test.rs"
$BINARY add-enum-variant --paths $TEMP_DIR/idempotent_enum_test.rs --enum-name Status --variant 'Testing' --apply > /dev/null 2>&1

run_test "idempotency-add-enum-variant" \
    "$BINARY add-enum-variant --paths $TEMP_DIR/idempotent_enum_test.rs --enum-name Status --variant 'Testing' --apply" \
    "grep -q 'Testing' $TEMP_DIR/idempotent_enum_test.rs" \
    "true"

# Test 25: Position control - after
cp "$INPUT" "$TEMP_DIR/position_test.rs"
$BINARY add-struct-field --paths $TEMP_DIR/position_test.rs --struct-name User --field 'middle_name: Option<String>' --position 'after:id' --apply > /dev/null 2>&1

run_test "position-control-after" \
    "grep -A 1 'id: u64' $TEMP_DIR/position_test.rs | grep -q 'middle_name'" \
    "true" \
    "true"

# Test 26: Find operation (utility)
run_test "find-struct-location" \
    "$BINARY find --paths $INPUT --node-type struct --name User --format json > $TEMP_DIR/find_output.json" \
    "grep -q 'location' $TEMP_DIR/find_output.json" \
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
section "FIND & TRANSFORM OPERATIONS (NEW)"
# ============================================================================

# Test 28: Find macro-call (NEW FEATURE)
cat > "$TEMP_DIR/find_macro_test.rs" << 'EOF'
fn main() {
    println!("Hello");
    eprintln!("[DEBUG] Some debug message");
    eprintln!("[SHADOW RENDER] Drawing shadow");
    eprintln!("[SHADOW RENDER] Shadow blur");
    todo!("Implement this");
}
EOF

run_test "find-macro-call" \
    "$BINARY find --paths $TEMP_DIR/find_macro_test.rs --node-type macro-call --name eprintln --format locations" \
    "grep -q 'find_macro_test.rs:3:4' $TEMP_DIR/cmd_output.txt && grep -q 'find_macro_test.rs:4:4' $TEMP_DIR/cmd_output.txt && grep -q 'find_macro_test.rs:5:4' $TEMP_DIR/cmd_output.txt" \
    "true"

# Test 29: Find with content filter (NEW FEATURE)
run_test "find-content-filter" \
    "$BINARY find --paths $TEMP_DIR/find_macro_test.rs --node-type macro-call --name eprintln --content-filter '[SHADOW RENDER]' --format locations" \
    "grep -q 'find_macro_test.rs:4:4' $TEMP_DIR/cmd_output.txt && grep -q 'find_macro_test.rs:5:4' $TEMP_DIR/cmd_output.txt && ! grep -q 'find_macro_test.rs:3:4' $TEMP_DIR/cmd_output.txt" \
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
    "$BINARY transform --paths $TEMP_DIR/transform_comment_test.rs --node-type macro-call --name eprintln --content-filter '[SHADOW RENDER]' --action comment --apply" \
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
    "$BINARY transform --paths $TEMP_DIR/transform_remove_test.rs --node-type macro-call --name eprintln --content-filter '[SHADOW RENDER]' --action remove --apply" \
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
    "$BINARY transform --paths $TEMP_DIR/transform_replace_test.rs --node-type function-call --name old_function --action replace --with 'new_function()' --apply" \
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
    "$BINARY transform --paths $TEMP_DIR/transform_method_test.rs --node-type method-call --name unwrap --action comment --apply" \
    "grep -q '// .*unwrap()' $TEMP_DIR/transform_method_test.rs && ! grep -q '// .*clone()' $TEMP_DIR/transform_method_test.rs" \
    "true"

# ============================================================================
section "PATTERN MATCHING FOR STRUCT LITERALS (NEW)"
# ============================================================================

# Test 34: Pattern matching - pure struct literal only (no ::)
cat > "$TEMP_DIR/pattern_pure_struct.rs" << 'EOF'
struct Rectangle { width: f32, height: f32 }

enum View {
    Rectangle { width: f32, height: f32 },
    Circle { radius: f32 },
}

fn test() {
    let rect = Rectangle { width: 10.0, height: 20.0 };
    let view = View::Rectangle { width: 30.0, height: 40.0 };
}
EOF

run_test "pattern-pure-struct-literal" \
    "$BINARY add-struct-literal-field --paths $TEMP_DIR/pattern_pure_struct.rs --struct-name Rectangle --field 'layer: 0' --apply" \
    "grep -A 5 'let rect = Rectangle' $TEMP_DIR/pattern_pure_struct.rs | grep -q 'layer: 0' && ! grep -A 5 'View::Rectangle' $TEMP_DIR/pattern_pure_struct.rs | grep -q 'layer'" \
    "true"

# Test 35: Pattern matching - wildcard (*::Rectangle matches all)
cat > "$TEMP_DIR/pattern_wildcard.rs" << 'EOF'
struct Rectangle { width: f32, height: f32 }

enum View {
    Rectangle { width: f32, height: f32 },
}

enum ViewType {
    Rectangle { width: f32, height: f32 },
}

fn test() {
    let rect = Rectangle { width: 10.0, height: 20.0 };
    let view = View::Rectangle { width: 30.0, height: 40.0 };
    let vtype = ViewType::Rectangle { width: 50.0, height: 60.0 };
}
EOF

run_test "pattern-wildcard-match" \
    "$BINARY add-struct-literal-field --paths $TEMP_DIR/pattern_wildcard.rs --struct-name '*::Rectangle' --field 'layer: 0' --apply" \
    "grep -A 5 'let rect = Rectangle' $TEMP_DIR/pattern_wildcard.rs | grep -q 'layer: 0' && grep -A 5 'let view = View::Rectangle' $TEMP_DIR/pattern_wildcard.rs | grep -q 'layer: 0' && grep -A 5 'let vtype = ViewType::Rectangle' $TEMP_DIR/pattern_wildcard.rs | grep -q 'layer: 0'" \
    "true"

# Test 36: Pattern matching - exact path (View::Rectangle only)
cat > "$TEMP_DIR/pattern_exact.rs" << 'EOF'
struct Rectangle { width: f32, height: f32 }

enum View {
    Rectangle { width: f32, height: f32 },
}

enum ViewType {
    Rectangle { width: f32, height: f32 },
}

fn test() {
    let rect = Rectangle { width: 10.0, height: 20.0 };
    let view = View::Rectangle { width: 30.0, height: 40.0 };
    let vtype = ViewType::Rectangle { width: 50.0, height: 60.0 };
}
EOF

run_test "pattern-exact-path-match" \
    "$BINARY add-struct-literal-field --paths $TEMP_DIR/pattern_exact.rs --struct-name 'View::Rectangle' --field 'layer: 0' --apply" \
    "! (grep -A 5 'let rect = Rectangle' $TEMP_DIR/pattern_exact.rs | grep -q 'layer') && grep -A 5 'let view = View::Rectangle' $TEMP_DIR/pattern_exact.rs | grep -q 'layer: 0' && ! (grep -A 5 'let vtype = ViewType::Rectangle' $TEMP_DIR/pattern_exact.rs | grep -q 'layer')" \
    "true"

# ============================================================================
section "VALIDATION MODE (Sprint 2, Issue 6)"
# ============================================================================

# Create test file with enum variant references
cat > "$TEMP_DIR/validate_enum.rs" << 'EOF'
pub enum Status {
    Draft,
    Published,
    Archived,
}

pub fn process(s: Status) {
    match s {
        Status::Draft => println!("draft"),
        Status::Published => println!("published"),
        Status::Archived => println!("archived"),
    }
}

pub fn check_draft(s: &Status) -> bool {
    matches!(s, Status::Draft)
}
EOF

# Test: Validation mode should detect all references before rename
run_test "validation-mode-enum-before-rename" \
    "$BINARY rename-enum-variant --paths $TEMP_DIR/validate_enum.rs --enum-name Status --old-variant Draft --new-variant Pending --validate" \
    "grep -q 'Found.*remaining references' $TEMP_DIR/cmd_output.txt && grep -q 'Status::Draft' $TEMP_DIR/cmd_output.txt" \
    "true"

# Test: Rename enum variant and then validate again
cp "$TEMP_DIR/validate_enum.rs" "$TEMP_DIR/validate_enum_backup.rs"
run_test "validation-mode-enum-after-partial-rename" \
    "$BINARY rename-enum-variant --paths $TEMP_DIR/validate_enum.rs --enum-name Status --old-variant Draft --new-variant Pending --apply && $BINARY rename-enum-variant --paths $TEMP_DIR/validate_enum.rs --enum-name Status --old-variant Draft --new-variant Pending --validate" \
    "grep -q 'Found.*remaining references' $TEMP_DIR/cmd_output.txt" \
    "true"

# Create test file with function references
cat > "$TEMP_DIR/validate_func.rs" << 'EOF'
pub fn process_v2(x: i32) -> i32 { x * 2 }
pub fn main() {
    let result = process_v2(5);
    let other = super::process_v2(10);
}
EOF

# Test: Function validation mode
run_test "validation-mode-function" \
    "$BINARY rename-function --paths $TEMP_DIR/validate_func.rs --old-name process_v2 --new-name process --validate" \
    "grep -q 'Found.*remaining references' $TEMP_DIR/cmd_output.txt && grep -q 'process_v2' $TEMP_DIR/cmd_output.txt" \
    "true"

# ============================================================================
section "SUMMARY FORMAT (Sprint 2, Issue 3)"
# ============================================================================

# Create test file for summary format testing
cat > "$TEMP_DIR/summary_test.rs" << 'EOF'
pub enum Color {
    RedOld,
    Green,
    Blue,
}

pub fn get_color() -> Color {
    Color::RedOld
}

pub fn process_color(c: Color) {
    match c {
        Color::RedOld => println!("red"),
        Color::Green => println!("green"),
        Color::Blue => println!("blue"),
    }
}
EOF

# Test: Summary format should show only changed lines
run_test "summary-format-enum-variant" \
    "$BINARY rename-enum-variant --paths $TEMP_DIR/summary_test.rs --enum-name Color --old-variant RedOld --new-variant Red --format summary" \
    "grep -q 'Changes for' $TEMP_DIR/cmd_output.txt && grep -q 'RedOld' $TEMP_DIR/cmd_output.txt && grep -q '|' $TEMP_DIR/cmd_output.txt" \
    "true"

# Create test file for function summary
cat > "$TEMP_DIR/summary_func.rs" << 'EOF'
pub fn calculate_old(x: i32) -> i32 { x * 2 }
pub fn main() {
    let r = calculate_old(5);
    println!("{}", r);
}
EOF

# Test: Summary format for function rename
run_test "summary-format-function" \
    "$BINARY rename-function --paths $TEMP_DIR/summary_func.rs --old-name calculate_old --new-name calculate --format summary" \
    "grep -q 'Changes for' $TEMP_DIR/cmd_output.txt && grep -q 'calculate_old' $TEMP_DIR/cmd_output.txt" \
    "true"

# Test: Summary format with --apply should still apply changes
cp "$INPUT" "$TEMP_DIR/summary_apply.rs"
run_test "summary-format-with-apply" \
    "$BINARY add-struct-field --paths $TEMP_DIR/summary_apply.rs --struct-name User --field 'verified: bool' --format summary --apply" \
    "grep -q 'verified: bool' $TEMP_DIR/summary_apply.rs && grep -q 'Changes for' $TEMP_DIR/cmd_output.txt" \
    "true"

# ============================================================================
section "EXCLUDE PATTERNS (Sprint 3, Issue 4)"
# ============================================================================

# Create test directory structure
mkdir -p "$TEMP_DIR/exclude_test/src" "$TEMP_DIR/exclude_test/tests/fixtures" "$TEMP_DIR/exclude_test/deprecated"

cat > "$TEMP_DIR/exclude_test/src/lib.rs" << 'EOF'
pub enum Color { RedOld, Green, Blue }
EOF

cat > "$TEMP_DIR/exclude_test/tests/fixtures/test.rs" << 'EOF'
pub enum Color { RedOld, Green, Blue }
EOF

cat > "$TEMP_DIR/exclude_test/deprecated/old.rs" << 'EOF'
pub enum Color { RedOld, Green, Blue }
EOF

# Test: Exclude patterns
run_test "exclude-patterns-glob" \
    "$BINARY rename-enum-variant --paths '$TEMP_DIR/exclude_test/**/*.rs' --enum-name Color --old-variant RedOld --new-variant Red --exclude '**/fixtures/**' --exclude '**/deprecated/**' --apply" \
    "grep -q 'Red' $TEMP_DIR/exclude_test/src/lib.rs && grep -q 'RedOld' $TEMP_DIR/exclude_test/tests/fixtures/test.rs && grep -q 'RedOld' $TEMP_DIR/exclude_test/deprecated/old.rs" \
    "true"

# ============================================================================
section "YAML BATCH OPERATIONS (Sprint 3, Issue 5)"
# ============================================================================

# Create separate directory for batch test
mkdir -p "$TEMP_DIR/batch_test"

# Create test file
cat > "$TEMP_DIR/batch_test/target.rs" << 'EOF'
pub enum Status { Draft, Published }
pub struct User { id: u32 }
EOF

# Create YAML batch spec
cat > "$TEMP_DIR/batch_spec.yaml" << EOF
base_path: $TEMP_DIR/batch_test/
operations:
  - type: RenameEnumVariant
    enum_name: Status
    old_variant: Draft
    new_variant: Pending
    edit_mode: surgical

  - type: AddStructField
    struct_name: User
    field_def: "email: String"
    position:
      Last: null
EOF

# Test: YAML batch operations
run_test "yaml-batch-operations" \
    "$BINARY batch --spec $TEMP_DIR/batch_spec.yaml --apply" \
    "grep -q 'Pending' $TEMP_DIR/batch_test/target.rs && grep -q 'email: String' $TEMP_DIR/batch_test/target.rs" \
    "true"

# Create separate directory for JSON batch test
mkdir -p "$TEMP_DIR/json_batch_test"

cat > "$TEMP_DIR/json_batch_test/target.rs" << 'EOF'
pub enum Status { Draft, Published }
EOF

# Test: JSON still works
cat > "$TEMP_DIR/batch_spec.json" << EOF
{
  "base_path": "$TEMP_DIR/json_batch_test/",
  "operations": [
    {
      "type": "AddEnumVariant",
      "enum_name": "Status",
      "variant_def": "Archived",
      "position": { "Last": null }
    }
  ]
}
EOF

run_test "json-batch-still-works" \
    "$BINARY batch --spec $TEMP_DIR/batch_spec.json --apply" \
    "grep -q 'Archived' $TEMP_DIR/json_batch_test/target.rs" \
    "true"

# ============================================================================
section "FIND: DISCOVERY MODE & VARIANT FILTERING (NEW v0.5.0)"
# ============================================================================

# Test: Findsearch all types (omit --node-type)
cat > "$TEMP_DIR/find_discovery.rs" << 'EOF'
pub struct Rectangle {
    width: f32,
    height: f32,
}

pub enum View {
    Rectangle {
        color: String,
    },
    Circle {
        radius: f32,
    },
}

pub fn main() {
    let r = Rectangle { width: 10.0, height: 20.0 };
}
EOF

run_test "find-discovery-mode-all-types" \
    "$BINARY find --paths $TEMP_DIR/find_discovery.rs --name Rectangle" \
    "grep -q 'struct:' $TEMP_DIR/cmd_output.txt && grep -q 'struct-literal:' $TEMP_DIR/cmd_output.txt && grep -q 'identifier' $TEMP_DIR/cmd_output.txt" \
    "true"

# Test: Enum variant filtering with --variant flag
run_test "find-enum-variant-filter-flag" \
    "$BINARY find --paths $TEMP_DIR/find_discovery.rs --node-type enum --variant Rectangle" \
    "grep -q 'View' $TEMP_DIR/cmd_output.txt && grep -q 'Rectangle' $TEMP_DIR/cmd_output.txt" \
    "true"

# Test: Enum variant filtering with :: syntax
run_test "find-enum-variant-double-colon-syntax" \
    "$BINARY find --paths $TEMP_DIR/find_discovery.rs --node-type enum --name View::Rectangle" \
    "grep -q 'View' $TEMP_DIR/cmd_output.txt && grep -q 'Rectangle' $TEMP_DIR/cmd_output.txt" \
    "true"

# Test: Enum variant filtering with wildcard
run_test "find-enum-variant-wildcard" \
    "$BINARY find --paths $TEMP_DIR/find_discovery.rs --node-type enum --name '*::Rectangle'" \
    "grep -q 'View' $TEMP_DIR/cmd_output.txt && grep -q 'Rectangle' $TEMP_DIR/cmd_output.txt" \
    "true"

# Test: Hints system when search fails
run_test "find-hints-system" \
    "$BINARY find --paths $TEMP_DIR/find_discovery.rs --node-type function --name Rectangle 2>&1" \
    "grep -q 'Hint:' $TEMP_DIR/cmd_output.txt && grep -q 'struct' $TEMP_DIR/cmd_output.txt" \
    "true"

# Test: Canonical names in output (struct not ItemStruct)
run_test "find-canonical-names" \
    "$BINARY find --paths $TEMP_DIR/find_discovery.rs --name Rectangle" \
    "grep -q 'struct:' $TEMP_DIR/cmd_output.txt && ! grep -q 'ItemStruct' $TEMP_DIR/cmd_output.txt" \
    "true"

# Test: Grouped output format
run_test "find-grouped-output" \
    "$BINARY find --paths $TEMP_DIR/find_discovery.rs --name Rectangle" \
    "grep -q 'Found \"Rectangle\" in' $TEMP_DIR/cmd_output.txt" \
    "true"

# ============================================================================
section "DOC COMMENT OPERATIONS"
# ============================================================================

# Test add-doc-comment
cat > "$TEMP_DIR/doc_test.rs" << 'EOF'
pub struct User { id: u64 }
EOF

run_test "add-doc-comment-struct" \
    "$BINARY add-doc-comment --paths $TEMP_DIR/doc_test.rs --target-type struct --name User --doc-comment 'User model' --apply" \
    "grep -q '/// User model' $TEMP_DIR/doc_test.rs"

# Test update-doc-comment
run_test "update-doc-comment" \
    "$BINARY update-doc-comment --paths $TEMP_DIR/doc_test.rs --target-type struct --name User --doc-comment 'Updated user model' --apply" \
    "grep -q '/// Updated user model' $TEMP_DIR/doc_test.rs && ! grep -q '/// User model' $TEMP_DIR/doc_test.rs"

# Test remove-doc-comment
run_test "remove-doc-comment" \
    "$BINARY remove-doc-comment --paths $TEMP_DIR/doc_test.rs --target-type struct --name User --apply" \
    "! grep -q '///' $TEMP_DIR/doc_test.rs"

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
    echo "  ✅ 4 Enum rename operations (basic, qualified-path, surgical, reformat) ⭐ v0.4.2"
    echo "  ✅ 2 Function rename operations (basic, surgical) ⭐ v0.4.2"
    echo "  ✅ 4 Match operations (add, update, remove, auto-detect)"
    echo "  ✅ 3 Code organization (derive, impl, use)"
    echo "  ✅ 2 Diff output tests"
    echo "  ✅ 4 State management (tracking, history, revert, clean)"
    echo "  ✅ 2 Idempotency tests"
    echo "  ✅ 1 Position control test"
    echo "  ✅ 1 Glob pattern test"
    echo "  ✅ 2 Utility operations (find, batch)"
    echo "  ✅ 6 Find & Transform operations (find, filter, comment, remove, replace, method-call)"
    echo "  ✅ 3 Pattern matching tests (pure, wildcard, exact)"
    echo "  ✅ 3 Validation mode tests (enum before/after, function) ⭐ Sprint 2"
    echo "  ✅ 3 Summary format tests (enum, function, with-apply) ⭐ Sprint 2"
    echo "  ✅ 1 Exclude patterns test (glob matching) ⭐ NEW Sprint 3"
    echo "  ✅ 2 YAML batch operations tests (yaml, json-backwards-compat) ⭐ NEW Sprint 3"
    echo "  ✅ 7 Find discovery & variant filtering (all-types, variant-flag, ::, wildcard, hints, canonical, grouped) ⭐ NEW v0.5.0"
    echo "  ✅ 3 Doc comment operations (add, update, remove) ⭐ NEW"
    echo ""
    printf "Total: %b61 tests%b\n" "$BLUE" "$NC"

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
