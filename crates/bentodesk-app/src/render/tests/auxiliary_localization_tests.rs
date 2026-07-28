use super::{
    bulk_manager_action_label, icon_kind_label, localized_icon_wire_label,
    localized_suggestor_group_name, localized_visible_range, rules_action_label,
    rules_preview_hit_label,
};
use crate::business::{
    bulk_manager_panel::BulkManagerPointerHit, icons::IconKind, rules_wizard::RulesWizardPointerHit,
};

#[test]
fn auxiliary_surfaces_use_user_facing_chinese_labels() {
    assert_eq!(icon_kind_label(IconKind::Folder, true), "文件夹");
    assert_eq!(localized_icon_wire_label("settings", true), "设置");
    assert_eq!(
        bulk_manager_action_label(BulkManagerPointerHit::Delete, true),
        "删除"
    );
    assert_eq!(
        rules_action_label(
            RulesWizardPointerHit::NextSave,
            crate::business::rules_wizard::WizardStep::Conditions,
            true,
        ),
        "下一步"
    );
    assert_eq!(localized_suggestor_group_name("Documents", true), "文档");
    assert_eq!(
        localized_suggestor_group_name("自定义分组", true),
        "自定义分组"
    );
    assert_eq!(
        localized_visible_range(5, 20, 8, true).as_deref(),
        Some("第 6–13 项，共 20 项")
    );
}

#[test]
fn rules_preview_never_exposes_debug_hit_prefix() {
    let label = rules_preview_hit_label("文档.txt", 0, true);
    assert_eq!(label, "命中项 1：文档.txt");
    assert!(!label.contains("hit:"));
}
