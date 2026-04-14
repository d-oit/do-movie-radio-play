use anyhow::Result;
use serde::Serialize;
use std::path::Path;

use crate::types::{SegmentKind, TimelineOutput};

#[derive(Debug, Clone, Serialize)]
struct ReviewSegment {
    index: usize,
    start_ms: u64,
    end_ms: u64,
    duration_ms: u64,
    confidence: f32,
    tags: Vec<String>,
    prompt: Option<String>,
}

pub fn write_review_html(
    input_media: &Path,
    timeline: &TimelineOutput,
    output: &Path,
    pre_roll_s: f32,
    post_roll_s: f32,
) -> Result<usize> {
    let media_path = input_media.to_string_lossy().to_string();
    let media_json = serde_json::to_string(&media_path)?;
    let pre_roll_json = serde_json::to_string(&pre_roll_s)?;
    let post_roll_json = serde_json::to_string(&post_roll_s)?;

    let segments: Vec<ReviewSegment> = timeline
        .segments
        .iter()
        .filter(|segment| segment.kind == SegmentKind::NonVoice)
        .enumerate()
        .map(|(i, segment)| ReviewSegment {
            index: i + 1,
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
            duration_ms: segment.end_ms.saturating_sub(segment.start_ms),
            confidence: segment.confidence,
            tags: segment.tags.clone(),
            prompt: segment.prompt.clone(),
        })
        .collect();

    let segments_json = serde_json::to_string(&segments)?;
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
      <video id="video" controls preload="metadata"></video>
      <div class="controls">
        <button type="button" id="prev">Previous (k)</button>
        <button type="button" id="play">Play Current (p)</button>
        <button type="button" id="next">Next (j)</button>
      </div>
      <p class="muted">This player jumps to each non-voice segment with configurable pre/post roll so you can quickly verify if each segment is truly non-voice.</p>
    </section>
    <aside class="panel">
      <h1>Segments</h1>
      <div class="segment-list" id="segment-list"></div>
    </aside>
  </div>
  <script>
    const mediaSrc = {media_json};
    const preRoll = {pre_roll_json};
    const postRoll = {post_roll_json};
    const segments = {segments_json};

    const video = document.getElementById('video');
    const segmentList = document.getElementById('segment-list');
    const summary = document.getElementById('summary');
    const btnPrev = document.getElementById('prev');
    const btnPlay = document.getElementById('play');
    const btnNext = document.getElementById('next');

    video.src = mediaSrc;
    let currentIndex = 0;
    let stopAt = null;

    function seconds(ms) {{
      return (ms / 1000).toFixed(2);
    }}

    function renderList() {{
      if (segments.length === 0) {{
        segmentList.innerHTML = '<p class="muted">No non-voice segments found in this JSON.</p>';
        summary.textContent = '0 non-voice segments';
        return;
      }}
      summary.textContent = `${{segments.length}} non-voice segments | preroll ${{preRoll}}s | postroll ${{postRoll}}s`;
      segmentList.innerHTML = '';
      for (const seg of segments) {{
        const el = document.createElement('div');
        el.className = 'segment';
        el.dataset.index = String(seg.index - 1);
        const tags = (seg.tags || []).map(t => `<span class="badge">${{t}}</span>`).join('');
        const prompt = seg.prompt ? `<div class="meta">Prompt: ${{seg.prompt}}</div>` : '';
        el.innerHTML = `
          <div><strong>#${{seg.index}}</strong> ${{seconds(seg.start_ms)}}s - ${{seconds(seg.end_ms)}}s</div>
          <div class="meta">duration ${{seconds(seg.duration_ms)}}s | confidence ${{Number(seg.confidence).toFixed(2)}}</div>
          <div>${{tags}}</div>
          ${{prompt}}
        `;
        el.addEventListener('click', () => {{
          currentIndex = seg.index - 1;
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
    }}

    function jumpToCurrent(playNow) {{
      if (segments.length === 0) return;
      const seg = segments[currentIndex];
      const startAt = Math.max(0, seg.start_ms / 1000 - preRoll);
      stopAt = seg.end_ms / 1000 + postRoll;
      video.currentTime = startAt;
      highlight();
      if (playNow) video.play();
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

    document.addEventListener('keydown', (event) => {{
      if (event.key === 'j') btnNext.click();
      if (event.key === 'k') btnPrev.click();
      if (event.key === 'p') btnPlay.click();
    }});

    video.addEventListener('timeupdate', () => {{
      if (stopAt !== null && video.currentTime >= stopAt) {{
        video.pause();
      }}
    }});

    renderList();
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
