# Design Constitution

This document defines non-negotiable UI quality rules for Homun.

## Principles

- Harmony first: one coherent visual language per screen.
- Braun essential: minimal, useful, calm, precise.
- Premium execution: production-grade polish, not prototype styling.
- Usability over decoration: every visual choice must improve comprehension.

## System Rules

- Base spacing grid: 8px.
- Approved spacing scale: 8 / 12 / 16 / 24 / 32 / 40.
- Control heights: 40px default, 32px compact.
- Typography scale: 12 / 14 / 16 / 20 / 28.
- Border radius: 8px and 12px only, plus pill for chips.
- One accent color family per page context.
- Focus, disabled, error, success states are mandatory.

## Composition Rules

- Clear section hierarchy: intent, controls, content, actions.
- Vertical rhythm must be consistent across all sections.
- Align baselines and column edges; avoid visible drift.
- Keep primary CTA obvious and unique per functional block.
- Use progressive disclosure for advanced options.

## Usability Rules

- Setup flows must be understandable without hidden knowledge.
- Avoid modal/prompt dependency for critical configuration.
- Labels must be verb-driven and explicit.
- Empty states must suggest the next action.

## Quality Gate

No UI change is complete unless it passes:

- Desktop and mobile layout checks.
- Visual alignment and spacing checks.
- Typography hierarchy check.
- Interaction state coverage check.
- Accessibility baseline check (contrast, focus visibility, touch targets).
