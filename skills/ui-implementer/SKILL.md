---
name: ui-implementer
description: Implement premium UI changes from approved design rules and review findings. Use for frontend execution after structure and style direction are defined.
license: MIT
allowed-tools: "read_file list_dir edit_file Bash(rg:*) Bash(sed:*) Bash(cargo:*)"
metadata:
  author: homun
  version: "1.0"
  category: design
---

# UI Implementer

Implement UI with precision and consistency. Prioritize readability and usability.

## Implementation Protocol

1. Map requested change to existing components and tokens.
2. Define section structure before styling details.
3. Apply typography and spacing scale first.
4. Implement states (default, hover, focus, empty, error).
5. Validate desktop and mobile behavior.

## Guardrails

- No ad-hoc colors when a token exists.
- No one-off spacing values outside approved scale.
- Do not mix multiple visual patterns for the same component type.
- Keep primary CTA count to one per block.
- Avoid prompt-driven setup for complex forms; prefer guided inline flow.

## Done Criteria

- Structure is understandable without explanation.
- Visual hierarchy is obvious at first glance.
- No clipped text, overflow, or misaligned controls.
- Feature is testable end-to-end from UI only.
