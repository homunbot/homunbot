---
name: ui-critic
description: >-
  Review UI/UX quality with strict premium criteria: alignment, spacing,
  hierarchy, usability, and visual consistency. Use when the user asks for
  UI review or shares screenshots.
license: MIT
allowed-tools: "read_file list_dir Bash(rg:*) Bash(sed:*)"
metadata:
  author: homun
  version: "1.0"
  category: design
---

# UI Critic

Perform a strict UI review. Focus on blockers first.

## Review Order

1. Information architecture and flow clarity.
2. Visual hierarchy and typography.
3. Alignment, spacing, and proportions.
4. Interaction clarity and action labeling.
5. Responsiveness and accessibility basics.

## Severity Levels

- P0: prevents task completion.
- P1: high friction or major confusion.
- P2: quality regression or visual inconsistency.
- P3: polish improvement.

## Required Output

- Findings first, sorted by severity.
- Each finding must include:
  - impacted area (section/component)
  - concrete issue
  - user impact
  - exact fix direction
- Brief summary only after findings.

## Critical Checks

- Is the primary action obvious within 2 seconds?
- Is the setup flow understandable without prior context?
- Are cards and controls readable at normal zoom?
- Are spacing and alignment consistent between rows/sections?
- Are empty and error states actionable?

## Rule

Do not approve a UI if it is functionally correct but visually incoherent.
