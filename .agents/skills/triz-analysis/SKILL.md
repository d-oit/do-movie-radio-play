---
name: triz-analysis
description: Generic TRIZ analysis for identifying technical and physical contradictions in software architecture and pipelines.
---

## When to use
Use during the initial phase of any major architectural change, feature implementation, or optimization task. TRIZ analysis helps surface fundamental tradeoffs (contradictions) that might be overlooked.

## Core Process

1. **Identify the Improving Parameter**: What are we trying to improve? (e.g., speed, accuracy, modularity).
2. **Identify the Worsening Parameter**: What gets worse if we improve the first parameter? (e.g., CPU usage, code complexity, latency).
3. **Formulate the Technical Contradiction**: If we increase [Improving Parameter], then [Worsening Parameter] becomes unacceptable.
4. **Formulate the Physical Contradiction**: A single element must have two opposite properties (e.g., the buffer must be large for accuracy but small for latency).

## Project-Specific Contradiction Examples

### 1. VAD Throughput vs. False Positives
- **Improving**: VAD throughput (frames/sec).
- **Worsening**: False positives (misidentified speech).
- **Contradiction**: Faster processing often requires simpler heuristics or larger windows, which can lead to missing short speech segments or misclassifying noise.

### 2. Review Quality vs. Pipeline Latency
- **Improving**: Review quality (interactive features, high-res spectral analysis).
- **Worsening**: Pipeline latency (time to first report).
- **Contradiction**: Higher quality reviews require more pre-computation and data extraction, delaying the point at which the user can start reviewing.

### 3. Deterministic Output vs. Self-Learning Calibration
- **Improving**: Deterministic pipeline output (reproducibility).
- **Worsening**: Self-learning calibration (adaptive thresholds).
- **Contradiction**: For reproducibility, we want fixed parameters. For better performance across diverse media, we want the system to learn and adapt its thresholds dynamically.

### 4. Modularity vs. CI/Runtime Complexity
- **Improving**: Modularity (separating logic into multiple crates or fine-grained modules).
- **Worsening**: CI/Runtime complexity (longer build times, complex dependency graph).
- **Contradiction**: More modules improve code organization and reusability but increase the overhead of managing dependencies, compilation times, and cross-module interfaces.

## Output
All TRIZ analysis findings must be written to `analysis/triz/` with a descriptive filename (e.g., `analysis/triz/feature-x-contradictions.md`).

## Related Skills
- **triz-solver**: For resolving the identified contradictions.
- **triz-audio-timeline**: Domain-specific TRIZ for audio segmentation.
