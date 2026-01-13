#!/bin/bash
# Find tool-related files
find src/tools -name "*.rs" -type f 2>/dev/null | sort
echo "---"
find src/commands -name "*.rs" -type f 2>/dev/null | sort
echo "---"
find src/core -name "*.rs" -type f 2>/dev/null | sort
