---
name: triz-solver
description: Generic TRIZ solver for resolving architectural and technical contradictions using inventive principles.
---

## When to use
Use after `triz-analysis` has identified technical or physical contradictions. The solver provides a structured way to find innovative solutions beyond simple compromises.

## Inventive Principles for Software/Rust
1. **Segmentation**: Divide into independent parts (crates, async tasks, plugins).
2. **Extraction**: Remove the "disturbing" part (isolate side effects, move hot loops to SIMD).
3. **Local Quality**: Make each part of the system function under optimal conditions (custom thresholds per audio profile).
4. **Asymmetry**: Use asymmetric structures for asymmetric problems (different processing for speech vs. silence).
5. **Prior Action**: Perform required changes before they are needed (pre-allocate buffers, pre-compute spectral features).
6. **Feedback**: Introduce feedback to improve the process (self-learning calibration).
7. **Taking Out**: Only keep the essential part (shared `types` crate).

## Solver Examples (Project-Specific)

### 1. VAD Throughput vs. False Positives
- **Contradiction**: High speed vs. high accuracy.
- **TRIZ Solution (Segmentation + Extraction)**:
  - Use a multi-pass VAD.
  - Pass 1: Extremely fast, coarse energy-based VAD to extract potential speech regions.
  - Pass 2: More expensive, high-accuracy analysis (spectral flux, entropy) only on the regions identified in Pass 1.
- **TRIZ Solution (Local Quality)**:
  - Apply noise reduction/pre-filtering only to regions with low signal-to-noise ratio.

### 2. Review Quality vs. Pipeline Latency
- **Contradiction**: High data detail vs. fast feedback.
- **TRIZ Solution (Prior Action)**:
  - Generate a "Draft" interactive report as soon as VAD is done.
  - Stream high-resolution spectral data in the background to update the report incrementally.
- **TRIZ Solution (Local Quality)**:
  - Only compute high-resolution spectral features for segments where VAD confidence is low or a "Review Required" tag is applied.

### 3. Deterministic Output vs. Self-Learning
- **Contradiction**: Reproducibility vs. Adaptation.
- **TRIZ Solution (Asymmetry)**:
  - Keep the core audio pipeline deterministic by taking all parameters as input.
  - Store the "Learned" parameters in a sidecar SQLite/libsql database (`learning.db`).
  - Provide a "Synchronize" step that updates the static config from the learned database periodically.
- **TRIZ Solution (Feedback)**:
  - Use the `verified_segments` table to continuously refine the spectral thresholds, but apply them in discrete, versioned "Profile" updates.

### 4. Modularity vs. CI/Runtime Complexity
- **Contradiction**: Clean organization vs. Build overhead.
- **TRIZ Solution (Taking Out)**:
  - Extract minimal, common data structures into a zero-dependency `types` crate to prevent dependency bloat in high-level modules.
- **TRIZ Solution (Homogeneity)**:
  - Use standardized error handling (`anyhow`, `thiserror`) and logging across all modules to reduce cognitive load and boilerplate.

## Output
All TRIZ solver outcomes must be written to `analysis/triz/` with a descriptive filename (e.g., `analysis/triz/feature-x-solutions.md`).

## Related Skills
- **triz-analysis**: For identifying the contradictions to solve.
- **triz-audio-timeline**: Domain-specific TRIZ for audio segmentation.
