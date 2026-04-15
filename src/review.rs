use anyhow::{bail, Result};
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

use crate::types::{SegmentKind, TimelineOutput};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReviewSegment {
    index: usize,
    start_ms: u64,
    end_ms: u64,
    duration_ms: u64,
    confidence: f32,
    tags: Vec<String>,
    prompt: Option<String>,
    #[serde(default)]
    verification_status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VerifiedOutput {
    segment_results: Vec<SegmentResult>,
}

#[derive(Debug, Deserialize)]
struct SegmentResult {
    start_ms: u64,
    end_ms: u64,
    verification_status: String,
}

#[allow(dead_code)]
pub fn write_review_html(
    input_media: &Path,
    timeline: &TimelineOutput,
    output: &Path,
    pre_roll_s: f32,
    post_roll_s: f32,
    verified: Option<&Path>,
) -> Result<usize> {
    write_review_html_with_options(
        input_media,
        timeline,
        output,
        pre_roll_s,
        post_roll_s,
        verified,
        false,
    )
}

pub fn write_review_html_with_options(
    input_media: &Path,
    timeline: &TimelineOutput,
    output: &Path,
    pre_roll_s: f32,
    post_roll_s: f32,
    verified: Option<&Path>,
    merged: bool,
) -> Result<usize> {
    if !input_media.exists() {
        bail!("input media does not exist: {}", input_media.display());
    }

    let media_path = std::fs::canonicalize(input_media)
        .unwrap_or_else(|_| input_media.to_path_buf())
        .to_string_lossy()
        .to_string();
    let media_json = serde_json::to_string(&media_path)?;
    let pre_roll_json = serde_json::to_string(&pre_roll_s)?;
    let post_roll_json = serde_json::to_string(&post_roll_s)?;

    let verification_map: HashMap<String, String> = if let Some(verified_path) = verified {
        if verified_path.exists() {
            let content = std::fs::read_to_string(verified_path)?;
            let verified_data: VerifiedOutput = serde_json::from_str(&content)?;
            verified_data
                .segment_results
                .into_iter()
                .map(|r| {
                    let key = format!("{}-{}", r.start_ms, r.end_ms);
                    (key, r.verification_status)
                })
                .collect()
        } else {
            HashMap::new()
        }
    } else {
        HashMap::new()
    };

    let segments: Vec<ReviewSegment> = if merged {
        let non_voice_segments: Vec<_> = timeline
            .segments
            .iter()
            .filter(|segment| segment.kind == SegmentKind::NonVoice)
            .collect();

        if non_voice_segments.is_empty() {
            vec![]
        } else {
            let first_start = non_voice_segments.first().map(|s| s.start_ms).unwrap_or(0);
            let last_end = non_voice_segments.last().map(|s| s.end_ms).unwrap_or(0);
            let duration_ms = last_end.saturating_sub(first_start);
            let avg_confidence: f32 = non_voice_segments.iter().map(|s| s.confidence).sum::<f32>()
                / non_voice_segments.len() as f32;
            let all_tags: Vec<String> = non_voice_segments
                .iter()
                .flat_map(|s| s.tags.clone())
                .collect();

            vec![ReviewSegment {
                index: 1,
                start_ms: first_start,
                end_ms: last_end,
                duration_ms,
                confidence: avg_confidence,
                tags: all_tags,
                prompt: None,
                verification_status: None,
            }]
        }
    } else {
        timeline
            .segments
            .iter()
            .filter(|segment| segment.kind == SegmentKind::NonVoice)
            .enumerate()
            .map(|(i, segment)| {
                let key = format!("{}-{}", segment.start_ms, segment.end_ms);
                let verification_status = verification_map.get(&key).cloned();
                ReviewSegment {
                    index: i + 1,
                    start_ms: segment.start_ms,
                    end_ms: segment.end_ms,
                    duration_ms: segment.end_ms.saturating_sub(segment.start_ms),
                    confidence: segment.confidence,
                    tags: segment.tags.clone(),
                    prompt: segment.prompt.clone(),
                    verification_status,
                }
            })
            .collect()
    };

    let segments_json = serde_json::to_string(&segments)?;
    let merged_json = serde_json::to_string(&merged)?;
    let html = format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Non-Voice Review Player</title>
  <style>
    :root {{
      --bg: #f5f3ef;
      --panel: #fffdf9;
      --text: #2a2118;
      --muted: #6a5a49;
      --accent: #b65328;
      --line: #e4dacd;
    }}
    body {{
      margin: 0;
      font-family: Georgia, "Times New Roman", serif;
      background: radial-gradient(circle at top, #fffaf3, var(--bg));
      color: var(--text);
    }}
    .layout {{
      max-width: 1100px;
      margin: 0 auto;
      padding: 20px;
      display: grid;
      gap: 20px;
      grid-template-columns: 2fr 1fr;
    }}
    .panel {{
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 14px;
      padding: 14px;
      box-shadow: 0 10px 24px rgba(72, 46, 25, 0.08);
    }}
    h1 {{ margin: 0 0 10px; font-size: 1.4rem; }}
    .muted {{ color: var(--muted); font-size: 0.95rem; }}
    video {{ width: 100%; border-radius: 10px; border: 1px solid var(--line); background: #000; }}
    .controls {{ display: flex; gap: 8px; flex-wrap: wrap; margin-top: 10px; }}
    .window-info {{ margin-top: 10px; border: 1px solid var(--line); border-radius: 10px; padding: 8px; background: #fff9f4; font-size: 0.92rem; }}
    .timeline-shell {{ margin-top: 10px; }}
    .timeline-track {{ position: relative; height: 18px; border: 1px solid var(--line); border-radius: 999px; background: #fbf6ef; overflow: hidden; }}
    .marker {{ position: absolute; top: 0; height: 100%; background: rgba(182, 83, 40, 0.55); cursor: pointer; }}
    .marker.active {{ background: rgba(182, 83, 40, 0.9); }}
    .playhead {{ position: absolute; top: 0; width: 2px; height: 100%; background: #1f140d; pointer-events: none; }}
    .timeline-legend {{ margin-top: 6px; font-size: 0.85rem; color: var(--muted); display: flex; justify-content: space-between; }}
    button {{
      border: 1px solid #ca9d83;
      background: #fff4ec;
      color: #4a2f1e;
      border-radius: 999px;
      padding: 6px 12px;
      cursor: pointer;
    }}
    button:hover {{ background: #ffe8da; }}
    .segment-list {{ max-height: 62vh; overflow: auto; display: grid; gap: 8px; }}
    .segment-controls {{ display: flex; gap: 8px; align-items: center; margin-bottom: 12px; flex-wrap: wrap; }}
    .segment-controls label {{ font-size: 0.85em; color: var(--muted); }}
    .segment-controls select {{ padding: 4px 8px; border: 1px solid var(--line); border-radius: 4px; font-family: inherit; }}
    .segment {{ border: 1px solid var(--line); border-radius: 10px; padding: 8px; }}
    .segment.active {{ border-color: var(--accent); box-shadow: 0 0 0 2px rgba(182, 83, 40, 0.2); }}
    .meta {{ font-size: 0.85rem; color: var(--muted); }}
    .badge {{
      display: inline-block;
      padding: 2px 8px;
      border-radius: 999px;
      border: 1px solid #d7b9a6;
      background: #fff8f3;
      margin-right: 4px;
      margin-top: 4px;
      font-size: 0.8rem;
    }}
    .badge-verified {{
      background: #d4edda;
      border-color: #28a745;
      color: #155724;
    }}
    .badge-suspicious {{
      background: #fff3cd;
      border-color: #ffc107;
      color: #856404;
    }}
    .badge-rejected {{
      background: #f8d7da;
      border-color: #dc3545;
      color: #721c24;
    }}
    .segment.suspicious {{
      border-left: 3px solid #ffc107;
      background: #fffdf5;
    }}
    .segment.rejected {{
      border-left: 3px solid #dc3545;
      background: #fff5f5;
    }}
    @media (max-width: 860px) {{
      .layout {{ grid-template-columns: 1fr; }}
      .segment-list {{ max-height: none; }}
    }}
  </style>
</head>
<body>
  <div class="layout">
    <section class="panel">
      <h1>Non-Voice Review Player</h1>
      <div class="muted" id="summary"></div>
      <video id="video" preload="metadata"></video>
      <div class="controls">
        <button type="button" id="prev">Previous (k)</button>
        <button type="button" id="play">Play Current (p)</button>
        <button type="button" id="play-all">Play Non-Voice Only (a)</button>
        <button type="button" id="toggle-full">Show Full Movie (f)</button>
        <button type="button" id="toggle-merged">Show as Single Block (m)</button>
        <button type="button" id="mark-voice">Mark Voice (x)</button>
        <button type="button" id="undo">Undo (u)</button>
        <button type="button" id="save-html">Save Reviewed HTML</button>
        <button type="button" id="export-learning">Export Learning Data (e)</button>
        <button type="button" id="pause">Pause (space)</button>
        <button type="button" id="next">Next (j)</button>
      </div>
      <div class="window-info" id="window-info"></div>
      <div class="timeline-shell">
        <div class="timeline-track" id="timeline-track">
          <div class="playhead" id="playhead"></div>
        </div>
        <div class="timeline-legend" id="timeline-legend"></div>
      </div>
      <p class="muted">This player is constrained to extracted non-voice windows only. Use Play Current or Play Non-Voice Only to validate timestamps quickly.</p>
      <p class="muted" id="status"></p>
    </section>
    <aside class="panel">
      <h1>Segments</h1>
      <div class="segment-controls">
        <label for="segment-filter">Filter:</label>
        <select id="segment-filter">
          <option value="all">All Segments</option>
          <option value="unverified">Unverified</option>
          <option value="verified">Verified</option>
          <option value="suspicious">Suspicious</option>
          <option value="excluded">Excluded</option>
        </select>
        <label for="segment-sort">Sort by:</label>
        <select id="segment-sort">
          <option value="time">Time</option>
          <option value="confidence">Confidence</option>
          <option value="duration">Duration</option>
        </select>
      </div>
      <div class="segment-list" id="segment-list"></div>
    </aside>
  </div>
  <script id="segments-data" type="application/json">{segments_json}</script>
  <script>
    const mediaSrc = {media_json};
    const preRoll = {pre_roll_json};
    const postRoll = {post_roll_json};
    const segmentsDataNode = document.getElementById('segments-data');
    const allSegments = JSON.parse(segmentsDataNode.textContent || '[]');
    const excluded = new Set();
    const undoStack = [];
    let segments = allSegments.slice();

    const video = document.getElementById('video');
    const segmentList = document.getElementById('segment-list');
    const summary = document.getElementById('summary');
    const status = document.getElementById('status');
    const windowInfo = document.getElementById('window-info');
    const timelineTrack = document.getElementById('timeline-track');
    const timelineLegend = document.getElementById('timeline-legend');
    const playhead = document.getElementById('playhead');
    const btnPrev = document.getElementById('prev');
    const btnPlay = document.getElementById('play');
    const btnPlayAll = document.getElementById('play-all');
    const btnToggleFull = document.getElementById('toggle-full');
    const btnToggleMerged = document.getElementById('toggle-merged');
    const btnMarkVoice = document.getElementById('mark-voice');
    const btnUndo = document.getElementById('undo');
    const btnSaveHtml = document.getElementById('save-html');
    const btnExportLearning = document.getElementById('export-learning');
    const btnPause = document.getElementById('pause');
    const btnNext = document.getElementById('next');
    const btnSegmentFilter = document.getElementById('segment-filter');
    const btnSegmentSort = document.getElementById('segment-sort');

    video.src = mediaSrc;
    let currentIndex = 0;
    let stopAt = null;
    let playAll = false;
    let nonVoiceOnlyMode = true;
    let mergedMode = {merged_json};
    let currentFilter = 'all';
    let currentSort = 'time';

    function segKey(seg) {{
      return `${{seg.start_ms}}-${{seg.end_ms}}`;
    }}

    function refreshSegments() {{
      let filtered = mergedMode ? getMergedSegments() : allSegments.filter(seg => !excluded.has(segKey(seg)));
      if (currentFilter === 'verified') {{
        filtered = filtered.filter(seg => seg.verification_status === 'verified');
      }} else if (currentFilter === 'unverified') {{
        filtered = filtered.filter(seg => !seg.verification_status);
      }} else if (currentFilter === 'suspicious') {{
        filtered = filtered.filter(seg => seg.verification_status === 'suspicious');
      }} else if (currentFilter === 'excluded') {{
        filtered = filtered.filter(seg => excluded.has(segKey(seg)));
      }}
      segments = filtered;
      if (currentIndex >= segments.length) {{
        currentIndex = Math.max(0, segments.length - 1);
      }}
      segmentsDataNode.textContent = JSON.stringify(segments);
    }}

    function saveReviewedHtml() {{
      const htmlDoc = '<!doctype html>\n' + document.documentElement.outerHTML;
      const blob = new Blob([htmlDoc], {{ type: 'text/html' }});
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'nonvoice-review-reviewed.html';
      document.body.appendChild(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(url), 5000);
    }}

    function exportLearningData() {{
      const excludedSegments = allSegments.filter(seg => excluded.has(segKey(seg)));
      const learningData = {{
        export_timestamp: new Date().toISOString(),
        total_segments: allSegments.length,
        marked_as_voice_count: excludedSegments.length,
        segments_marked_as_voice: excludedSegments.map(seg => ({{
          start_ms: seg.start_ms,
          end_ms: seg.end_ms,
          confidence: seg.confidence,
          duration_ms: seg.end_ms - seg.start_ms,
          verification_status: seg.verification_status || null
        }}))
      }};
      const json = JSON.stringify(learningData, null, 2);
      const blob = new Blob([json], {{ type: 'application/json' }});
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'learning-false-positives.json';
      document.body.appendChild(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(url), 5000);
      status.textContent = `Exported ${{excludedSegments.length}} false positives for learning`;
    }}

    function formatClock(seconds) {{
      const totalMs = Math.max(0, Math.round(seconds * 1000));
      const h = Math.floor(totalMs / 3600000);
      const m = Math.floor((totalMs % 3600000) / 60000);
      const s = Math.floor((totalMs % 60000) / 1000);
      const ms = totalMs % 1000;
      if (h > 0) return `${{String(h).padStart(2, '0')}}:${{String(m).padStart(2, '0')}}:${{String(s).padStart(2, '0')}}.${{String(ms).padStart(3, '0')}}`;
      return `${{String(m).padStart(2, '0')}}:${{String(s).padStart(2, '0')}}.${{String(ms).padStart(3, '0')}}`;
    }}

    function seconds(ms) {{
      return (ms / 1000).toFixed(2);
    }}

    function renderList() {{
      if (segments.length === 0) {{
        segmentList.innerHTML = '<p class="muted">No non-voice segments found in this JSON.</p>';
        summary.textContent = '0 non-voice segments';
        status.textContent = 'No playable non-voice windows in this file.';
        btnPrev.disabled = true;
        btnPlay.disabled = true;
        btnPlayAll.disabled = true;
        btnMarkVoice.disabled = true;
        btnPause.disabled = true;
        btnNext.disabled = true;
        btnUndo.disabled = undoStack.length === 0;
        return;
      }}
      summary.textContent = `${{segments.length}} non-voice segments (reviewed) | preroll ${{preRoll}}s | postroll ${{postRoll}}s`;
      status.textContent = 'Ready. Player will stay inside non-voice timestamps only.';
      btnPrev.disabled = false;
      btnPlay.disabled = false;
      btnPlayAll.disabled = false;
      btnMarkVoice.disabled = false;
      btnPause.disabled = false;
      btnNext.disabled = false;
      btnUndo.disabled = undoStack.length === 0;
      segmentList.innerHTML = '';
      for (let i = 0; i < segments.length; i++) {{
        const seg = segments[i];
        const el = document.createElement('div');
        el.className = 'segment';
        if (seg.verification_status === 'suspicious') el.classList.add('suspicious');
        if (seg.verification_status === 'rejected') el.classList.add('rejected');
        el.dataset.index = String(i);
        const tags = (seg.tags || []).map(t => `<span class="badge">${{t}}</span>`).join('');
        let verificationBadge = '';
        if (seg.verification_status === 'verified') {{
          verificationBadge = '<span class="badge badge-verified">Verified</span>';
        }} else if (seg.verification_status === 'suspicious') {{
          verificationBadge = '<span class="badge badge-suspicious">Suspicious</span>';
        }} else if (seg.verification_status === 'rejected') {{
          verificationBadge = '<span class="badge badge-rejected">Rejected</span>';
        }}
        const prompt = seg.prompt ? `<div class="meta">Prompt: ${{seg.prompt}}</div>` : '';
        el.innerHTML = `
          <div><strong>#${{seg.index}}</strong> ${{seconds(seg.start_ms)}}s - ${{seconds(seg.end_ms)}}s ${{verificationBadge}}</div>
          <div class="meta">duration ${{seconds(seg.duration_ms)}}s | confidence ${{Number(seg.confidence).toFixed(2)}}</div>
          <div>${{tags}}</div>
          ${{prompt}}
        `;
        el.addEventListener('click', () => {{
          currentIndex = i;
          jumpToCurrent(true);
        }});
        segmentList.appendChild(el);
      }}
      highlight();
    }}

    function highlight() {{
      const items = segmentList.querySelectorAll('.segment');
      for (const item of items) item.classList.remove('active');
      const active = segmentList.querySelector(`.segment[data-index="${{currentIndex}}"]`);
      if (active) active.classList.add('active');

      const markers = timelineTrack.querySelectorAll('.marker');
      for (const marker of markers) marker.classList.remove('active');
      const activeMarker = timelineTrack.querySelector(`.marker[data-index="${{currentIndex}}"]`);
      if (activeMarker) activeMarker.classList.add('active');

      if (segments.length > 0) {{
        const seg = segments[currentIndex];
        windowInfo.innerHTML = `<strong>Selected:</strong> #${{seg.index}} | <strong>start:</strong> ${{formatClock(seg.start_ms / 1000)}} | <strong>end:</strong> ${{formatClock(seg.end_ms / 1000)}} | <strong>duration:</strong> ${{seconds(seg.duration_ms)}}s | <strong>confidence:</strong> ${{Number(seg.confidence).toFixed(2)}}`;
      }} else {{
        windowInfo.textContent = 'All reviewed segments are currently marked as voice.';
      }}
    }}

    function mediaDurationMs() {{
      const fromVideo = Number.isFinite(video.duration) && video.duration > 0 ? Math.round(video.duration * 1000) : 0;
      if (fromVideo > 0) return fromVideo;
      let maxEnd = 0;
      for (const seg of segments) {{
        if (seg.end_ms > maxEnd) maxEnd = seg.end_ms;
      }}
      return maxEnd;
    }}

    function renderTimelineMarkers() {{
      const existing = timelineTrack.querySelectorAll('.marker');
      for (const el of existing) el.remove();

      const totalMs = mediaDurationMs();
      if (totalMs <= 0 || segments.length === 0) {{
        timelineLegend.textContent = 'timeline unavailable';
        return;
      }}

      timelineLegend.innerHTML = `<span>00:00.000</span><span>${{formatClock(totalMs / 1000)}}</span>`;

      for (let i = 0; i < segments.length; i++) {{
        const seg = segments[i];
        const marker = document.createElement('div');
        marker.className = 'marker';
        marker.dataset.index = String(i);
        const leftPct = Math.max(0, Math.min(100, (seg.start_ms / totalMs) * 100));
        const widthPct = Math.max(0.35, (seg.duration_ms / totalMs) * 100);
        marker.style.left = `${{leftPct}}%`;
        marker.style.width = `${{Math.min(100 - leftPct, widthPct)}}%`;
        marker.title = `#${{seg.index}} ${{formatClock(seg.start_ms / 1000)}} - ${{formatClock(seg.end_ms / 1000)}} (${{seconds(seg.duration_ms)}}s)`;
        marker.addEventListener('click', () => {{
          currentIndex = i;
          jumpToCurrent(false);
        }});
        timelineTrack.appendChild(marker);
      }}
      highlight();
    }}

    function jumpToCurrent(playNow) {{
      if (segments.length === 0) return;
      const seg = segments[currentIndex];
      const startAt = Math.max(0, seg.start_ms / 1000 - preRoll);
      stopAt = seg.end_ms / 1000 + postRoll;
      try {{
        video.currentTime = startAt;
      }} catch (_err) {{
        // Some browsers throw if media failed to load; keep list navigation usable.
      }}
      highlight();
      if (playNow) {{
        const playPromise = video.play();
        if (playPromise && typeof playPromise.catch === 'function') {{
          playPromise.catch(() => {{}});
        }}
      }}
      status.textContent = `Segment #${{seg.index}} | ${{seconds(seg.start_ms)}}s - ${{seconds(seg.end_ms)}}s`;
    }}

    function updateModeUi() {{
      if (nonVoiceOnlyMode) {{
        btnToggleFull.textContent = 'Show Full Movie (f)';
        video.controls = false;
        status.textContent = 'Non-voice-only mode enabled.';
      }} else {{
        btnToggleFull.textContent = 'Lock Non-Voice Only (f)';
        video.controls = true;
        stopAt = null;
        playAll = false;
        status.textContent = 'Full movie mode enabled (manual verification context).';
      }}
    }}

    function updateMergedUi() {{
      if (mergedMode) {{
        btnToggleMerged.textContent = 'Show Individual Segments (m)';
        status.textContent = 'Merged view: all non-voice shown as single block.';
      }} else {{
        btnToggleMerged.textContent = 'Show as Single Block (m)';
        status.textContent = 'Segment view: individual non-voice segments.';
      }}
      renderList();
      renderTimelineMarkers();
    }}

    function getMergedSegments() {{
      if (!mergedMode) return allSegments.filter(seg => !excluded.has(segKey(seg)));
      
      const nonVoice = allSegments.filter(seg => seg.kind === 'non_voice' || !seg.kind);
      if (nonVoice.length === 0) return [];
      
      const firstStart = nonVoice[0].start_ms;
      const lastEnd = nonVoice[nonVoice.length - 1].end_ms;
      const duration = lastEnd - firstStart;
      const avgConf = nonVoice.reduce((sum, s) => sum + s.confidence, 0) / nonVoice.length;
      const allTags = [...new Set(nonVoice.flatMap(s => s.tags || []))];
      
      return [{{
        index: 1,
        start_ms: firstStart,
        end_ms: lastEnd,
        duration_ms: duration,
        confidence: avgConf,
        tags: allTags,
        prompt: null,
        verification_status: null,
      }}];
    }}

    function getActiveSegments() {{
      return mergedMode ? getMergedSegments() : segments;
    }}

    btnPrev.addEventListener('click', () => {{
      if (segments.length === 0) return;
      currentIndex = (currentIndex - 1 + segments.length) % segments.length;
      jumpToCurrent(true);
    }});

    btnNext.addEventListener('click', () => {{
      if (segments.length === 0) return;
      currentIndex = (currentIndex + 1) % segments.length;
      jumpToCurrent(true);
    }});

    btnPlay.addEventListener('click', () => jumpToCurrent(true));
    btnPlayAll.addEventListener('click', () => {{
      if (segments.length === 0) return;
      playAll = true;
      jumpToCurrent(true);
    }});
    btnToggleFull.addEventListener('click', () => {{
      nonVoiceOnlyMode = !nonVoiceOnlyMode;
      updateModeUi();
      if (nonVoiceOnlyMode && segments.length > 0) {{
        jumpToCurrent(false);
      }}
    }});
    btnToggleMerged.addEventListener('click', () => {{
      mergedMode = !mergedMode;
      updateMergedUi();
      if (segments.length > 0) jumpToCurrent(false);
    }});
    btnMarkVoice.addEventListener('click', () => {{
      if (segments.length === 0) return;
      const seg = segments[currentIndex];
      const key = segKey(seg);
      if (excluded.has(key)) return;
      excluded.add(key);
      undoStack.push(key);
      playAll = false;
      video.pause();
      refreshSegments();
      renderList();
      renderTimelineMarkers();
      if (segments.length > 0) {{
        jumpToCurrent(false);
      }}
    }});
    btnUndo.addEventListener('click', () => {{
      const key = undoStack.pop();
      if (!key) return;
      excluded.delete(key);
      refreshSegments();
      renderList();
      renderTimelineMarkers();
      if (segments.length > 0) jumpToCurrent(false);
    }});
    btnSaveHtml.addEventListener('click', saveReviewedHtml);
    btnExportLearning.addEventListener('click', exportLearningData);
    btnPause.addEventListener('click', () => {{
      playAll = false;
      video.pause();
    }});

    btnSegmentFilter.addEventListener('change', (e) => {{
      currentFilter = e.target.value;
      refreshSegments();
      renderList();
    }});
    btnSegmentSort.addEventListener('change', (e) => {{
      currentSort = e.target.value;
      if (currentSort === 'time') {{
        segments.sort((a, b) => a.start_ms - b.start_ms);
      }} else if (currentSort === 'confidence') {{
        segments.sort((a, b) => b.confidence - a.confidence);
      }} else if (currentSort === 'duration') {{
        segments.sort((a, b) => (b.end_ms - b.start_ms) - (a.end_ms - a.start_ms));
      }}
      currentIndex = 0;
      renderList();
      jumpToCurrent(false);
    }});

    document.addEventListener('keydown', (event) => {{
      if (event.key === ' ') {{
        event.preventDefault();
        btnPause.click();
      }}
      if (event.key === 'j') btnNext.click();
      if (event.key === 'k') btnPrev.click();
      if (event.key === 'm') btnToggleMerged.click();
      if (event.key === 'p') btnPlay.click();
      if (event.key === 'a') btnPlayAll.click();
      if (event.key === 'f') btnToggleFull.click();
      if (event.key === 'x') btnMarkVoice.click();
      if (event.key === 'u') btnUndo.click();
      if (event.key === 'e') btnExportLearning.click();
      if ((event.ctrlKey || event.metaKey) && event.key === 's') {{
        event.preventDefault();
        saveReviewedHtml();
      }}
    }});

    video.addEventListener('play', () => {{
      if (!nonVoiceOnlyMode) return;
      if (segments.length === 0) {{
        video.pause();
        return;
      }}
      const seg = segments[currentIndex];
      const allowedStart = Math.max(0, seg.start_ms / 1000 - preRoll);
      const allowedEnd = seg.end_ms / 1000 + postRoll;
      if (video.currentTime < allowedStart || video.currentTime > allowedEnd) {{
        try {{
          video.currentTime = allowedStart;
        }} catch (_err) {{}}
      }}
      stopAt = allowedEnd;
    }});

    video.addEventListener('timeupdate', () => {{
      const total = mediaDurationMs();
      if (total > 0) {{
        const leftPct = Math.max(0, Math.min(100, (video.currentTime * 1000 / total) * 100));
        playhead.style.left = `${{leftPct}}%`;
      }}
      if (!nonVoiceOnlyMode) return;
      if (stopAt !== null && video.currentTime >= stopAt) {{
        if (playAll && currentIndex + 1 < segments.length) {{
          currentIndex += 1;
          jumpToCurrent(true);
        }} else {{
          playAll = false;
          video.pause();
          btnPlayAll.textContent = 'Play All (a) - Done!';
          setTimeout(() => {{ btnPlayAll.textContent = 'Play All (a)'; }}, 2000);
        }}
      }}
    }});

    refreshSegments();
    renderList();
    video.addEventListener('loadedmetadata', renderTimelineMarkers);
    renderTimelineMarkers();
    updateModeUi();
    jumpToCurrent(false);
  </script>
</body>
</html>
"#
    );

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, html)?;
    Ok(segments.len())
}
