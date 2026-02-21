#!/usr/bin/env bash
# Benchmark + leak-check runner for disku's macOS scanner.
#
# Usage: tests/bench.sh [PATH]
#   PATH defaults to $HOME

set -euo pipefail

TARGET="${1:-$HOME}"
BENCH_BIN="./target/release/bench_scan"

echo "=== Stage 1: Build ==="
cargo build --release --bin bench_scan
echo "build OK"
echo

echo "=== Stage 2: Benchmark ==="
"$BENCH_BIN" --iterations 5 --compare "$TARGET"
echo

echo "=== Stage 3: Leak Check ==="
# leaks requires MallocStackLogging for full traces
export MallocStackLogging=1
LEAKS_OUTPUT=$(leaks --atExit -- "$BENCH_BIN" --single "$TARGET" 2>&1) || true
unset MallocStackLogging

echo "$LEAKS_OUTPUT" | tail -5

if echo "$LEAKS_OUTPUT" | grep -q "0 leaks for 0 total leaked bytes"; then
    echo "LEAK CHECK: PASS"
elif echo "$LEAKS_OUTPUT" | grep -q "0 leaks"; then
    echo "LEAK CHECK: PASS"
else
    echo "LEAK CHECK: FAIL"
    echo "$LEAKS_OUTPUT"
    exit 1
fi
