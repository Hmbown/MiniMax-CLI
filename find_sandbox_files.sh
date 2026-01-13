#!/bin/bash
# Find sandbox-related files
find . -name "*sandbox*" -o -name "*command*" 2>/dev/null | grep -E "\.(rs|toml)$" | sort
