use super::*;

// ── M1d 2026-05-29 — Performance §5 + Startup management §6 ────────────
//
// Replaces the deleted bespoke 高级 / 未来集成验证 sections (native-only, not
// in Tauri) with the two genuine Tauri sections from
// `SettingsPanel.tsx:601-698`. Performance has 3 SliderRows (no
// conditionals); Startup has 2 toggles + 2 conditional steppers + 1 toggle
// + 1 conditional slider, so its height is dynamic (gated by
// `crash_restart_enabled` and `safe_start_after_hibernation`).
//
// The number-stepper rects (− value +) and slider track rect below are the
// generalized descendants of the old `settings_advanced_num_*` /
// `settings_advanced_slider_rect` templates.

/// M1d — number of SliderRows in the Performance section (展开/收起/缓存).
pub const SETTINGS_PERF_ROW_COUNT: u8 = 3;

/// M1d / Tauri parity — native-number spinner side target. Two 16-DIP side
/// targets plus the 40-DIP value band form Tauri's 72-DIP number input.
pub const SETTINGS_NUM_BTN_W: f32 = 16.0;
pub const SETTINGS_NUM_BTN_H: f32 = 30.0;

/// M1d — number-stepper value label width (between − and +).
pub const SETTINGS_NUM_VALUE_W: f32 = 40.0;

/// M1d — slider track geometry shared by Performance + hibernate sliders.
pub const SETTINGS_SLIDER_W: f32 = 200.0;
pub const SETTINGS_SLIDER_TRACK_H: f32 = 4.0;
pub const SETTINGS_SLIDER_THUMB_D: f32 = 14.0;

/// M1d — SliderRow total height. Tauri `.slider-row` is a column: a
/// `label + tabular value` line on top, the `<input type=range>` below
/// (`SettingsPanel.tsx:848-871`). 24 (label line) + 20 (track band) = 44.
pub const SETTINGS_SLIDER_ROW_H: f32 = 44.0;

/// M1d — height of a one-line `.settings-row__desc` caption under a toggle.
pub const SETTINGS_DESC_H: f32 = 18.0;

// ── Performance §5 geometry (3 SliderRows, no conditionals) ────────────

/// M1d / M1i fidelity / G3 parity — scroll-space Y at which the Performance
/// group title starts, PINNED at the fixed [`SETTINGS_SOURCE_ROW_VISIBLE_MAX`]
/// source reserve baseline. The live source-count reflow is applied by callers
/// folding [`settings_sources_reserve_delta`] into `scroll_offset_y`
/// (single-base-offset mechanism) rather than threading the count through every
/// perf-and-below rect fn — so this and all sections rooted on it keep their
/// `(viewport, scroll_offset_y)` signatures untouched.
///
/// G3 parity (2026-06-01): §3 Appearance + §4 DisplayMode now sit between §2
/// Paths and §5 Performance (Tauri body order). Their content heights are added
/// here so Performance and everything below it shift down by exactly the
/// Appearance grid + DisplayMode group. [`settings_appearance_origin_y_offset`]
/// already equals the §2 Paths terms (m2 origin + sources + path + watch); we
/// extend it by the Appearance and DisplayMode section content heights. Takes
/// `viewport` because the Appearance grid height is body-width driven (the same
/// width paint/hit feed `settings_appearance_content_height`), keeping the
/// offset chain exactly consistent with the rendered Appearance section.
fn settings_perf_origin_y_offset(viewport: Size) -> f32 {
    settings_appearance_origin_y_offset()
        + settings_appearance_content_height(viewport)
        + settings_display_mode_content_height()
}

/// M1d — `性能 / Performance` group title rect.
pub fn settings_performance_label_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let body = settings_body_rect(viewport);
    let origin_y = settings_body_content_origin(viewport, scroll_offset_y);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: origin_y + settings_perf_origin_y_offset(viewport),
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// M1d — full SliderRow rect for Performance slider `index` (0..3).
pub fn settings_performance_slider_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    index: u8,
) -> Rect {
    let label = settings_performance_label_rect(viewport, scroll_offset_y);
    Rect {
        x: label.x,
        y: label.bottom() + SETTINGS_SLIDER_ROW_H * index as f32,
        width: label.width,
        height: SETTINGS_SLIDER_ROW_H,
    }
}

