//! Group suggestion algorithm.
//!
//! Analyses scanned desktop files, matches them against extension groups,
//! detects common name prefixes, and produces ranked suggestions.

use std::collections::HashMap;

use serde::Serialize;

use super::rules::EXTENSION_GROUPS;
use super::scanner::FileInfo;
use crate::layout::persistence::{AutoGroupRule, GroupRuleType};

/// A suggested group that the user can apply with one click.
#[derive(Debug, Clone, Serialize)]
pub struct SuggestedGroup {
    pub name: String,
    pub icon: String,
    pub rule: AutoGroupRule,
    pub matching_files: Vec<String>,
    pub confidence: f64,
}

/// Analyse a list of files and produce group suggestions.
///
/// Applies three heuristics:
/// 1. **Extension groups**: match files against predefined extension categories.
/// 2. **Name prefix**: detect files sharing a common prefix (>= 3 chars, >= 3 files).
/// 3. **Date groups**: (reserved for future expansion).
///
/// Only groups with >= 3 matching files are suggested. Results are sorted by
/// confidence (descending) and capped at 5 suggestions.
pub fn suggest_groups(files: &[FileInfo]) -> Vec<SuggestedGroup> {
    let total = files.len() as f64;
    if total == 0.0 {
        return Vec::new();
    }

    let mut suggestions: Vec<SuggestedGroup> = Vec::new();

    // 1. Extension-based suggestions
    for (group_name, icon, extensions) in EXTENSION_GROUPS {
        let matching: Vec<String> = files
            .iter()
            .filter(|f| {
                f.extension
                    .as_ref()
                    .is_some_and(|ext| extensions.contains(&ext.as_str()))
            })
            .map(|f| f.path.clone())
            .collect();

        if matching.len() >= 3 {
            let confidence = matching.len() as f64 / total;
            suggestions.push(SuggestedGroup {
                name: group_name.to_string(),
                icon: icon.to_string(),
                rule: AutoGroupRule {
                    rule_type: GroupRuleType::Extension,
                    pattern: None,
                    extensions: Some(extensions.iter().map(|s| s.to_string()).collect()),
                },
                matching_files: matching,
                confidence,
            });
        }
    }

    // 2. Name prefix detection
    //    Find file names sharing a common prefix of >= 3 characters, with >= 3 files.
    let non_dir_files: Vec<&FileInfo> = files.iter().filter(|f| !f.is_directory).collect();
    let prefix_groups = detect_common_prefixes(&non_dir_files);
    for (prefix, matching_paths) in prefix_groups {
        if matching_paths.len() >= 3 {
            let confidence = matching_paths.len() as f64 / total;
            suggestions.push(SuggestedGroup {
                name: format!("{prefix}..."),
                icon: "\u{1F4C1}".to_string(), // folder emoji
                rule: AutoGroupRule {
                    rule_type: GroupRuleType::NamePattern,
                    pattern: Some(format!("^{prefix}")),
                    extensions: None,
                },
                matching_files: matching_paths,
                confidence,
            });
        }
    }

    // 3. AI cluster signal (Theme E2-b): hierarchical clustering over feature
    //    vectors catches groupings that simple prefix / extension heuristics
    //    miss (e.g. mixed extensions sharing a project name).
    let ai_suggestions = super::ai_recommender::clusters_to_suggestions(files);
    for ai in ai_suggestions {
        // Dedupe: skip if an existing signal already covers every file.
        let covered = suggestions.iter().any(|s| {
            s.matching_files.len() >= ai.matching_files.len()
                && ai
                    .matching_files
                    .iter()
                    .all(|f| s.matching_files.contains(f))
        });
        if !covered {
            suggestions.push(ai);
        }
    }

    // Sort by confidence descending
    suggestions.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Return top 5
    suggestions.truncate(5);
    suggestions
}

/// Detect common prefixes among file names.
///
/// Returns a map from prefix to list of full file paths that share it.
/// Only prefixes of >= 3 characters are considered meaningful.
fn detect_common_prefixes(files: &[&FileInfo]) -> HashMap<String, Vec<String>> {
    let mut prefix_map: HashMap<String, Vec<String>> = HashMap::new();

    // Extract the stem (name without extension) for each file.
    let stems: Vec<(&FileInfo, String)> = files
        .iter()
        .map(|f| {
            let stem = std::path::Path::new(&f.name)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            (*f, stem)
        })
        .collect();

    // Compare every pair to find the longest common prefix.
    // We collect all prefixes of length >= 3 and group files by them.
    let mut seen_prefixes: HashMap<String, Vec<String>> = HashMap::new();

    for i in 0..stems.len() {
        for j in (i + 1)..stems.len() {
            let prefix = longest_common_prefix(&stems[i].1, &stems[j].1);
            if prefix.len() >= 3 {
                // Normalize prefix to lowercase for grouping
                let key = prefix.to_lowercase();
                let entry = seen_prefixes.entry(key).or_default();
                let path_i = stems[i].0.path.clone();
                let path_j = stems[j].0.path.clone();
                if !entry.contains(&path_i) {
                    entry.push(path_i);
                }
                if !entry.contains(&path_j) {
                    entry.push(path_j);
                }
            }
        }
    }

    // Only keep groups with >= 3 files, and pick the longest prefix for overlapping groups
    for (prefix, paths) in seen_prefixes {
        if paths.len() >= 3 {
            prefix_map.insert(prefix, paths);
        }
    }

    prefix_map
}

