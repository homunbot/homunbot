---
name: design-system-guardian
description: Enforce premium UI rules for harmony, proportions, spacing, alignment, and Braun essential style. Use this before planning or implementing frontend/UI changes.
license: MIT
allowed-tools: "read_file list_dir Bash(rg:*) Bash(sed:*)"
metadata:
  author: homun
  version: "1.0"
  category: design
---

# Design System Guardian

Apply these rules as non-negotiable constraints before any UI work.

## Core Principles

- Harmony: one visual grammar across the page. No mixed radii, random shadows, or inconsistent control heights.
- Proportion: use an 8px spacing grid and stable vertical rhythm.
- Alignment: enforce column and baseline alignment. No visible 1px drift.
- Braun essential: remove decorative noise. Prioritize clarity, hierarchy, and calm.
- Premium quality: every screen must feel intentional, minimal, and production-ready.

## Hard Constraints

- Spacing scale: 8, 12, 16, 24, 32, 40.
- Control heights: inputs/buttons 40px, small controls 32px.
- Border radius tokens only: 8px (small), 12px (default), pill for chips/badges.
- Typography scale only: 12 / 14 / 16 / 20 / 28.
- Max one primary CTA per functional block.
- Contrast must satisfy WCAG AA minimum.

## Layout Rules

- Build sections with clear purpose: search, filters, content, actions.
- Group related controls in compact clusters; keep destructive actions visually separated.
- Keep scanning order obvious: title -> status -> action -> details.
- Use progressive disclosure: default simple, advanced behind explicit affordances.

## Interaction Rules

- Never rely on browser default focus styles.
- Provide visible states for: loading, empty, error, disabled, success.
- Keep action labels unambiguous: use verbs (`Install`, `Test`, `Save`).
- Prefer inline guidance over hidden modal prompts for critical setup flows.

## Workflow

1. Audit the current screen against hard constraints.
2. Write a minimal structure plan (sections + hierarchy + CTA strategy).
3. Implement with existing tokens/components where possible.
4. Run visual QA gate before finalizing.

## Visual QA Gate (must pass)

- Spacing rhythm consistent.
- Alignments clean on desktop and mobile.
- Typography hierarchy clear and stable.
- CTA hierarchy clear.
- Empty/error/loading states present and readable.
- No style regressions vs existing design system.
