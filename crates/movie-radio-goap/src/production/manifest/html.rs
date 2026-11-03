//! Self-contained HTML rendering of a run manifest.

use std::fmt::Write;

use super::{GateStatus, RunManifest, RunStatus, StageRunStatus};

pub(super) fn render(manifest: &RunManifest) -> String {
    let mut html = String::with_capacity(16 * 1024);
    html.push_str(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <title>Production report</title><style>\
         body{font-family:system-ui,sans-serif;margin:2rem;color:#1f2328}\
         table{border-collapse:collapse;margin:0 0 1.5rem;width:100%}\
         th,td{border:1px solid #d0d7de;padding:.35rem .5rem;text-align:left;vertical-align:top}\
         th{background:#f6f8fa}code{font-size:.85em}\
         .pass{color:#1a7f37}.warn{color:#9a6700}.fail{color:#cf222e}\
         .skipped{color:#57606a}.muted{color:#57606a}</style></head><body>",
    );
    let _ = write!(
        html,
        "<h1>Production report</h1><p class=\"{}\"><strong>{:?}</strong> &middot; run <code>{}</code> \
         &middot; generated {} ms &middot; {} {}</p>",
        status_class(manifest.status),
        manifest.status,
        escape(&manifest.run_id),
        manifest.generated_unix_ms,
        escape(&manifest.tool.name),
        escape(&manifest.tool.version)
    );
    html.push_str("<table><tbody>");
    row(&mut html, "Input", &escape(&manifest.input.path));
    row(
        &mut html,
        "Input fingerprint",
        &format!(
            "<code>{}</code> ({} bytes)",
            escape(&manifest.input_fingerprint),
            manifest.input.size_bytes
        ),
    );
    row(
        &mut html,
        "Config hash",
        &format!("<code>{}</code>", escape(&manifest.config_hash)),
    );
    row(
        &mut html,
        "Schema",
        &format!("v{}", manifest.schema_version),
    );
    html.push_str("</tbody></table>");

    let finished = manifest
        .stages
        .iter()
        .filter(|stage| stage.status.is_finished())
        .count();
    let _ = write!(
        html,
        "<p>{} of {} stages finished &middot; {} artifacts &middot; {} narration items \
         &middot; {} SFX items &middot; {} warnings</p>",
        finished,
        manifest.stages.len(),
        manifest.artifacts.len(),
        manifest.narration.len(),
        manifest.sfx.len(),
        manifest.warnings.len()
    );

    html.push_str("<h2>Providers</h2><table><tbody>");
    let providers = &manifest.providers;
    row(
        &mut html,
        "VAD engine",
        &escape(providers.vad_engine.as_deref().unwrap_or("unknown")),
    );
    row(
        &mut html,
        "VAD model",
        &escape(providers.vad_model.as_deref().unwrap_or("—")),
    );
    row(&mut html, "Cue source", &escape(&providers.cue_source));
    row(
        &mut html,
        "Language",
        &escape(providers.language.as_deref().unwrap_or("unknown")),
    );
    row(
        &mut html,
        "Narrator",
        &escape(providers.narrator_provider.as_deref().unwrap_or("unknown")),
    );
    row(
        &mut html,
        "Narrator model",
        &escape(providers.narrator_model.as_deref().unwrap_or("—")),
    );
    row(
        &mut html,
        "TTS chain",
        &escape(&providers.fallback_chain.join(" → ")),
    );
    let models: Vec<String> = providers
        .tts_models
        .iter()
        .map(|(provider, model)| format!("{provider}: {model}"))
        .collect();
    row(&mut html, "TTS models", &escape(&models.join(", ")));
    row(
        &mut html,
        "SFX providers",
        &escape(&providers.sfx_providers.join(", ")),
    );
    row(
        &mut html,
        "Mastering profile",
        &escape(providers.mastering_profile.as_deref().unwrap_or("default")),
    );
    html.push_str("</tbody></table>");

    html.push_str(
        "<h2>Quality gates</h2><table><thead><tr><th>Gate</th><th>Status</th>\
                   <th>Detail</th></tr></thead><tbody>",
    );
    for gate in &manifest.quality_gates {
        let _ = write!(
            html,
            "<tr><td>{}</td><td class=\"{}\">{:?}</td><td>{}</td></tr>",
            escape(&gate.name),
            gate_class(gate.status),
            gate.status,
            escape(&gate.detail)
        );
    }
    html.push_str("</tbody></table>");

    html.push_str(
        "<h2>Stages</h2><table><thead><tr><th>Stage</th><th>Status</th>\
                   <th>Provider</th><th>Artifacts</th></tr></thead><tbody>",
    );
    for stage in &manifest.stages {
        let artifacts: Vec<String> = stage
            .artifacts
            .iter()
            .map(|artifact| {
                let checksum = artifact.checksum.as_deref().unwrap_or("missing");
                format!(
                    "<code>{}</code> {} ({} bytes, checksum {})",
                    escape(&artifact.key),
                    escape(&artifact.path),
                    artifact.size_bytes,
                    checksum
                )
            })
            .collect();
        let error = stage
            .error
            .as_deref()
            .map(|error| format!("<br><span class=\"fail\">{}</span>", escape(error)))
            .unwrap_or_default();
        let _ = write!(
            html,
            "<tr><td><code>{}</code>{}{}</td><td class=\"{}\">{:?}</td><td>{}</td><td>{}</td></tr>",
            escape(&stage.name),
            if stage.optional { " (optional)" } else { "" },
            error,
            stage_class(stage.status),
            stage.status,
            escape(stage.provider.as_deref().unwrap_or("—")),
            artifacts.join("<br>")
        );
    }
    html.push_str("</tbody></table>");

    if !manifest.omitted_stages.is_empty() {
        let _ = write!(
            html,
            "<p class=\"muted\">Omitted optional stages: {}</p>",
            escape(&manifest.omitted_stages.join(", "))
        );
    }

    if !manifest.warnings.is_empty() {
        html.push_str("<h2>Warnings</h2><ul>");
        for warning in &manifest.warnings {
            let _ = write!(html, "<li>{}</li>", escape(warning));
        }
        html.push_str("</ul>");
    }

    html.push_str(
        "<h2>Narration</h2><table><thead><tr><th>#</th><th>Source interval</th>\
                   <th>Output interval</th><th>Provider</th><th>Text</th></tr></thead><tbody>",
    );
    for link in &manifest.narration {
        let output = match (link.output_start_ms, link.output_end_ms) {
            (Some(start), Some(end)) => format!("{start}–{end} ms"),
            _ => "not synthesized".to_string(),
        };
        let _ = write!(
            html,
            "<tr><td>{}</td><td>{}–{} ms</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            link.index,
            link.source_start_ms,
            link.source_end_ms,
            escape(&output),
            escape(link.provider.as_deref().unwrap_or("—")),
            escape(&link.text)
        );
    }
    html.push_str("</tbody></table>");

    html.push_str(
        "<h2>Sound effects</h2><table><thead><tr><th>ID</th>\
                   <th>Source interval</th><th>Output interval</th><th>Asset</th>\
                   <th>License</th></tr></thead><tbody>",
    );
    for link in &manifest.sfx {
        let _ = write!(
            html,
            "<tr><td><code>{}</code></td><td>{}–{} ms</td><td>{}–{} ms</td>\
             <td>{} ({})</td><td>{}</td></tr>",
            escape(&link.id),
            link.source_start_ms,
            link.source_end_ms,
            link.output_start_ms,
            link.output_end_ms,
            escape(&link.asset_id),
            escape(&link.source),
            escape(&link.license)
        );
    }
    html.push_str("</tbody></table>");

    html.push_str("<h2>Usage</h2><table><tbody>");
    row(
        &mut html,
        "Narration requests",
        &manifest.usage.narration_requests.to_string(),
    );
    row(
        &mut html,
        "Narration succeeded",
        &manifest.usage.narration_succeeded.to_string(),
    );
    row(
        &mut html,
        "Narration failed",
        &manifest.usage.narration_failed.to_string(),
    );
    row(
        &mut html,
        "SFX assets used",
        &manifest.usage.sfx_assets_used.to_string(),
    );
    row(
        &mut html,
        "Reported cost",
        &manifest
            .usage
            .total_cost_usd
            .map_or_else(|| "not reported".to_string(), |cost| format!("${cost:.2}")),
    );
    html.push_str("</tbody></table></body></html>");
    html
}

fn row(html: &mut String, label: &str, value: &str) {
    let _ = write!(
        html,
        "<tr><th>{}</th><td>{}</td></tr>",
        escape(label),
        value
    );
}

fn status_class(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Complete => "pass",
        RunStatus::Incomplete => "warn",
        RunStatus::Failed => "fail",
    }
}

fn gate_class(status: GateStatus) -> &'static str {
    match status {
        GateStatus::Pass => "pass",
        GateStatus::Warn => "warn",
        GateStatus::Fail => "fail",
        GateStatus::Skipped => "skipped",
    }
}

fn stage_class(status: StageRunStatus) -> &'static str {
    match status {
        StageRunStatus::Completed | StageRunStatus::Reused => "pass",
        StageRunStatus::Failed => "fail",
        StageRunStatus::Skipped | StageRunStatus::NotRun => "skipped",
    }
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}
