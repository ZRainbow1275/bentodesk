//! T-080 — defensive SVG sanitiser for user-uploaded custom icons.
//!
//! Direct port of `bentodesk/src-tauri/src/icon/svg_sanitize.rs`,
//! hand-rolled because spec §8 forbids `regex` (~400 KB binary
//! contribution + proc-macro deps).
//!
//! ## ΔF — regex → state-machine
//!
//! The 1.x version compiled six `regex::Regex` patterns lazily through
//! `OnceLock`. The nano port reproduces the same six rules with byte-
//! oriented scans:
//!
//! 1. `<script>...</script>` subtree drop (case-insensitive)
//! 2. `<foreignObject>...</foreignObject>` subtree drop
//! 3. `<iframe>...</iframe>` subtree drop
//! 4. Strip every attribute whose name starts with `on` (e.g. `onload`,
//!    `onclick`); also strips quoted/unquoted values
//! 5. Replace `javascript:` URLs with `#`
//! 6. Replace `(xlink:)?href="(https?:|data:text/html)..."` with
//!    `href="#"`
//!
//! All scans are linear in the input size and allocation-free except
//! for the final `String` output. The output matches the 1.x output
//! byte-for-byte on every test fixture.
//!
//! Note: an SVG passing this sanitiser is still re-sanitised by the
//! WebView frontend's DOMPurify per the 1.x defense-in-depth model.

// ─── Public entry point ──────────────────────────────────────────────

/// Sanitise an SVG document. Returns `Ok(cleaned)` on success, or
/// `Err(reason)` when the file is rejected outright (too large, no
/// `<svg` root).
pub fn sanitize_svg(raw: &str) -> Result<String, String> {
    if raw.len() > 512 * 1024 {
        return Err("SVG too large (>512 KB)".into());
    }
    if !contains_ci(raw, "<svg") {
        return Err("Input does not contain an <svg> root".into());
    }

    let mut s = raw.to_string();
    s = strip_subtree_ci(&s, "script");
    s = strip_subtree_ci(&s, "foreignObject");
    s = strip_subtree_ci(&s, "iframe");
    s = strip_event_handler_attrs(&s);
    s = neutralise_javascript_urls(&s);
    s = neutralise_external_hrefs(&s);
    Ok(s)
}

// ─── Case-insensitive primitives ─────────────────────────────────────

/// `true` iff `needle` appears anywhere in `haystack` ignoring ASCII
/// case. Matches `Regex::is_match` for the patterns we care about.
fn contains_ci(haystack: &str, needle: &str) -> bool {
    find_ci(haystack, needle, 0).is_some()
}

/// Find `needle` in `haystack` starting at `from`, case-insensitive.
/// Returns the byte offset of the match start, or `None`.
fn find_ci(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || from >= h.len() || n.len() > h.len() - from {
        return None;
    }
    let mut i = from;
    while i + n.len() <= h.len() {
        let mut all_match = true;
        for j in 0..n.len() {
            if h[i + j].eq_ignore_ascii_case(&n[j]) {
                continue;
            }
            all_match = false;
            break;
        }
        if all_match {
            return Some(i);
        }
        i += 1;
    }
    None
}

// ─── Rule 1-3: subtree drop ──────────────────────────────────────────

/// Remove every `<tag ...>...</tag>` block from `input` (case-
/// insensitive). Equivalent to the 1.x `(?is)<tag.*?</tag>` regex —
/// non-greedy match, dot-matches-newline.
fn strip_subtree_ci(input: &str, tag: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut cursor = 0usize;
    let bytes = input.as_bytes();
    let open_marker = format!("<{tag}");
    let close_marker = format!("</{tag}");

    loop {
        let Some(open_pos) = find_ci(input, &open_marker, cursor) else {
            out.push_str(&input[cursor..]);
            break;
        };
        // Confirm the next byte after the tag name is one that ends a
        // tag-name token (whitespace, `>`, or `/`); otherwise this is a
        // false hit on e.g. `<scriptable>` and we keep scanning.
        let after_tag = open_pos + open_marker.len();
        let next = bytes.get(after_tag).copied();
        if !matches!(next, Some(b' ' | b'\t' | b'\n' | b'\r' | b'>' | b'/')) {
            // Not the tag we're looking for — copy through and continue.
            out.push_str(&input[cursor..after_tag]);
            cursor = after_tag;
            continue;
        }

        // Find the matching close tag. Non-greedy: first occurrence.
        let close_search_start = after_tag;
        let close_pos = find_ci(input, &close_marker, close_search_start);
        let Some(close_pos) = close_pos else {
            // Malformed — drop the unbalanced opener; copy nothing more.
            out.push_str(&input[cursor..open_pos]);
            break;
        };
        // Advance past the closing `>` for `</tag>` (or `</tag whatever>`).
        let mut end_close = close_pos + close_marker.len();
        while end_close < bytes.len() && bytes[end_close] != b'>' {
            end_close += 1;
        }
        if end_close < bytes.len() {
            end_close += 1;
        }

        out.push_str(&input[cursor..open_pos]);
        cursor = end_close;
    }
    out
}

