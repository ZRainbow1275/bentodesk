use super::{
    ITEM_LABEL_BASE_FONT_PX, ITEM_LABEL_BOTTOM_INSET_PX, ITEM_LABEL_MIN_FONT_PX,
    item_icon_slots_for_card, item_label_font_size_for_width, item_label_group_font_size,
    item_label_rect_for_card, item_label_text_color_for_reference, item_label_visible_name,
};
use bentodesk_style::Rect;
use bentodesk_style::tokens::{PALETTE_DARK, PALETTE_LIGHT};

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.01,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn short_item_label_keeps_base_font_size() {
    assert_eq!(
        item_label_font_size_for_width("Docs", 80.0),
        ITEM_LABEL_BASE_FONT_PX
    );
}

#[test]
fn item_label_uses_reference_frame_effective_size_and_rail() {
    assert_close(ITEM_LABEL_BASE_FONT_PX, 11.0);
    assert_close(ITEM_LABEL_BOTTOM_INSET_PX, 8.0);
}

#[test]
fn long_item_label_shrinks_instead_of_relying_on_ellipsis() {
    let got = item_label_font_size_for_width("Roxy Browser", 58.0);

    assert!(
        got < ITEM_LABEL_BASE_FONT_PX,
        "long item labels must shrink before drawing, got {got}"
    );
    assert!(
        got >= ITEM_LABEL_MIN_FONT_PX,
        "item labels must keep the shared readability floor, got {got}"
    );
}

#[test]
fn extremely_narrow_item_label_bottoms_out_at_readability_floor() {
    assert_eq!(
        item_label_font_size_for_width("very-long-file-name.txt", 20.0),
        ITEM_LABEL_MIN_FONT_PX
    );
}

#[test]
fn item_grid_uses_one_uniform_font_size_for_every_visible_label() {
    let labels = [("Docs", 80.0), ("DB Browser (SQLCipher)", 80.0)];
    let group = item_label_group_font_size(labels.into_iter());

    assert_eq!(group, ITEM_LABEL_MIN_FONT_PX);
    assert!(group < item_label_font_size_for_width("Docs", 80.0));
}

#[test]
fn shortcut_extensions_are_removed_before_fit() {
    assert_eq!(item_label_visible_name("Project.lnk"), "Project");
    assert_eq!(item_label_visible_name("Docs.URL"), "Docs");
    assert_eq!(item_label_visible_name("archive.txt"), "archive.txt");
}

#[test]
fn item_label_color_uses_tauri_secondary_text_ink() {
    assert_eq!(
        item_label_text_color_for_reference(PALETTE_DARK),
        PALETTE_DARK.text_secondary
    );
    assert_eq!(
        item_label_text_color_for_reference(PALETTE_LIGHT),
        PALETTE_LIGHT.text_secondary
    );
}

#[test]
fn standard_item_label_uses_reference_lower_text_rail() {
    let card = Rect {
        x: 10.0,
        y: 20.0,
        width: 88.0,
        height: 78.0,
    };

    let label = item_label_rect_for_card(card, 1.0, ITEM_LABEL_BASE_FONT_PX);
    let expected_h = ITEM_LABEL_BASE_FONT_PX * 1.4;

    assert_close(label.x, 14.0);
    assert_close(label.width, 80.0);
    assert_close(label.height, expected_h);
    assert_close(
        label.y,
        card.bottom() - expected_h - ITEM_LABEL_BOTTOM_INSET_PX,
    );
}

#[test]
fn scaled_item_label_keeps_bottom_inset_with_card_transform() {
    let card = Rect {
        x: 4.0,
        y: 6.0,
        width: 120.0,
        height: 90.0,
    };
    let scale = 1.25;

    let label = item_label_rect_for_card(card, scale, ITEM_LABEL_BASE_FONT_PX);
    let expected_h = ITEM_LABEL_BASE_FONT_PX * 1.4 * scale;

    assert_close(label.x, card.x + 4.0 * scale);
    assert_close(label.width, card.width - 8.0 * scale);
    assert_close(label.height, expected_h);
    assert_close(
        label.y,
        card.bottom() - expected_h - ITEM_LABEL_BOTTOM_INSET_PX * scale,
    );
}

#[test]
fn standard_item_icon_uses_36px_container_and_24px_render_slot() {
    let card = Rect {
        x: 10.0,
        y: 20.0,
        width: 88.0,
        height: 78.0,
    };

    let (container, render) = item_icon_slots_for_card(card, false, 1.0);

    assert_close(container.x, 36.0);
    assert_close(container.y, 28.0);
    assert_close(container.width, 36.0);
    assert_close(container.height, 36.0);
    assert_close(render.x, 42.0);
    assert_close(render.y, 34.0);
    assert_close(render.width, 24.0);
    assert_close(render.height, 24.0);
}

#[test]
fn wide_item_icon_uses_28px_container_and_20px_render_slot() {
    let card = Rect {
        x: 5.0,
        y: 10.0,
        width: 200.0,
        height: 78.0,
    };

    let (container, render) = item_icon_slots_for_card(card, true, 1.0);

    assert_close(container.x, 91.0);
    assert_close(container.y, 18.0);
    assert_close(container.width, 28.0);
    assert_close(container.height, 28.0);
    assert_close(render.x, 95.0);
    assert_close(render.y, 22.0);
    assert_close(render.width, 20.0);
    assert_close(render.height, 20.0);
}
