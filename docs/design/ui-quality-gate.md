# UI Quality Gate Checklist

Use this checklist before merging frontend changes.

## 1. Layout & Structure

- Screen has a clear primary task.
- Sections are ordered logically.
- One primary CTA per functional block.
- Advanced actions are secondary and discoverable.

## 2. Harmony & Consistency

- Same component type looks and behaves the same everywhere.
- No ad-hoc styles conflicting with design tokens.
- Border, radius, and shadow usage is consistent.

## 3. Spacing & Alignment

- Spacing follows approved scale only.
- Controls align to shared edges/baselines.
- No visual jumps or cramped clusters.

## 4. Typography

- Typography scale follows system values.
- Heading/body/meta hierarchy is clear.
- Card text is readable without zooming.

## 5. Interaction & States

- States implemented: default, hover, focus, disabled.
- Async states implemented: loading, empty, error, success.
- Action labels are clear and unambiguous.

## 6. Responsive

- Verified at 1280+, 1024, 768, 390 widths.
- No horizontal overflow.
- Touch targets are at least 44x44 where applicable.

## 7. Accessibility Baseline

- Contrast is at least WCAG AA.
- Keyboard focus is visible and predictable.
- Important controls have clear labels.

## 8. Final Review Output

- List blockers first (P0/P1).
- Include exact component/section and fix.
- Do not approve if visual coherence is broken.