/// M1d — slider track/hit rect inside a Performance SliderRow (full-width
/// band on the lower line of the row).
pub fn settings_performance_slider_rect(viewport: Size, scroll_offset_y: f32, index: u8) -> Rect {
    let row = settings_performance_slider_row_rect(viewport, scroll_offset_y, index);
    Rect {
        x: row.x,
        y: row.bottom() - SETTINGS_SLIDER_THUMB_D - 4.0,
        width: row.width,
        height: SETTINGS_SLIDER_THUMB_D,
    }
}

// ── Startup management §6 geometry (dynamic height) ────────────────────
//
// Row order (visible subset depends on the two gating bools):
//   0  高优先级启动 toggle              (always)
//   0d desc
//   1  崩溃自动重启 toggle              (always) — gates 2/3
//   1d desc
//   2  最大重试次数 stepper             (crash_restart only)
//   3  崩溃窗口（秒）stepper            (crash_restart only)
//   4  休眠安全恢复 toggle              (always) — gates the slider
//   4d desc
//   5  恢复延迟 SliderRow               (hibernation only)

/// M1d — `启动管理 / Startup Management` group title rect. Sits below the
/// Performance section (3 SliderRows + a section gap).
pub fn settings_startup_label_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let last_perf = settings_performance_slider_row_rect(
        viewport,
        scroll_offset_y,
        SETTINGS_PERF_ROW_COUNT - 1,
    );
    let body = settings_body_rect(viewport);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: last_perf.bottom() + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// M1d — `高优先级启动` toggle row rect (row 0, always shown).
pub fn settings_startup_high_priority_row_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let label = settings_startup_label_rect(viewport, scroll_offset_y);
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_ROW_H_M1,
    }
}

/// M1d — `崩溃自动重启` toggle row rect (row 1, always shown). Sits below the
/// high-priority row + its description caption.
pub fn settings_crash_restart_row_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let prev = settings_startup_high_priority_row_rect(viewport, scroll_offset_y);
    Rect {
        x: prev.x,
        y: prev.bottom() + SETTINGS_DESC_H,
        width: prev.width,
        height: SETTINGS_ROW_H_M1,
    }
}

/// M1d — `最大重试次数` stepper row rect (row 2). Only laid out / painted /
/// hit-tested when `crash_restart_enabled`. Sits below the crash-restart row
/// + its description caption.
pub fn settings_crash_max_retries_row_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let prev = settings_crash_restart_row_rect(viewport, scroll_offset_y);
    Rect {
        x: prev.x,
        y: prev.bottom() + SETTINGS_DESC_H,
        width: prev.width,
        height: SETTINGS_ROW_H_M1,
    }
}

/// M1d — `崩溃窗口（秒）` stepper row rect (row 3). Conditional on
/// `crash_restart_enabled`; sits directly below the retries stepper row.
pub fn settings_crash_window_row_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let prev = settings_crash_max_retries_row_rect(viewport, scroll_offset_y);
    Rect {
        x: prev.x,
        y: prev.bottom(),
        width: prev.width,
        height: SETTINGS_ROW_H_M1,
    }
}

/// M1d — `休眠安全恢复` toggle row rect (row 4, always shown). Its Y depends
/// on whether the two crash steppers are present, so it takes the gating
/// bool as a parameter (geometry stays pure — never reads global state).
///
/// When the steppers are hidden, the row sits below the crash-restart
/// toggle's always-shown description caption (so `+ SETTINGS_DESC_H` clears
/// it). When the steppers are shown, the last stepper (崩溃窗口) has NO
/// description caption, so the row sits directly below it (no extra gap).
pub fn settings_safe_start_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
) -> Rect {
    let (anchor, desc_gap) = if crash_restart_enabled {
        (
            settings_crash_window_row_rect(viewport, scroll_offset_y),
            0.0,
        )
    } else {
        (
            settings_crash_restart_row_rect(viewport, scroll_offset_y),
            SETTINGS_DESC_H,
        )
    };
    Rect {
        x: anchor.x,
        y: anchor.bottom() + desc_gap,
        width: anchor.width,
        height: SETTINGS_ROW_H_M1,
    }
}

/// M1d — `恢复延迟` SliderRow rect (row 5). Conditional on
/// `safe_start_after_hibernation`; positioned below the safe-start toggle +
/// its description caption. Takes `crash_restart_enabled` to chain through
/// the dynamic safe-start anchor.
pub fn settings_hibernate_slider_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
) -> Rect {
    let prev = settings_safe_start_row_rect(viewport, scroll_offset_y, crash_restart_enabled);
    Rect {
        x: prev.x,
        y: prev.bottom() + SETTINGS_DESC_H,
        width: prev.width,
        height: SETTINGS_SLIDER_ROW_H,
    }
}

