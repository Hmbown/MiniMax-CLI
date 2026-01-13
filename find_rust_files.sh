#!/usr/bin/env bash
# Find all Rust source files in the project
find . -name "*.rs" -type f | sort

echo "---"
echo "Total Rust files:"
find . -name "*.rs" -type f | wc -l