/// Compute the longest common prefix of two strings (case-insensitive).
fn longest_common_prefix(a: &str, b: &str) -> String {
    let mut result = String::new();
    for (ca, cb) in a.chars().zip(b.chars()) {
        if ca.to_lowercase().eq(cb.to_lowercase()) {
            result.push(ca);
        } else {
            break;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_file(name: &str, ext: Option<&str>) -> FileInfo {
        FileInfo {
            name: name.to_string(),
            path: format!("C:\\Desktop\\{name}"),
            size: 1024,
            file_type: ext.unwrap_or("unknown").to_string(),
            modified_at: "2026-01-01T00:00:00Z".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            is_directory: false,
            extension: ext.map(|e| e.to_string()),
        }
    }

    #[test]
    fn suggest_groups_empty_input() {
        let result = suggest_groups(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn suggest_groups_needs_minimum_three_files() {
        let files = vec![
            make_file("a.pdf", Some("pdf")),
            make_file("b.pdf", Some("pdf")),
        ];
        let result = suggest_groups(&files);
        // Only 2 PDF files, need >= 3 for a suggestion
        assert!(result.is_empty());
    }

    #[test]
    fn suggest_groups_detects_extension_group() {
        let files = vec![
            make_file("report.pdf", Some("pdf")),
            make_file("notes.txt", Some("txt")),
            make_file("guide.doc", Some("doc")),
            make_file("data.csv", Some("csv")),
        ];
        let result = suggest_groups(&files);
        assert!(!result.is_empty());
        let doc_group = result.iter().find(|g| g.name == "Documents");
        assert!(doc_group.is_some());
        assert_eq!(doc_group.unwrap().matching_files.len(), 4);
    }

    #[test]
    fn suggest_groups_sorted_by_confidence_descending() {
        let mut files = Vec::new();
        // 5 images
        for i in 0..5 {
            files.push(make_file(&format!("img{i}.png"), Some("png")));
        }
        // 3 documents
        for i in 0..3 {
            files.push(make_file(&format!("doc{i}.pdf"), Some("pdf")));
        }
        let result = suggest_groups(&files);
        assert!(result.len() >= 2);
        // Higher confidence (more matching) should come first
        assert!(result[0].confidence >= result[1].confidence);
    }

    #[test]
    fn suggest_groups_capped_at_five() {
        // Create many different extension groups with 3+ files each
        let mut files = Vec::new();
        for (group_exts, prefix) in [
            ("png", "img"),
            ("pdf", "doc"),
            ("mp4", "vid"),
            ("mp3", "aud"),
            ("rs", "code"),
            ("zip", "arch"),
        ] {
            for i in 0..4 {
                files.push(make_file(
                    &format!("{prefix}{i}.{group_exts}"),
                    Some(group_exts),
                ));
            }
        }
        let result = suggest_groups(&files);
        assert!(result.len() <= 5);
    }

    #[test]
    fn longest_common_prefix_basic() {
        assert_eq!(longest_common_prefix("hello", "help"), "hel");
        assert_eq!(longest_common_prefix("abc", "xyz"), "");
        assert_eq!(longest_common_prefix("Test", "test"), "Test");
    }

    #[test]
    fn longest_common_prefix_empty_input() {
        assert_eq!(longest_common_prefix("", "hello"), "");
        assert_eq!(longest_common_prefix("hello", ""), "");
        assert_eq!(longest_common_prefix("", ""), "");
    }

    #[test]
    fn detect_name_prefix_group() {
        let files = vec![
            make_file("ProjectA_report.pdf", Some("pdf")),
            make_file("ProjectA_notes.txt", Some("txt")),
            make_file("ProjectA_data.csv", Some("csv")),
            make_file("unrelated.doc", Some("doc")),
        ];
        let result = suggest_groups(&files);
        let prefix_group = result
            .iter()
            .find(|g| g.rule.rule_type == GroupRuleType::NamePattern);
        assert!(prefix_group.is_some());
    }
}
