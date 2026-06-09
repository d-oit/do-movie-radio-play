# Unresolved Review Player Issues

**Date:** 2026-06-08  
**Source:** `plans/070-review-player-testing/TESTING.md` (2026-04-15)  

These 4 minor bugs were found during review player testing but were never filed as GitHub issues. They remain unresolved.

---

## 1. Save Reviewed HTML Missing Merged State

**Source:** `src/review.rs:332-343` (original), `src/review_template.rs` (extracted)  
**Priority:** Minor

**Description:** When saving reviewed HTML, the current merged/individual view mode is not preserved in the saved file. The exported HTML defaults to individual view mode on reload, losing the user's last review state.

**Expected behavior:** The exported HTML should auto-load the last review state (merged vs individual view, excluded segments, active segment position).

**Impact:** Users who review in merged mode must manually toggle back each time they reload the saved HTML.

---

## 2. No Segment Kind Filtering in UI

**Source:** `src/review.rs:324-330` (original), `src/review_template.rs` (extracted)  
**Priority:** Minor

**Description:** The `refreshSegments()` function filters by the excluded Set but provides no UI to filter or sort segments by kind, confidence, or duration.

**Expected behavior:** Users should be able to filter the segment list by:
- Confidence range (e.g., show only segments with confidence ≥ 0.8)
- Kind (e.g., show only music, only silence)
- Duration (e.g., show segments longer than 1s)

Or at minimum sort by these columns.

**Impact:** Users reviewing many segments cannot efficiently find low-confidence or short segments without manually scanning the full list.

---

## 3. Timeline Markers Not Draggable

**Source:** `src/review.rs:466-470` (original), `src/review_template.rs` (extracted)  
**Priority:** Minor

**Description:** Timeline markers are click-only — clicking jumps to the segment start, but there is no drag-to-seek behavior.

**Expected behavior:** Users should be able to click and drag the playhead along the timeline waveform to seek to any position, not just segment boundaries.

**Impact:** Limited manual scrubbing UX. Users cannot fine-tune playback position within a long segment.

---

## 4. Empty Segments After Exclusion Has No Recovery UX

**Source:** `src/review.rs:359-372` (original), `src/review_template.rs` (extracted)  
**Priority:** Info

**Description:** When all segments are marked as voice via the 'x' key, the UI shows "No non-voice segments found" but provides no clear recovery path other than Undo ('u').

**Expected behavior:** A "Restore All" button should appear when all segments are excluded, providing a one-click recovery path.

**Impact:** Users who accidentally exclude all segments must manually undo each one or refresh. The current behavior is functional but provides poor UX for an edge case that should have a self-evident recovery action.