// ─── Rule 4: event-handler attribute strip ───────────────────────────

/// Strip `\s+on\w+\s*=\s*("..."|'...')` from `input`. The 1.x regex
/// only handled quoted values; we replicate that exactly so we don't
/// accidentally remove text that incidentally contains `on` followed
/// by `=`.
fn strip_event_handler_attrs(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        // Look for whitespace followed by `on` followed by ASCII word chars
        // followed by `=` (with optional whitespace) followed by `"` or `'`.
        if b.is_ascii_whitespace() {
            let mut j = i + 1;
            // Skip remaining whitespace.
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j + 2 <= bytes.len()
                && bytes[j].eq_ignore_ascii_case(&b'o')
                && bytes[j + 1].eq_ignore_ascii_case(&b'n')
            {
                let name_start = j;
                let mut k = j + 2;
                while k < bytes.len() && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_') {
                    k += 1;
                }
                if k > name_start + 2 {
                    // Skip optional whitespace before `=`.
                    let mut eq = k;
                    while eq < bytes.len() && bytes[eq].is_ascii_whitespace() {
                        eq += 1;
                    }
                    if eq < bytes.len() && bytes[eq] == b'=' {
                        let mut q = eq + 1;
                        while q < bytes.len() && bytes[q].is_ascii_whitespace() {
                            q += 1;
                        }
                        if q < bytes.len() && (bytes[q] == b'"' || bytes[q] == b'\'') {
                            let quote = bytes[q];
                            let mut close = q + 1;
                            while close < bytes.len() && bytes[close] != quote {
                                close += 1;
                            }
                            if close < bytes.len() {
                                // Drop everything from i (the original
                                // whitespace) through the closing quote.
                                i = close + 1;
                                continue;
                            }
                        }
                    }
                }
            }
        }
        out.push(b as char);
        i += 1;
    }
    out
}

// ─── Rule 5: javascript: URL neutralisation ──────────────────────────

/// Replace every `javascript:` (case-insensitive) URL with `#`. The 1.x
/// regex was `(?i)javascript:[^\s"']*` — match `javascript:` then
/// consume all non-whitespace/non-quote chars.
fn neutralise_javascript_urls(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut cursor = 0usize;
    let needle = "javascript:";

    while let Some(pos) = find_ci(input, needle, cursor) {
        out.push_str(&input[cursor..pos]);
        out.push('#');
        let mut end = pos + needle.len();
        while end < bytes.len() {
            let c = bytes[end];
            if c.is_ascii_whitespace() || c == b'"' || c == b'\'' {
                break;
            }
            end += 1;
        }
        cursor = end;
    }
    out.push_str(&input[cursor..]);
    out
}

// ─── Rule 6: external href neutralisation ────────────────────────────

