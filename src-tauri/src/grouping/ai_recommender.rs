//! Single-pass hierarchical clustering over file FeatureVectors.
//!
//! The algorithm avoids K-means because the number of natural clusters on a
//! desktop is unknown. Instead we:
//!   1. Treat each file as a singleton cluster.
//!   2. Compute the similarity between every pair of clusters.
//!   3. Merge the highest-scoring pair when similarity ≥ THRESHOLD and the
//!      combined size stays ≤ MAX_CLUSTER_SIZE.
//!   4. Repeat until no merge meets the criteria.
//!
//! Complexity is O(n²) in the number of files. At 200 files this is ~40 k
//! comparisons, well under 10 ms in release mode.

use serde::Serialize;

use super::feature::{featurize, mean_pairwise_similarity, similarity, FeatureVector};
use super::scanner::FileInfo;
use crate::layout::persistence::{AutoGroupRule, GroupRuleType};

use super::suggestions::SuggestedGroup;

/// Minimum pairwise similarity to merge two clusters.
pub const MERGE_THRESHOLD: f64 = 0.55;
/// Upper bound on cluster size — keeps the recommender from producing a
/// single giant "all files" bucket.
pub const MAX_CLUSTER_SIZE: usize = 15;
/// Minimum files required for a cluster to surface as a suggestion.
pub const MIN_SUGGESTION_SIZE: usize = 3;

/// Serialisable view of a single cluster — used for debug tests and
/// potentially future export.
#[derive(Debug, Clone, Serialize)]
pub struct ClusterSummary {
    pub size: usize,
    pub confidence: f64,
    pub representative_extension: String,
    pub paths: Vec<String>,
}

/// Run the single-pass hierarchical clustering.
pub fn cluster(files: &[FileInfo]) -> Vec<Vec<FeatureVector>> {
    if files.is_empty() {
        return Vec::new();
    }

    let mut clusters: Vec<Vec<FeatureVector>> = files.iter().map(|f| vec![featurize(f)]).collect();

    loop {
        let mut best: Option<(usize, usize, f64)> = None;
        for i in 0..clusters.len() {
            for j in (i + 1)..clusters.len() {
                if clusters[i].len() + clusters[j].len() > MAX_CLUSTER_SIZE {
                    continue;
                }
                let sim = inter_cluster_similarity(&clusters[i], &clusters[j]);
                if sim < MERGE_THRESHOLD {
                    continue;
                }
                match best {
                    None => best = Some((i, j, sim)),
                    Some((_, _, prev)) if sim > prev => best = Some((i, j, sim)),
                    _ => {}
                }
            }
        }
        match best {
            Some((i, j, _)) => {
                let moved = clusters.remove(j);
                clusters[i].extend(moved);
            }
            None => break,
        }
    }

    clusters
}

/// Average-linkage similarity between two clusters.
fn inter_cluster_similarity(a: &[FeatureVector], b: &[FeatureVector]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut total = 0.0;
    let mut n = 0;
    for x in a {
        for y in b {
            total += similarity(x, y);
            n += 1;
        }
    }
    if n == 0 {
        0.0
    } else {
        total / n as f64
    }
}

/// Confidence score for a cluster. Higher = more cohesive suggestion.
pub fn confidence(cluster: &[FeatureVector], total_files: usize) -> f64 {
    if cluster.len() < 2 {
        return 0.0;
    }
    let mean = mean_pairwise_similarity(cluster);
    let ratio = if total_files == 0 {
        0.0
    } else {
        cluster.len() as f64 / total_files as f64
    };
    let ext_share = extension_share(cluster);
    0.55 * mean + 0.25 * ext_share + 0.2 * (1.0 - (-ratio * 4.0).exp())
}

fn extension_share(cluster: &[FeatureVector]) -> f64 {
    if cluster.is_empty() {
        return 0.0;
    }
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for f in cluster {
        if !f.extension.is_empty() {
            *counts.entry(f.extension.as_str()).or_insert(0) += 1;
        }
    }
    counts
        .values()
        .max()
        .map(|n| *n as f64 / cluster.len() as f64)
        .unwrap_or(0.0)
}

