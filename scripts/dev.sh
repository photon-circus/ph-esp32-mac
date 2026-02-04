#!/bin/bash
# ph-esp32-mac Development Scripts
#
# Shell script for common development tasks across all crates.
# Usage: ./scripts/dev.sh <command>
#
# Commands:
#   fmt       - Format all code
#   clean     - Clean all build artifacts
#   check     - Check all crates
#   test      - Run all tests
#   clippy    - Run clippy on all crates

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

print_header() {
    echo ""
    echo "=== $1 ==="
}

case "${1:-help}" in
    fmt)
        print_header "Formatting main crate"
        cargo fmt
        
        print_header "Formatting integration_tests"
        (cd "$ROOT_DIR/integration_tests" && cargo fmt)
        
        echo ""
        echo "All code formatted!"
        ;;
    
    clean)
        print_header "Cleaning main crate"
        cargo clean
        
        print_header "Cleaning integration_tests"
        (cd "$ROOT_DIR/integration_tests" && cargo clean)
        
        echo ""
        echo "All build artifacts cleaned!"
        ;;
    
    check)
        print_header "Checking main crate"
        cargo check
        
        print_header "Checking integration_tests"
        (cd "$ROOT_DIR/integration_tests" && cargo check)
        
        echo ""
        echo "All crates check passed!"
        ;;
    
    test)
        print_header "Running main crate tests"
        cargo test --lib
        
        echo ""
        echo "All tests passed!"
        ;;
    
    clippy)
        print_header "Running clippy on main crate"
        cargo clippy -- -D warnings
        
        print_header "Running clippy on integration_tests"  
        (cd "$ROOT_DIR/integration_tests" && cargo clippy -- -D warnings)
        
        echo ""
        echo "Clippy passed on all crates!"
        ;;
    
    all)
        "$0" fmt
        "$0" clippy
        "$0" test
        
        echo ""
        echo "All checks passed!"
        ;;
    
    *)
        cat << EOF
ph-esp32-mac Development Scripts

Usage: ./scripts/dev.sh <command>

Commands:
  fmt       Format all code (main + integration_tests)
  clean     Clean all build artifacts
  check     Check all crates compile
  test      Run all host tests
  clippy    Run clippy on all crates
  all       Run fmt, clippy, and test

Examples:
  ./scripts/dev.sh fmt
  ./scripts/dev.sh all
EOF
        ;;
esac
