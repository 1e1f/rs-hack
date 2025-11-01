#!/bin/bash
# Integration test for rust-ast-edit

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/.."
BINARY="cargo run --quiet --"
INPUT="$PROJECT_DIR/examples/sample.rs"
TEMP_DIR="$PROJECT_DIR/target/test-output"

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Clean up
rm -rf "$TEMP_DIR"
mkdir -p "$TEMP_DIR"

# Build the binary
echo "Building rust-ast-edit..."
cd "$PROJECT_DIR" && cargo build 2>&1 | grep -v "warning:" || true

echo "Running integration tests..."
echo ""

# Test counter
PASSED=0
FAILED=0

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
            ((FAILED++))
            return 1
        fi
    else
        printf "%bFAILED%b (command failed)\n" "$RED" "$NC"
        cat "$TEMP_DIR/cmd_output.txt"
        ((FAILED++))
        return 1
    fi
}

# Test 1: Add struct field
run_test "add-struct-field" \
    "$BINARY add-struct-field --path $TEMP_DIR/test.rs --struct-name User --field 'email: String' --output $TEMP_DIR/out1.rs --apply" \
    "grep -q 'email: String' $TEMP_DIR/out1.rs"

# Test 2: Update struct field
cp "$TEMP_DIR/out1.rs" "$TEMP_DIR/test.rs"
run_test "update-struct-field" \
    "$BINARY update-struct-field --path $TEMP_DIR/test.rs --struct-name User --field 'pub email: String' --output $TEMP_DIR/out2.rs --apply" \
    "grep -q 'pub email: String' $TEMP_DIR/out2.rs" \
    "true"

# Test 3: Remove struct field
cp "$TEMP_DIR/out2.rs" "$TEMP_DIR/test.rs"
run_test "remove-struct-field" \
    "$BINARY remove-struct-field --path $TEMP_DIR/test.rs --struct-name User --field-name email --output $TEMP_DIR/out3.rs --apply" \
    "! grep -q 'email' $TEMP_DIR/out3.rs" \
    "true"

# Test 4: Add enum variant
run_test "add-enum-variant" \
    "$BINARY add-enum-variant --path $INPUT --enum-name Status --variant 'Archived' --output $TEMP_DIR/out4.rs --apply" \
    "grep -q 'Archived' $TEMP_DIR/out4.rs"

# Test 5: Update enum variant
cp "$TEMP_DIR/out4.rs" "$TEMP_DIR/test.rs"
run_test "update-enum-variant" \
    "$BINARY update-enum-variant --path $TEMP_DIR/test.rs --enum-name Status --variant 'Draft { created_at: u64 }' --output $TEMP_DIR/out5.rs --apply" \
    "grep -q 'Draft.*created_at' $TEMP_DIR/out5.rs" \
    "true"

# Test 6: Remove enum variant
cp "$TEMP_DIR/out5.rs" "$TEMP_DIR/test.rs"
run_test "remove-enum-variant" \
    "$BINARY remove-enum-variant --path $TEMP_DIR/test.rs --enum-name Status --variant-name Archived --output $TEMP_DIR/out6.rs --apply" \
    "! grep -q '^[[:space:]]*Archived' $TEMP_DIR/out6.rs" \
    "true"

# Test 7: Add match arm
cp "$INPUT" "$TEMP_DIR/out7.rs"
run_test "add-match-arm" \
    "$BINARY add-match-arm --path $TEMP_DIR/out7.rs --pattern 'Status::Archived' --body '\"archived\".to_string()' --function handle_status --apply" \
    "grep -q 'Archived' $TEMP_DIR/out7.rs" \
    "true"

# Test 8: Update match arm
cp "$TEMP_DIR/out7.rs" "$TEMP_DIR/out8.rs"
run_test "update-match-arm" \
    "$BINARY update-match-arm --path $TEMP_DIR/out8.rs --pattern 'Status::Draft' --body '\"pending\".to_string()' --function handle_status --apply" \
    "grep -q 'pending' $TEMP_DIR/out8.rs" \
    "true"

# Test 9: Remove match arm
cp "$TEMP_DIR/out8.rs" "$TEMP_DIR/out9.rs"
run_test "remove-match-arm" \
    "$BINARY remove-match-arm --path $TEMP_DIR/out9.rs --pattern 'Status::Deleted' --function handle_status --apply" \
    "! grep -q 'Status.*::.*Deleted.*=>' $TEMP_DIR/out9.rs" \
    "true"

# Summary
echo ""
echo "====== Test Summary ======"
printf "Passed: %b%s%b\n" "$GREEN" "$PASSED" "$NC"
printf "Failed: %b%s%b\n" "$RED" "$FAILED" "$NC"
echo "=========================="

if [ $FAILED -eq 0 ]; then
    printf "%bAll tests passed!%b\n" "$GREEN" "$NC"
    exit 0
else
    printf "%bSome tests failed.%b\n" "$RED" "$NC"
    exit 1
fi
