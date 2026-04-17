//! Grouping rule engine: predefined extension-based and date-based rules.

use std::time::SystemTime;

/// Predefined extension groups.
///
/// Each tuple is `(group_name, emoji_icon, list_of_extensions)`.
pub const EXTENSION_GROUPS: &[(&str, &str, &[&str])] = &[
    (
        "Documents",
        "\u{1F4C4}",
        &[
            "doc", "docx", "pdf", "txt", "md", "rtf", "odt", "xlsx", "xls", "pptx", "ppt", "csv",
        ],
    ),
    (
        "Images",
        "\u{1F5BC}\u{FE0F}",
        &[
            "png", "jpg", "jpeg", "gif", "svg", "webp", "bmp", "ico", "tiff",
        ],
    ),
    (
        "Videos",
        "\u{1F3AC}",
        &["mp4", "avi", "mkv", "mov", "wmv", "flv", "webm"],
    ),
    (
        "Audio",
        "\u{1F3B5}",
        &["mp3", "wav", "flac", "aac", "ogg", "wma", "m4a"],
    ),
    (
        "Code",
        "\u{1F4BB}",
        &[
            "rs", "js", "ts", "tsx", "jsx", "py", "go", "java", "cpp", "c", "h", "cs", "rb", "php",
            "html", "css",
        ],
    ),
    (
        "Archives",
        "\u{1F4E6}",
        &["zip", "rar", "7z", "tar", "gz", "bz2"],
    ),
    (
        "Executables",
        "\u{2699}\u{FE0F}",
        &["exe", "msi", "bat", "cmd", "ps1", "sh"],
    ),
    ("Shortcuts", "\u{1F517}", &["lnk", "url"]),
];

/// Return a human-readable date group name based on the file's modification time.
pub fn date_group_name(modified: SystemTime) -> &'static str {
    let now = SystemTime::now();
    let duration = now.duration_since(modified).unwrap_or_default();
    let days = duration.as_secs() / 86_400;

    match days {
        0 => "Today",
        1..=6 => "This Week",
        7..=29 => "This Month",
        _ => "Older",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn extension_groups_not_empty() {
        assert!(!EXTENSION_GROUPS.is_empty());
    }

    #[test]
    fn extension_groups_have_valid_structure() {
        for (name, icon, exts) in EXTENSION_GROUPS {
            assert!(!name.is_empty(), "Group name should not be empty");
            assert!(!icon.is_empty(), "Group icon should not be empty");
            assert!(!exts.is_empty(), "Group extensions should not be empty");
            for ext in *exts {
                assert!(
                    !ext.contains('.'),
                    "Extensions should not contain dots: {ext}"
                );
                assert_eq!(
                    *ext,
                    ext.to_lowercase(),
                    "Extensions should be lowercase: {ext}"
                );
            }
        }
    }

    #[test]
    fn date_group_name_today() {
        let now = SystemTime::now();
        assert_eq!(date_group_name(now), "Today");
    }

    #[test]
    fn date_group_name_this_week() {
        let three_days_ago = SystemTime::now() - Duration::from_secs(3 * 86_400);
        assert_eq!(date_group_name(three_days_ago), "This Week");
    }

    #[test]
    fn date_group_name_this_month() {
        let two_weeks_ago = SystemTime::now() - Duration::from_secs(14 * 86_400);
        assert_eq!(date_group_name(two_weeks_ago), "This Month");
    }

    #[test]
    fn date_group_name_older() {
        let sixty_days_ago = SystemTime::now() - Duration::from_secs(60 * 86_400);
        assert_eq!(date_group_name(sixty_days_ago), "Older");
    }

    #[test]
    fn no_duplicate_extensions_across_groups() {
        let mut all_exts: Vec<&str> = Vec::new();
        for (_, _, exts) in EXTENSION_GROUPS {
            for ext in *exts {
                assert!(
                    !all_exts.contains(ext),
                    "Duplicate extension across groups: {ext}"
                );
                all_exts.push(ext);
            }
        }
    }
}
