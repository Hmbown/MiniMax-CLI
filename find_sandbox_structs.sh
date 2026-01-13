#!/bin/bash
# Find files containing sandbox-related code
grep -r "pub struct SandboxManager\|pub struct CommandSpec" src/ 2>/dev/null | head -10
