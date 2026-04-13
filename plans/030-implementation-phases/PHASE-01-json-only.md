# Phase 01 -- COMPLETE

Implement extract JSON pipeline and deterministic segmentation.

## Status: Complete

Core pipeline (decode -> resample -> frame -> VAD -> smooth -> segment -> invert -> JSON)
is fully implemented and tested. Deterministic output verified by
`repeated_extract_is_deterministic` test.
