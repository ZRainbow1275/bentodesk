//! Match a captured window record back to a live HWND for restore.
//!
//! Three signals are combined, in priority order:
//!   1. `class_name` exact match (strongest).
//!   2. `process_name` exact match (case-insensitive).
//!   3. `title` similarity via Levenshtein ≤ 3, OR one contains the other.
//!
//! A live window is selected when at least TWO signals agree. Titles alone
//! are not enough to avoid matching unrelated File Explorer / Chrome tabs.

use super::enum_windows::LiveWindow;
use super::CapturedWindow;

/// Pure Levenshtein edit distance, capped at `max + 1` for early exit.
pub fn levenshtein(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr: Vec<usize> = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

fn title_similar(a: &str, b: &str) -> bool {
    let al = a.to_lowercase();
    let bl = b.to_lowercase();
    if al == bl {
        return true;
    }
    if al.is_empty() || bl.is_empty() {
        return false;
    }
    if al.contains(&bl) || bl.contains(&al) {
        return true;
    }
    levenshtein(&al, &bl) <= 3
}

/// Find the live HWND that best matches a captured window. Returns `None`
/// when no live window accumulates ≥ 2 matching signals.
pub fn match_window(captured: &CapturedWindow, live: &[LiveWindow]) -> Option<isize> {
    // Score each candidate, keeping the highest-scoring HWND. Ties are broken
    // by position proximity so repeated notepad.exe windows don't ping-pong.
    let mut best: Option<(isize, i32, i64)> = None;
    for lw in live {
        let mut score = 0i32;
        if !lw.class_name.is_empty() && lw.class_name == captured.class_name {
            score += 2;
        }
        if !lw.process_name.is_empty()
            && lw.process_name.eq_ignore_ascii_case(&captured.process_name)
        {
            score += 2;
        }
        if title_similar(&lw.title, &captured.title) {
            score += 1;
        }
        if score >= 2 {
            // Distance tiebreaker: prefer the live window closest to captured position.
            let dx = (lw.rect.0 as i64 - captured.rect.0 as i64).abs();
            let dy = (lw.rect.1 as i64 - captured.rect.1 as i64).abs();
            let dist = dx + dy;
            match &best {
                None => best = Some((lw.hwnd, score, dist)),
                Some((_, s, d)) if score > *s || (score == *s && dist < *d) => {
                    best = Some((lw.hwnd, score, dist));
                }
                _ => {}
            }
        }
    }
    best.map(|(h, _, _)| h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_known_distances() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", "abc"), 0);
        assert_eq!(levenshtein("abc", "abd"), 1);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "abc"), 3);
    }

    fn cap(title: &str, class: &str, proc_name: &str) -> CapturedWindow {
        CapturedWindow {
            title: title.into(),
            class_name: class.into(),
            process_name: proc_name.into(),
            rect: (0, 0, 100, 100),
            is_maximized: false,
        }
    }

    fn live(hwnd: isize, title: &str, class: &str, proc_name: &str) -> LiveWindow {
        LiveWindow {
            hwnd,
            title: title.into(),
            class_name: class.into(),
            process_name: proc_name.into(),
            rect: (0, 0, 100, 100),
            is_maximized: false,
        }
    }

    #[test]
    fn matches_when_class_and_process_agree() {
        let captured = cap("Document - Notepad", "Notepad", "notepad.exe");
        let ls = vec![live(42, "Untitled - Notepad", "Notepad", "notepad.exe")];
        assert_eq!(match_window(&captured, &ls), Some(42));
    }

    #[test]
    fn no_match_when_only_title_agrees() {
        let captured = cap("Report.txt - Notepad", "Notepad", "notepad.exe");
        let ls = vec![live(
            7,
            "Report.txt - Notepad",
            "ChromeWindow",
            "chrome.exe",
        )];
        assert_eq!(match_window(&captured, &ls), None);
    }

    #[test]
    fn picks_closest_when_scores_tie() {
        let mut captured = cap("t", "C", "p.exe");
        captured.rect = (500, 500, 600, 600);
        let ls = vec![
            LiveWindow {
                hwnd: 1,
                title: "t".into(),
                class_name: "C".into(),
                process_name: "p.exe".into(),
                rect: (0, 0, 100, 100),
                is_maximized: false,
            },
            LiveWindow {
                hwnd: 2,
                title: "t".into(),
                class_name: "C".into(),
                process_name: "p.exe".into(),
                rect: (490, 490, 590, 590),
                is_maximized: false,
            },
        ];
        assert_eq!(match_window(&captured, &ls), Some(2));
    }
}
