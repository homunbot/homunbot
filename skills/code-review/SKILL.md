---
name: code-review
description: Review code files for bugs, style issues, security vulnerabilities, and suggest improvements. Use when the user asks to review code, check for bugs, or improve code quality.
license: MIT
allowed-tools: "Bash(git:*) read_file list_dir"
metadata:
  author: homunbot
  version: "1.0"
  category: development
---

# Code Review

You are performing a thorough code review. Follow this methodology:

## Review Process

### 1. Understand Context
- Read the file(s) to review using `read_file`
- Check git history for recent changes: `git log --oneline -10` and `git diff HEAD~1`
- Understand the project structure with `list_dir`

### 2. Check Categories

**Correctness:**
- Logic errors, off-by-one, null/None handling
- Edge cases not covered
- Race conditions in async/concurrent code

**Security:**
- Input validation and sanitization
- SQL injection, XSS, command injection risks
- Secrets or credentials in code
- Unsafe operations

**Performance:**
- Unnecessary allocations or copies
- N+1 queries, missing indexes
- Blocking operations in async context
- Inefficient algorithms

**Style & Maintainability:**
- Naming conventions
- Function length (>50 lines is a smell)
- Dead code
- Missing error handling
- Documentation gaps

### 3. Output Format

Present findings as:

**Summary:** One paragraph overview of code quality.

**Issues Found:**
- [CRITICAL] Description — must fix before merge
- [WARNING] Description — should fix soon
- [SUGGESTION] Description — nice to have improvement

**Positive Notes:** What the code does well.

## Rules
- Be specific: reference line numbers and variable names
- Suggest fixes, don't just point out problems
- Prioritize issues by severity
- Be constructive, not condescending
- If the code is good, say so
