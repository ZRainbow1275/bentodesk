//! Defensive SVG sanitisation for user-uploaded custom icons.
//!
//! The backend does a **conservative** strip of obviously dangerous constructs
//! before writing the SVG to disk. The frontend repeats sanitisation with
//! DOMPurify before rendering (defense in depth).
//!
//! We intentionally avoid pulling in `resvg` / `usvg` for size reasons — they
//! add ~2 MB to the release binary. Instead we:
//!
//!   - Drop `<script>`, `<foreignObject>`, `<iframe>` subtrees.
//!   - Strip every attribute starting with `on` (event handlers).
//!   - Neutralise `href`/`xlink:href` that point to `javascript:` or
//!     `data:text/html`.
//!   - Reject the file outright if the resulting markup is not well-formed
//!     SVG (checked via presence of `<svg` root).

use regex::Regex;
use std::sync::OnceLock;

static RE_SCRIPT: OnceLock<Regex> = OnceLock::new();
static RE_FOREIGN: OnceLock<Regex> = OnceLock::new();
static RE_IFRAME: OnceLock<Regex> = OnceLock::new();
static RE_EVENT_ATTR: OnceLock<Regex> = OnceLock::new();
static RE_JS_URL: OnceLock<Regex> = OnceLock::new();
static RE_EXTERNAL_HREF: OnceLock<Regex> = OnceLock::new();

fn compile(re: &'static OnceLock<Regex>, pat: &str) -> &'static Regex {
    re.get_or_init(|| Regex::new(pat).expect("sanitise regex must compile"))
}

/// Sanitise an SVG document. Returns `Ok(cleaned)` on success, or
/// `Err(reason)` when the file is rejected.
pub fn sanitize_svg(raw: &str) -> Result<String, String> {
    if raw.len() > 512 * 1024 {
        return Err("SVG too large (>512 KB)".into());
    }
    let lower = raw.to_ascii_lowercase();
    if !lower.contains("<svg") {
        return Err("Input does not contain an <svg> root".into());
    }

    let mut cleaned = raw.to_string();

    cleaned = compile(&RE_SCRIPT, r"(?is)<script.*?</script>")
        .replace_all(&cleaned, "")
        .into_owned();
    cleaned = compile(&RE_FOREIGN, r"(?is)<foreignObject.*?</foreignObject>")
        .replace_all(&cleaned, "")
        .into_owned();
    cleaned = compile(&RE_IFRAME, r"(?is)<iframe.*?</iframe>")
        .replace_all(&cleaned, "")
        .into_owned();
    cleaned = compile(&RE_EVENT_ATTR, r#"(?i)\s+on\w+\s*=\s*("[^"]*"|'[^']*')"#)
        .replace_all(&cleaned, "")
        .into_owned();
    cleaned = compile(&RE_JS_URL, r#"(?i)javascript:[^\s"']*"#)
        .replace_all(&cleaned, "#")
        .into_owned();
    cleaned = compile(
        &RE_EXTERNAL_HREF,
        r#"(?i)(?:xlink:)?href\s*=\s*"(?:https?:|data:text/html)[^"]*""#,
    )
    .replace_all(&cleaned, r##"href="#""##)
    .into_owned();

    Ok(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_svg() {
        assert!(sanitize_svg("<html>bad</html>").is_err());
    }

    #[test]
    fn strips_script_tag() {
        let raw = "<svg><script>alert(1)</script><circle r='5'/></svg>";
        let clean = sanitize_svg(raw).unwrap();
        assert!(!clean.contains("script"));
        assert!(clean.contains("circle"));
    }

    #[test]
    fn strips_onload_attribute() {
        let raw = r#"<svg onload="alert(1)" width="10"><rect/></svg>"#;
        let clean = sanitize_svg(raw).unwrap();
        assert!(!clean.to_ascii_lowercase().contains("onload"));
        assert!(clean.contains("width"));
    }

    #[test]
    fn neutralises_javascript_href() {
        let raw = r#"<svg><a href="javascript:alert(1)"><rect/></a></svg>"#;
        let clean = sanitize_svg(raw).unwrap();
        assert!(!clean.to_ascii_lowercase().contains("javascript:"));
    }

    #[test]
    fn strips_external_xlink_href() {
        let raw = r#"<svg><image xlink:href="https://evil.example/steal.png"/></svg>"#;
        let clean = sanitize_svg(raw).unwrap();
        assert!(!clean.contains("evil.example"));
    }

    #[test]
    fn allows_clean_lucide_icon() {
        let raw = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide"><path d="M12 2v20"/></svg>"#;
        let clean = sanitize_svg(raw).unwrap();
        assert!(clean.contains("<path"));
    }

    #[test]
    fn rejects_oversized_input() {
        let big = "<svg>".to_string() + &"a".repeat(600 * 1024) + "</svg>";
        assert!(sanitize_svg(&big).is_err());
    }
}
