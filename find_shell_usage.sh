#!/bin/bash
# Find all Rust files that might be relevant
find src -name "*.rs" -type f 2>/dev/null | xargs grep -l "Command::new\|shell\|exec\|std::process" 2>/dev/null | head -20
