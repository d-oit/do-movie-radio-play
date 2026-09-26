# Unresolved Review Player Issues

**Date:** 2026-06-08 (Created) | Updated: 2026-09-24
**Source:** `plans/070-review-player-testing/TESTING.md` (2026-04-15)  
**Status:** Resolved — see GitHub issues #269 and #270

All 4 issues listed in this document have been resolved in current source (`templates/review.html`, `crates/movie-radio-timeline/src/review.rs`, `crates/movie-radio-timeline/src/review_template.rs`) and verified with unit tests.

---

## 1. Save Reviewed HTML Missing Merged State

**Source:** `crates/movie-radio-timeline/src/review.rs` & `templates/review.html`
**Priority:** Minor
**Status:** **Resolved** (closed in issue #269)

**Description:** When saving reviewed HTML, the current merged/individual view mode is not preserved in the saved file. The exported HTML defaults to individual view mode on reload, losing the user's last review state.

**Resolution:** `review.rs` persists `merged_json` state into a `<script id="merged-data">` DOM element in the saved HTML. Tests `test_merged_view_state_persistence_in_review_html` in `review.rs` and `test_merged_state_script_element` in `review_template.rs` verify state persistence.

---

## 2. No Segment Kind Filtering in UI

**Source:** `templates/review.html`
**Priority:** Minor
**Status:** **Resolved**

**Description:** The `refreshSegments()` function filters by the excluded Set but provides no UI to filter or sort segments by kind, confidence, or duration.

**Resolution:** `templates/review.html` provides `#segment-filter` (all, verified, priority, unverified, suspicious, excluded) and `#segment-sort` (start, confidence, duration) UI controls that filter and sort segments dynamically during `refreshSegments()`.

---

## 3. Timeline Markers Not Draggable

**Source:** `templates/review.html` & `crates/movie-radio-timeline/src/review_template.rs`
**Priority:** Minor
**Status:** **Resolved** (closed in issue #270)

**Description:** Timeline markers are click-only — clicking jumps to the segment start, but there is no drag-to-seek behavior.

**Resolution:** `templates/review.html` implements drag-to-seek handlers (`dragMoved` and `.timeline-track.dragging` / `mousedown`/`mousemove`/`mouseup` events on track and playhead). Verified by `test_drag_to_seek_handlers_present` in `review_template.rs`.

---

## 4. Empty Segments After Exclusion Has No Recovery UX

**Source:** `templates/review.html` & `crates/movie-radio-timeline/src/review_template.rs`
**Priority:** Info
**Status:** **Resolved** (closed in issue #270)

**Description:** When all segments are marked as voice via the 'x' key, the UI shows "No non-voice segments found" but provides no clear recovery path other than Undo ('u').

**Resolution:** `templates/review.html` provides a "Restore All (r)" button when all segments are excluded. Verified by `test_restore_all_elements_present` in `review_template.rs`.