/// Derive a display name for a cluster. Picks the most common leading token,
/// falling back to the extension.
fn derive_name(cluster: &[FeatureVector]) -> String {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for f in cluster {
        if let Some(first) = f.name_tokens.first() {
            *counts.entry(first.clone()).or_insert(0) += 1;
        }
    }
    let best = counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(name, _)| name)
        .unwrap_or_default();
    if best.is_empty() {
        cluster
            .first()
            .map(|f| f.extension.clone())
            .unwrap_or_else(|| "group".to_string())
    } else {
        capitalize(&best)
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Produce `SuggestedGroup`s from AI clusters, for integration with the
/// existing `suggestions::suggest_groups` pipeline.
pub fn clusters_to_suggestions(files: &[FileInfo]) -> Vec<SuggestedGroup> {
    let total = files.len();
    let clusters = cluster(files);
    let mut out: Vec<SuggestedGroup> = Vec::new();

    for c in clusters {
        if c.len() < MIN_SUGGESTION_SIZE {
            continue;
        }
        let conf = confidence(&c, total);
        if conf < 0.35 {
            continue;
        }

        let name = derive_name(&c);
        let paths: Vec<String> = c.iter().map(|f| f.path.clone()).collect();

        // Prefer an extension-typed rule when a dominant extension is present;
        // otherwise emit a name-pattern rule derived from the common prefix.
        let dominant_ext: Option<String> = {
            let mut counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for f in &c {
                if !f.extension.is_empty() {
                    *counts.entry(f.extension.clone()).or_insert(0) += 1;
                }
            }
            counts
                .into_iter()
                .max_by_key(|(_, n)| *n)
                .and_then(|(k, n)| {
                    if n as f64 / c.len() as f64 >= 0.6 {
                        Some(k)
                    } else {
                        None
                    }
                })
        };

        let rule = if let Some(ext) = dominant_ext {
            AutoGroupRule {
                rule_type: GroupRuleType::Extension,
                pattern: None,
                extensions: Some(vec![ext]),
            }
        } else {
            AutoGroupRule {
                rule_type: GroupRuleType::NamePattern,
                pattern: Some(format!("^{}", regex_escape(&name.to_lowercase()))),
                extensions: None,
            }
        };

        out.push(SuggestedGroup {
            name: format!("{name} (AI)"),
            icon: "sparkles".to_string(),
            rule,
            matching_files: paths,
            confidence: conf,
        });
    }

    out
}

fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(
            c,
            '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_file(name: &str, ext: &str, day: i64, size: u64) -> FileInfo {
        FileInfo {
            name: name.to_string(),
            path: format!("C:/Desktop/{name}"),
            size,
            file_type: ext.to_string(),
            modified_at: "2026-04-18T00:00:00Z".to_string(),
            created_at: chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    + chrono::Duration::days(day),
                chrono::Utc,
            )
            .to_rfc3339(),
            is_directory: false,
            extension: Some(ext.to_string()),
        }
    }

    #[test]
    fn empty_input_empty_output() {
        assert!(cluster(&[]).is_empty());
        assert!(clusters_to_suggestions(&[]).is_empty());
    }

    #[test]
    fn similar_names_cluster_together() {
        let files = vec![
            mk_file("Alpha_Report_v1.pdf", "pdf", 0, 1000),
            mk_file("Alpha_Report_v2.pdf", "pdf", 1, 1000),
            mk_file("Alpha_Summary.pdf", "pdf", 2, 1000),
            mk_file("unrelated.mp4", "mp4", 60, 1_000_000),
        ];
        let suggestions = clusters_to_suggestions(&files);
        assert!(
            suggestions.iter().any(|s| s.matching_files.len() >= 3),
            "expected a cluster of 3 alpha-prefixed PDFs, got {suggestions:?}"
        );
    }

    #[test]
    fn respects_max_cluster_size() {
        let files: Vec<FileInfo> = (0..30)
            .map(|i| mk_file(&format!("Report_{i}.pdf"), "pdf", i as i64, 1000))
            .collect();
        let clusters = cluster(&files);
        for c in &clusters {
            assert!(
                c.len() <= MAX_CLUSTER_SIZE,
                "cluster size {} exceeds cap",
                c.len()
            );
        }
    }

    #[test]
    fn confidence_in_zero_to_one_range() {
        let files: Vec<FileInfo> = (0..5)
            .map(|i| mk_file(&format!("Alpha_{i}.txt"), "txt", 0, 1000))
            .collect();
        let clusters = cluster(&files);
        for c in &clusters {
            let conf = confidence(c, files.len());
            assert!((0.0..=1.0).contains(&conf), "conf out of range: {conf}");
        }
    }
}