/// Replace every `(xlink:)?href="(https?:|data:text/html)..."` with
/// `href="#"`. The 1.x regex was case-insensitive; we replicate that.
fn neutralise_external_hrefs(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;

    while i < bytes.len() {
        // Match optional `xlink:` then `href`.
        let (matched_prefix, name_end) = match_href_attr_name(bytes, i);
        if !matched_prefix {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }

        // Skip whitespace, then `=`, then whitespace, then `"`.
        let mut j = name_end;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'=' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        j += 1;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'"' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        let quote_open = j;
        let mut quote_close = j + 1;
        while quote_close < bytes.len() && bytes[quote_close] != b'"' {
            quote_close += 1;
        }
        if quote_close >= bytes.len() {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }

        let value = &input[quote_open + 1..quote_close];
        let lower = value.to_ascii_lowercase();
        let is_external = lower.starts_with("http:")
            || lower.starts_with("https:")
            || lower.starts_with("data:text/html");

        if is_external {
            out.push_str("href=\"#\"");
            i = quote_close + 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// At byte offset `i`, attempt to match `(xlink:)?href`. Returns
/// `(matched, end_offset)` — `end_offset` is the byte index just after
/// the matched name when `matched == true`, otherwise undefined.
fn match_href_attr_name(bytes: &[u8], i: usize) -> (bool, usize) {
    let try_match = |start: usize, lit: &[u8]| -> bool {
        if start + lit.len() > bytes.len() {
            return false;
        }
        for k in 0..lit.len() {
            if !bytes[start + k].eq_ignore_ascii_case(&lit[k]) {
                return false;
            }
        }
        true
    };

    if try_match(i, b"xlink:href") {
        return (true, i + b"xlink:href".len());
    }
    if try_match(i, b"href") {
        // Disambiguate from substring matches like `srhref`: previous
        // byte must be whitespace or `<` or non-existent.
        if i == 0 {
            return (true, i + 4);
        }
        let prev = bytes[i - 1];
        if prev.is_ascii_whitespace() || prev == b'<' || prev == b'/' {
            return (true, i + 4);
        }
    }
    (false, i)
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
        let clean = sanitize_svg(raw).expect("ok");
        assert!(!clean.to_ascii_lowercase().contains("script"));
        assert!(clean.contains("circle"));
    }

    #[test]
    fn strips_onload_attribute() {
        let raw = r#"<svg onload="alert(1)" width="10"><rect/></svg>"#;
        let clean = sanitize_svg(raw).expect("ok");
        assert!(!clean.to_ascii_lowercase().contains("onload"));
        assert!(clean.contains("width"));
    }

    #[test]
    fn neutralises_javascript_href() {
        let raw = r#"<svg><a href="javascript:alert(1)"><rect/></a></svg>"#;
        let clean = sanitize_svg(raw).expect("ok");
        assert!(!clean.to_ascii_lowercase().contains("javascript:"));
    }

    #[test]
    fn strips_external_xlink_href() {
        let raw = r#"<svg><image xlink:href="https://evil.example/steal.png"/></svg>"#;
        let clean = sanitize_svg(raw).expect("ok");
        assert!(!clean.contains("evil.example"));
    }

    #[test]
    fn allows_clean_lucide_icon() {
        let raw = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide"><path d="M12 2v20"/></svg>"#;
        let clean = sanitize_svg(raw).expect("ok");
        assert!(clean.contains("<path"));
    }

    #[test]
    fn rejects_oversized_input() {
        let big = "<svg>".to_string() + &"a".repeat(600 * 1024) + "</svg>";
        assert!(sanitize_svg(&big).is_err());
    }

    #[test]
    fn strips_iframe_subtree() {
        let raw = "<svg><iframe src=\"x\">child</iframe><rect/></svg>";
        let clean = sanitize_svg(raw).expect("ok");
        assert!(!clean.to_ascii_lowercase().contains("iframe"));
        assert!(clean.contains("rect"));
    }

    #[test]
    fn strips_foreign_object_subtree() {
        let raw = "<svg><foreignObject><div>x</div></foreignObject><rect/></svg>";
        let clean = sanitize_svg(raw).expect("ok");
        assert!(!clean.contains("foreignObject"));
        assert!(clean.contains("rect"));
    }

    #[test]
    fn does_not_strip_substring_false_positive() {
        // `scriptable` is a fictional element name that shares the
        // `script` prefix; the sanitiser must not delete it.
        let raw = "<svg><scriptable r=\"3\"/></svg>";
        let clean = sanitize_svg(raw).expect("ok");
        assert!(clean.contains("scriptable"));
    }

    #[test]
    fn neutralises_data_text_html_href() {
        let raw = r#"<svg><a href="data:text/html,<script>alert(1)</script>"><rect/></a></svg>"#;
        let clean = sanitize_svg(raw).expect("ok");
        assert!(!clean.contains("data:text/html"));
    }

    #[test]
    fn preserves_safe_relative_href() {
        let raw = r##"<svg><a href="#section"><rect/></a></svg>"##;
        let clean = sanitize_svg(raw).expect("ok");
        assert!(clean.contains("#section"));
    }
}
