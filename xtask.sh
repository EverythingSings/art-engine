#!/usr/bin/env bash
# art-engine build infrastructure
# Usage: bash xtask.sh <command>
#
# Commands:
#   check   - Full verification: fmt, clippy, test, doc
#   test    - Run all workspace tests
#   clippy  - Lint all crates
#   fmt     - Format check (no writes)
#   doc     - Build docs
#   build   - Build all crates (native)
#   wasm    - Build WASM target
#   clean   - Remove build artifacts

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m'

step() { echo -e "${CYAN}==> $1${NC}"; }
pass() { echo -e "${GREEN}==> $1 passed${NC}"; }
fail() { echo -e "${RED}==> $1 FAILED${NC}"; exit 1; }

cmd_fmt() {
    step "cargo fmt --check"
    cargo fmt --all -- --check && pass "fmt" || fail "fmt"
}

cmd_clippy() {
    step "cargo clippy (workspace)"
    cargo clippy --all -- -D warnings && pass "clippy (workspace)" || fail "clippy (workspace)"
    step "cargo clippy (core + render)"
    cargo clippy -p art-engine-core --features render -- -D warnings && pass "clippy (render)" || fail "clippy (render)"
}

cmd_test() {
    if [ -n "${1:-}" ]; then
        step "cargo test -p $1"
        cargo test -p "$1" && pass "test ($1)" || fail "test ($1)"
    else
        step "cargo test --all"
        cargo test --all && pass "test (workspace)" || fail "test (workspace)"
        step "cargo test (core + render)"
        cargo test -p art-engine-core --features render && pass "test (render)" || fail "test (render)"
    fi
}

cmd_doc() {
    step "cargo doc --all --no-deps"
    cargo doc --all --no-deps && pass "doc" || fail "doc"
}

cmd_build() {
    step "cargo build --all"
    cargo build --all && pass "build (workspace)" || fail "build (workspace)"
    step "cargo build (core + render)"
    cargo build -p art-engine-core --features render && pass "build (render)" || fail "build (render)"
}

cmd_wasm() {
    step "cargo build (wasm32)"
    cargo build -p art-engine-wasm --target wasm32-unknown-unknown && pass "wasm" || fail "wasm"
}

cmd_clean() {
    step "cargo clean"
    cargo clean
}

cmd_check() {
    cmd_fmt
    cmd_clippy
    cmd_test
    cmd_doc
    echo -e "${GREEN}==> All checks passed.${NC}"
}

case "${1:-check}" in
    check)  cmd_check ;;
    test)   cmd_test "${2:-}" ;;
    clippy) cmd_clippy ;;
    fmt)    cmd_fmt ;;
    doc)    cmd_doc ;;
    build)  cmd_build ;;
    wasm)   cmd_wasm ;;
    clean)  cmd_clean ;;
    *)
        echo "Unknown command: $1"
        echo "Usage: bash xtask.sh {check|test|clippy|fmt|doc|build|wasm|clean}"
        exit 1
        ;;
esac
