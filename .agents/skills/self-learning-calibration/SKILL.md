---
name: self-learning-calibration
description: self-learning-calibration
---

## When to use
Use for pipeline updates in the skill domain.

## Inputs expected
- Media path or timeline JSON
- Analysis config values

## Outputs expected
- Deterministic JSON updates
- Tests and notes under analysis/

## Guardrails
- CPU-only and deterministic.
- Keep functions small and auditable.
- No uncontrolled online learning.

## Steps
1. Validate config and fixtures.
2. Implement smallest safe increment.
3. Add/adjust deterministic tests.
4. Run quality gates.

## References
- `reference/NOTES.md`
