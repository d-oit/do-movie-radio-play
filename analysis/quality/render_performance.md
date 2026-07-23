# Render Performance Guidance

This document provides architectural best practices and performance guidelines for audio rendering and mixing in the `movie-radio` play pipeline.

## Performance-Driven Design

Audio rendering in the `movie-radio-render` crate has been heavily optimized to ensure minimal overhead, making it highly suitable for real-time playbacks, streaming previews, or rapid batch-processing pipelines.

Two core optimizations have been implemented:

### 1. Precomputed Math and Gain Constants (`StereoPosition`)

In stereo rendering, spatial panning represents how a mono track's signal is distributed between left and right channels based on a panning angle.
Traditionally, calculating these gains on every block or frame involved trigonometric operations:
$$\text{angle} = (pos + 1.0) \times \frac{\pi}{4}$$
$$left = \cos(\text{angle}), \quad right = \sin(\text{angle})$$

Computing `sin_cos()` per-frame or per-track is computationally intensive. To solve this:
- **Precomputed Fields**: `StereoPosition` stores `pos`, `left_gain`, and `right_gain` directly.
- **Single Computation**: Trigonometric panning calculation is performed **once** during `StereoPosition::new` (or during deserialization).
- **Compile-Time Constants**: Standard preset positions (such as `CENTRE`, `LEFT`, `RIGHT`, `HARD_LEFT`, `HARD_RIGHT`) are defined with precise, pre-calculated constant gains, completely bypassing mathematical operations at runtime.
- **Zero-Cost Access**: The `gains()` method retrieves precomputed gains in $O(1)$ time with simple memory copies.

### 2. Amortized Memory Allocations via Reusable Buffers (`Mixer`)

To eliminate garbage collection pressure and system allocator bottlenecking in the rendering loops:
- **Flat Buffer Layout**: Rendering operates on flat contiguous sample buffers (`Vec<f32>` per channel) rather than nested vectors or collections.
- **The `Mixer` Context**: The `Mixer` struct encapsulates `left_channel`, `right_channel`, and `interleaved_output` buffers.
- **Amortized Allocation**: We use `.clear()` and `.resize(max_len, 0.0)` which resets the buffer state but **retains the underlying capacity**. This results in $O(1)$ amortized memory allocation.
- **Separate Channels before Interleaving**: Mixing computes separate contiguous left and right buffers first, improving cache locality before interleaving the samples for final stereo peak normalization.

---

## Recommended Block and Sample Sizes

When feeding audio tracks into the renderer, utilizing appropriate chunk/block sizes is vital for optimal CPU cache utilization (L1/L2 caches).

| Block Size (Samples) | Typical Latency (at 48 kHz) | Recommended Use Case |
|---|---|---|
| **512** | ~10.6 ms | Real-time playback, low-latency interactive monitoring. |
| **1024** | ~21.3 ms | Standard balanced playback, streaming servers, preview players. |
| **2048** | ~42.6 ms | Default recommendation for production timeline exports and offline generation. Excellent throughput. |
| **4096+** | >85 ms | Batch exports, deep pipeline runs. Maximizes throughput at the expense of memory footprint. |

---

## Performance Best Practices

1. **Prefer `Mixer::render_mix` over `render_mix`:**
   Instead of using the convenient `render_mix(tracks)` function which creates a new `Mixer` every time, instantiate a `Mixer` once and reuse it across multiple rendering frames:
   ```rust
   use movie_radio_render::mixer::Mixer;

   let mut mixer = Mixer::new();
   for block in audio_blocks {
       let stereo_data = mixer.render_mix(block.tracks)?;
       // Process or stream stereo_data...
   }
   ```

2. **Keep Sample Rates Uniform:**
   Mixing tracks with mismatched sample rates forces resampling inside the mixer, increasing the CPU footprint. Ensure that voice, ambient noise, and music tracks are pre-resampled to a standard sample rate (e.g., **48,000 Hz**).

3. **Avoid Per-Frame Reverb and AGC Parameter Changes:**
   Reverb and AGC setups are initialized using `rodio` buffers. Re-instantiating these configurations per-block triggers reallocation. Prefer stable spatial and reverb configurations.