/// M1d — slider track/hit rect inside the hibernate SliderRow.
pub fn settings_hibernate_slider_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
) -> Rect {
    let row = settings_hibernate_slider_row_rect(viewport, scroll_offset_y, crash_restart_enabled);
    Rect {
        x: row.x,
        y: row.bottom() - SETTINGS_SLIDER_THUMB_D - 4.0,
        width: row.width,
        height: SETTINGS_SLIDER_THUMB_D,
    }
}

/// M1d — right-anchored toggle hit-box inside a Startup-section toggle row.
/// Shared by the renderer (paints the rocker centred in this box) and the
/// hit-tester. Mirrors `SETTINGS_TOP_TOGGLE_HIT_*` so click ergonomics match
/// the General-section toggles.
pub fn settings_startup_toggle_hit_rect(row: Rect) -> Rect {
    Rect {
        x: row.right() - SETTINGS_TOP_TOGGLE_HIT_W,
        y: row.y + (row.height - SETTINGS_TOP_TOGGLE_HIT_H) * 0.5,
        width: SETTINGS_TOP_TOGGLE_HIT_W,
        height: SETTINGS_TOP_TOGGLE_HIT_H,
    }
}

/// M1d — number-stepper "+" button rect, right-anchored inside `row`.
pub fn settings_stepper_plus_rect(row: Rect) -> Rect {
    Rect {
        x: row.right() - SETTINGS_NUM_BTN_W,
        y: row.y + (row.height - SETTINGS_NUM_BTN_H) * 0.5,
        width: SETTINGS_NUM_BTN_W,
        height: SETTINGS_NUM_BTN_H,
    }
}

/// M1d — number-stepper value label rect (between − and +).
pub fn settings_stepper_value_rect(row: Rect) -> Rect {
    let plus = settings_stepper_plus_rect(row);
    Rect {
        x: plus.x - SETTINGS_NUM_VALUE_W,
        y: plus.y,
        width: SETTINGS_NUM_VALUE_W,
        height: SETTINGS_NUM_BTN_H,
    }
}

/// M1d — number-stepper "−" button rect (left of the value label).
pub fn settings_stepper_minus_rect(row: Rect) -> Rect {
    let value = settings_stepper_value_rect(row);
    Rect {
        x: value.x - SETTINGS_NUM_BTN_W,
        y: value.y,
        width: SETTINGS_NUM_BTN_W,
        height: SETTINGS_NUM_BTN_H,
    }
}

/// Visible Tauri `.settings-row__number-input` shell enclosing the existing
/// decrement / value / increment targets.
pub fn settings_stepper_input_rect(row: Rect) -> Rect {
    let minus = settings_stepper_minus_rect(row);
    let plus = settings_stepper_plus_rect(row);
    Rect {
        x: minus.x,
        y: minus.y,
        width: plus.right() - minus.x,
        height: SETTINGS_NUM_BTN_H,
    }
}

/// M1d — combined height of the Performance + Startup sections, fed into
/// `settings_body_content_height`. Conditional rows make this dynamic, so
/// the two gating bools are parameters (geometry never reads global state).
pub(super) fn settings_perf_startup_content_height(
    viewport: Size,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
) -> f32 {
    // Performance: title + 3 SliderRows + section gap.
    let perf = SETTINGS_SECTION_LABEL_H
        + SETTINGS_SLIDER_ROW_H * SETTINGS_PERF_ROW_COUNT as f32
        + SETTINGS_SECTION_GAP;
    // Startup: title + the visible-row stack measured off the last laid-out
    // element so the helpers below are the single source of layout truth.
    let title_top = settings_startup_label_rect(viewport, 0.0).y;
    let last_bottom = if safe_start_after_hibernation {
        settings_hibernate_slider_row_rect(viewport, 0.0, crash_restart_enabled).bottom()
    } else {
        // safe-start toggle + its description caption.
        settings_safe_start_row_rect(viewport, 0.0, crash_restart_enabled).bottom()
            + SETTINGS_DESC_H
    };
    let startup = (last_bottom - title_top) + SETTINGS_SECTION_GAP;
    perf + startup
}
