//! Feature vectors + similarity functions used by the AI recommender.
//!
//! Features are intentionally language-agnostic: the tokenizer splits on
//! camelCase boundaries, `_`/`-`/whitespace, and treats every CJK character
//! as its own token. Jaccard similarity is used because it handles the
//! sparse high-dimensional nature of filename tokens better than cosine.

use super::scanner::FileInfo;

/// Lightweight numeric description of a single file.
#[derive(Debug, Clone)]
pub struct FeatureVector {
    pub path: String,
    pub name: String,
    pub extension: String,
    pub name_tokens: Vec<String>,
    pub created_day: i64,
    pub size_bucket: u8,
}

/// Tokenise a filename stem into a set of lowercase tokens.
///
/// - `camelCase` boundaries split the word.
/// - `_`, `-`, `.`, and whitespace split.
/// - Every CJK character contributes a single-character token.
pub fn tokenize_name(name: &str) -> Vec<String> {
    let stem = std::path::Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_string());

    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut prev_lower = false;

    for ch in stem.chars() {
        let is_sep = ch == '_' || ch == '-' || ch == '.' || ch.is_whitespace();
        let is_cjk = is_cjk_char(ch);
        let is_upper = ch.is_uppercase();

        if is_sep {
            push_if_nonempty(&mut tokens, &mut current);
            prev_lower = false;
            continue;
        }

        if is_cjk {
            push_if_nonempty(&mut tokens, &mut current);
            tokens.push(ch.to_lowercase().collect());
            prev_lower = false;
            continue;
        }

        if is_upper && prev_lower {
            push_if_nonempty(&mut tokens, &mut current);
        }
        for c in ch.to_lowercase() {
            current.push(c);
        }
        prev_lower = ch.is_lowercase() || ch.is_ascii_digit();
    }
    push_if_nonempty(&mut tokens, &mut current);

    tokens.retain(|t| !t.is_empty());
    tokens.sort();
    tokens.dedup();
    tokens
}

fn push_if_nonempty(tokens: &mut Vec<String>, buf: &mut String) {
    if !buf.is_empty() {
        tokens.push(std::mem::take(buf));
    }
}

fn is_cjk_char(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF    // CJK Unified Ideographs
        | 0x3040..=0x30FF  // Hiragana + Katakana
        | 0xAC00..=0xD7AF  // Hangul
    )
}

fn size_bucket(bytes: u64) -> u8 {
    match bytes {
        0..=10_240 => 0,               // ≤10 KB
        10_241..=102_400 => 1,         // ≤100 KB
        102_401..=1_048_576 => 2,      // ≤1 MB
        1_048_577..=10_485_760 => 3,   // ≤10 MB
        10_485_761..=104_857_600 => 4, // ≤100 MB
        _ => 5,                        // >100 MB
    }
}

fn created_day(iso: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(iso)
        .map(|t| t.timestamp() / 86_400)
        .unwrap_or(0)
}

/// Build a feature vector for a single file.
pub fn featurize(file: &FileInfo) -> FeatureVector {
    FeatureVector {
        path: file.path.clone(),
        name: file.name.clone(),
        extension: file
            .extension
            .as_ref()
            .map(|s| s.to_lowercase())
            .unwrap_or_default(),
        name_tokens: tokenize_name(&file.name),
        created_day: created_day(&file.created_at),
        size_bucket: size_bucket(file.size),
    }
}

/// Compute Jaccard similarity (|A ∩ B| / |A ∪ B|) for two sorted token lists.
pub fn jaccard(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let set_a: std::collections::HashSet<&String> = a.iter().collect();
    let set_b: std::collections::HashSet<&String> = b.iter().collect();
    let inter = set_a.intersection(&set_b).count() as f64;
    let union = set_a.union(&set_b).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Composite similarity: weighted mix of token Jaccard + extension + date decay.
pub fn similarity(a: &FeatureVector, b: &FeatureVector) -> f64 {
    let name_j = jaccard(&a.name_tokens, &b.name_tokens);
    let ext = if !a.extension.is_empty() && a.extension == b.extension {
        1.0
    } else {
        0.0
    };
    let day_diff = (a.created_day - b.created_day).abs() as f64;
    let time = (-day_diff / 7.0).exp(); // decays over a week
    0.55 * name_j + 0.3 * ext + 0.15 * time
}

/// Mean pairwise similarity within a cluster. Used for confidence scoring.
pub fn mean_pairwise_similarity(cluster: &[FeatureVector]) -> f64 {
    if cluster.len() < 2 {
        return 0.0;
    }
    let mut total = 0.0;
    let mut n = 0;
    for i in 0..cluster.len() {
        for j in (i + 1)..cluster.len() {
            total += similarity(&cluster[i], &cluster[j]);
            n += 1;
        }
    }
    if n == 0 {
        0.0
    } else {
        total / n as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_camel_and_separators() {
        let t = tokenize_name("ProjectAlphaReport_final_v2.pdf");
        assert!(t.contains(&"project".to_string()));
        assert!(t.contains(&"alpha".to_string()));
        assert!(t.contains(&"report".to_string()));
        assert!(t.contains(&"final".to_string()));
        assert!(t.contains(&"v2".to_string()));
    }

    #[test]
    fn tokenize_cjk_per_char() {
        let t = tokenize_name("项目报告_v1.docx");
        assert!(t.contains(&"项".to_string()));
        assert!(t.contains(&"目".to_string()));
        assert!(t.contains(&"报".to_string()));
        assert!(t.contains(&"告".to_string()));
        assert!(t.contains(&"v1".to_string()));
    }

    #[test]
    fn tokenize_mixed_ascii_cjk() {
        let t = tokenize_name("ProjectAlpha_项目_v2.pdf");
        assert!(t.contains(&"项".to_string()));
        assert!(t.contains(&"alpha".to_string()));
    }

    #[test]
    fn jaccard_identical_tokens() {
        let a = vec!["foo".to_string(), "bar".to_string()];
        let b = vec!["bar".to_string(), "foo".to_string()];
        assert!((jaccard(&a, &b) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn jaccard_no_overlap_zero() {
        let a = vec!["foo".to_string()];
        let b = vec!["bar".to_string()];
        assert_eq!(jaccard(&a, &b), 0.0);
    }

    #[test]
    fn similarity_same_extension_and_tokens_is_high() {
        let a = FeatureVector {
            path: "/x".into(),
            name: "Alpha_Report.pdf".into(),
            extension: "pdf".into(),
            name_tokens: vec!["alpha".into(), "report".into()],
            created_day: 100,
            size_bucket: 2,
        };
        let b = FeatureVector {
            path: "/y".into(),
            name: "Alpha_Summary.pdf".into(),
            extension: "pdf".into(),
            name_tokens: vec!["alpha".into(), "summary".into()],
            created_day: 100,
            size_bucket: 2,
        };
        let s = similarity(&a, &b);
        assert!(s > 0.5, "expected >0.5 got {s}");
    }
}
