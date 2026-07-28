use super::*;
use bentodesk_style::tokens as style_tokens;
use std::borrow::Cow;

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.01,
        "expected {expected}, got {actual}"
    );
}

include!("tests/01_stack_bloom_timing_matches_tauri_entry_stagger_contract.rs");
include!("tests/02_stack_bloom_compact_content_layout_keeps_icon_and_title_.rs");
