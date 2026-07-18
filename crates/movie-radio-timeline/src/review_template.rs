//! HTML template for the review player.
//! Contains the full interactive player UI with timeline, segment list, and controls.
//!
//! The HTML template is loaded from `templates/review.html` at compile time
//! via [`include_str!`]. This keeps the Rust source lean and allows the
//! template to be edited independently.

/// Timeout in milliseconds before revoking a blob URL created during
/// Save-Reviewed-HTML or Export-Learning-Data operations.
pub const URL_REVOKE_TIMEOUT_MS: u64 = 5000;

/// Escape JSON for safe embedding in a `<script>` tag.
/// Escapes `<`, `>`, `&`, Unicode line/paragraph separators, and backtick.
pub fn escape_json_for_script(json: String) -> String {
    // Robustly escape JSON for embedding in a <script> tag by escaping characters
    // that could be used for tag breakout or other injection attacks.
    // Optimization: Use a single pass with a pre-allocated buffer to reduce allocations.
    let mut escaped = String::with_capacity(json.len() + 32);
    for c in json.chars() {
        match c {
            '<' => escaped.push_str("\\u003c"),
            '>' => escaped.push_str("\\u003e"),
            '&' => escaped.push_str("\\u0026"),
            '\u{2028}' => escaped.push_str("\\u2028"),
            '\u{2029}' => escaped.push_str("\\u2029"),
            '`' => escaped.push_str("\\u0060"),
            _ => escaped.push(c),
        }
    }
    escaped
}

/// Render the complete review player HTML page.
/// All JSON parameters must be pre-escaped via [`escape_json_for_script`].
pub fn render_review_html(
    segments_json: &str,
    media_json: &str,
    pre_roll_json: &str,
    post_roll_json: &str,
    merged_json: &str,
) -> String {
    // Trivial comment to force recompilation and template reloading
    format!(
        include_str!("../../../templates/review.html"),
        segments_json = segments_json,
        media_json = media_json,
        pre_roll_json = pre_roll_json,
        post_roll_json = post_roll_json,
        merged_json = merged_json,
        url_revoke_timeout_ms = URL_REVOKE_TIMEOUT_MS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_loads_and_formats() {
        let html = render_review_html("[]", r#""movie.mp4""#, "1.0", "1.0", "false");
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("Non-Voice Review Player"));
        assert!(html.contains("movie.mp4"));
        assert!(html.contains("5000"));
    }

    #[test]
    fn test_url_revoke_constant_is_used_in_output() {
        let html = render_review_html("[]", r#""test.mp4""#, "0.5", "0.5", "true");
        // The constant 5000 should appear as the URL.revokeObjectURL timeout
        let revoke_calls = html.matches("URL.revokeObjectURL(url), 5000").count();
        assert_eq!(
            revoke_calls, 2,
            "expected 2 revokeObjectURL calls with 5000ms timeout, found {revoke_calls}"
        );
    }

    #[test]
    fn test_escape_json_for_script() {
        let input = r#"<script>alert("xss")</script>"#.to_string();
        let escaped = escape_json_for_script(input);
        assert!(!escaped.contains('<'));
        assert!(!escaped.contains('>'));
        assert!(escaped.contains("\\u003c"));
        assert!(escaped.contains("\\u003e"));
    }

    #[test]
    fn test_escape_ampersand() {
        let input = "rock & roll".to_string();
        let escaped = escape_json_for_script(input);
        assert!(escaped.contains("\\u0026"));
    }

    #[test]
    fn test_escape_line_terminators() {
        let input = "line\u{2028}para\u{2029}back`tick".to_string();
        let escaped = escape_json_for_script(input);
        assert!(escaped.contains("\\u2028"));
        assert!(escaped.contains("\\u2029"));
        assert!(escaped.contains("\\u0060"));
    }
}
