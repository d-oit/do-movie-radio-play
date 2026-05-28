# Codacy Configuration Format

Codacy can be configured via a `.codacy.yml` or `.codacy.yaml` file in the repository root.

## Basic Example for this Repository

```yaml
---
exclude_paths:
  - "testdata/**"
  - "analysis/**"
  - "reports/**"
  - "target/**"
  - "**/*.json"
  - "**/*.wav"

engines:
  duplication:
    exclude_paths:
      - "tests/**"
      - "benches/**"

languages:
  rust:
    enabled: true
