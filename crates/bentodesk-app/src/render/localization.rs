use super::*;

pub(super) fn icon_kind_label(kind: IconKind, zh: bool) -> &'static str {
    match (kind, zh) {
        (IconKind::Folder, true) => "文件夹",
        (IconKind::Document, true) => "文档",
        (IconKind::Image, true) => "图片",
        (IconKind::Music, true) => "音乐",
        (IconKind::Video, true) => "视频",
        (IconKind::Code, true) => "代码",
        (IconKind::Download, true) => "下载",
        (IconKind::Archive, true) => "归档",
        (IconKind::Star, true) => "收藏",
        (IconKind::Bookmark, true) => "书签",
        (IconKind::Tag, true) => "标签",
        (IconKind::Globe, true) => "网络",
        (IconKind::Lightning, true) => "快捷",
        (IconKind::Briefcase, true) => "工作",
        (IconKind::Gamepad, true) => "游戏",
        (IconKind::Palette, true) => "调色板",
        (IconKind::ArrowRight, true) => "箭头",
        (IconKind::Trash, true) => "回收站",
        (IconKind::Search, true) => "搜索",
        (IconKind::Copy, true) => "复制",
        (IconKind::ExternalLink, true) => "外部链接",
        (IconKind::FolderOpen, true) => "打开文件夹",
        (IconKind::Camera, true) => "相机",
        (IconKind::Columns, true) => "分栏",
        (IconKind::X, true) => "关闭",
        (IconKind::Edit, true) => "编辑",
        (IconKind::Grid, true) => "网格",
        (IconKind::Square, true) => "方框",
        (IconKind::Pin, true) => "固定",
        (IconKind::Settings, true) => "设置",
        (IconKind::Folder, false) => "Folder",
        (IconKind::Document, false) => "Document",
        (IconKind::Image, false) => "Image",
        (IconKind::Music, false) => "Music",
        (IconKind::Video, false) => "Video",
        (IconKind::Code, false) => "Code",
        (IconKind::Download, false) => "Download",
        (IconKind::Archive, false) => "Archive",
        (IconKind::Star, false) => "Star",
        (IconKind::Bookmark, false) => "Bookmark",
        (IconKind::Tag, false) => "Tag",
        (IconKind::Globe, false) => "Globe",
        (IconKind::Lightning, false) => "Lightning",
        (IconKind::Briefcase, false) => "Briefcase",
        (IconKind::Gamepad, false) => "Gamepad",
        (IconKind::Palette, false) => "Palette",
        (IconKind::ArrowRight, false) => "Arrow",
        (IconKind::Trash, false) => "Trash",
        (IconKind::Search, false) => "Search",
        (IconKind::Copy, false) => "Copy",
        (IconKind::ExternalLink, false) => "External link",
        (IconKind::FolderOpen, false) => "Open folder",
        (IconKind::Camera, false) => "Camera",
        (IconKind::Columns, false) => "Columns",
        (IconKind::X, false) => "Close",
        (IconKind::Edit, false) => "Edit",
        (IconKind::Grid, false) => "Grid",
        (IconKind::Square, false) => "Square",
        (IconKind::Pin, false) => "Pin",
        (IconKind::Settings, false) => "Settings",
    }
}

pub(super) fn localized_icon_wire_label(wire: &str, zh: bool) -> &str {
    IconKind::from_str_opt(wire)
        .map(|kind| icon_kind_label(kind, zh))
        .unwrap_or(wire)
}

pub(super) fn localized_visible_range(
    start: usize,
    count: usize,
    visible_limit: usize,
    zh: bool,
) -> Option<SmolStr> {
    if count <= visible_limit {
        return None;
    }
    let start = start.min(count.saturating_sub(visible_limit));
    let end = count.min(start + visible_limit);
    Some(SmolStr::new(if zh {
        format!("第 {}–{} 项，共 {} 项", start + 1, end, count)
    } else {
        format!("Items {}–{} of {}", start + 1, end, count)
    }))
}

pub(super) fn bulk_manager_action_label(
    hit: bulk_manager_panel::BulkManagerPointerHit,
    zh: bool,
) -> &'static str {
    use bulk_manager_panel::BulkManagerPointerHit as Hit;
    match (hit, zh) {
        (Hit::SelectAll, true) => "全选",
        (Hit::Invert, true) => "反选",
        (Hit::Hide, true) => "隐藏",
        (Hit::Show, true) => "显示",
        (Hit::LayoutGrid, true) => "网格",
        (Hit::LayoutRow, true) => "横排",
        (Hit::LayoutColumn, true) => "纵列",
        (Hit::LayoutSpiral, true) => "环绕",
        (Hit::LayoutOrganic, true) => "自然",
        (Hit::Update, true) => "刷新",
        (Hit::TextEdit, true) => "文字",
        (Hit::IconPicker, true) => "图标",
        (Hit::AccentPicker, true) => "颜色",
        (Hit::Delete, true) => "删除",
        (Hit::Move, true) => "移动",
        (Hit::Close, true) => "关闭",
        (Hit::SelectAll, false) => "All",
        (Hit::Invert, false) => "Invert",
        (Hit::Hide, false) => "Hide",
        (Hit::Show, false) => "Show",
        (Hit::LayoutGrid, false) => "Grid",
        (Hit::LayoutRow, false) => "Row",
        (Hit::LayoutColumn, false) => "Column",
        (Hit::LayoutSpiral, false) => "Spiral",
        (Hit::LayoutOrganic, false) => "Organic",
        (Hit::Update, false) => "Refresh",
        (Hit::TextEdit, false) => "Text",
        (Hit::IconPicker, false) => "Icon",
        (Hit::AccentPicker, false) => "Color",
        (Hit::Delete, false) => "Delete",
        (Hit::Move, false) => "Move",
        (Hit::Close, false) => "Close",
        (Hit::SearchInput | Hit::Sort(_) | Hit::Row(_), _) => "",
    }
}

pub(super) fn bulk_manager_sort_label(key: bulk_manager_panel::SortKey, zh: bool) -> &'static str {
    use bulk_manager_panel::SortKey;
    match (key, zh) {
        (SortKey::Name, true) => "名称",
        (SortKey::Items, true) => "项目数",
        (SortKey::Accent, true) => "颜色",
        (SortKey::Size, true) => "尺寸",
        (SortKey::Name, false) => "Name",
        (SortKey::Items, false) => "Items",
        (SortKey::Accent, false) => "Accent",
        (SortKey::Size, false) => "Size",
    }
}

pub(super) fn bulk_text_edit_field_label(
    field: bulk_manager_panel::BulkTextEditField,
    zh: bool,
) -> &'static str {
    use bulk_manager_panel::BulkTextEditField;
    match (field, zh) {
        (BulkTextEditField::Alias, true) => "别名",
        (BulkTextEditField::Icon, true) => "图标",
        (BulkTextEditField::Accent, true) => "颜色",
        (BulkTextEditField::CapsuleSize, true) => "胶囊尺寸",
        (BulkTextEditField::DisplayMode, true) => "显示模式",
        (BulkTextEditField::Alias, false) => "alias",
        (BulkTextEditField::Icon, false) => "icon",
        (BulkTextEditField::Accent, false) => "accent",
        (BulkTextEditField::CapsuleSize, false) => "capsule size",
        (BulkTextEditField::DisplayMode, false) => "display mode",
    }
}

pub(super) fn bulk_text_edit_placeholder(
    field: bulk_manager_panel::BulkTextEditField,
    zh: bool,
) -> &'static str {
    use bulk_manager_panel::BulkTextEditField;
    match (field, zh) {
        (BulkTextEditField::Alias, true) => "留空可清除别名",
        (BulkTextEditField::Icon, true) => "例如 folder、star、archive",
        (BulkTextEditField::Accent, true) => "例如 #3b82f6",
        (BulkTextEditField::CapsuleSize, true) => "small / medium / large",
        (BulkTextEditField::DisplayMode, true) => "hover / always / click / clear",
        (_, false) => field.placeholder(),
    }
}

pub(super) fn timeline_action_label(
    hit: timeline_panel::TimelinePointerHit,
    zh: bool,
) -> &'static str {
    use timeline_panel::TimelinePointerHit as Hit;
    match (hit, zh) {
        (Hit::Save, true) => "保存",
        (Hit::Pin, true) => "固定",
        (Hit::Restore, true) => "恢复",
        (Hit::Delete, true) => "删除",
        (Hit::Close, true) => "关闭",
        (Hit::Save, false) => "Save",
        (Hit::Pin, false) => "Pin",
        (Hit::Restore, false) => "Restore",
        (Hit::Delete, false) => "Delete",
        (Hit::Close, false) => "Close",
        (Hit::Row(_), _) => "",
    }
}

pub(super) fn capsule_action_label(hit: CapsulePickerHit, zh: bool) -> &'static str {
    match (hit, zh) {
        (CapsulePickerHit::Capture, true) => "保存当前",
        (CapsulePickerHit::Restore, true) => "恢复",
        (CapsulePickerHit::Delete, true) => "删除",
        (CapsulePickerHit::Close, true) => "关闭",
        (CapsulePickerHit::Capture, false) => "Save current",
        (CapsulePickerHit::Restore, false) => "Restore",
        (CapsulePickerHit::Delete, false) => "Delete",
        (CapsulePickerHit::Close, false) => "Close",
        (
            CapsulePickerHit::Hint
            | CapsulePickerHit::Error
            | CapsulePickerHit::Empty
            | CapsulePickerHit::Row(_),
            _,
        ) => "",
    }
}

pub(super) fn snapshot_action_label(
    hit: snapshot_picker::SnapshotPickerPointerHit,
    zh: bool,
) -> &'static str {
    use snapshot_picker::SnapshotPickerPointerHit as Hit;
    match (hit, zh) {
        (Hit::Save, true) => "保存",
        (Hit::Load, true) => "载入",
        (Hit::Delete, true) => "删除",
        (Hit::Timeline, true) => "时间线",
        (Hit::Close, true) => "关闭",
        (Hit::Save, false) => "Save",
        (Hit::Load, false) => "Load",
        (Hit::Delete, false) => "Delete",
        (Hit::Timeline, false) => "Timeline",
        (Hit::Close, false) => "Close",
        (Hit::Row(_), _) => "",
    }
}

pub(super) fn rules_action_label(
    hit: rules_wizard::RulesWizardPointerHit,
    step: WizardStep,
    zh: bool,
) -> &'static str {
    use rules_wizard::RulesWizardPointerHit as Hit;
    match (hit, step, zh) {
        (Hit::NextSave, WizardStep::Review, true) => "保存",
        (Hit::NextSave, _, true) => "下一步",
        (Hit::Predicate, _, true) => "条件",
        (Hit::Action, _, true) => "操作",
        (Hit::RunMode, _, true) => "运行",
        (Hit::Combine, _, true) => "关系",
        (Hit::AddCondition, _, true) => "添加",
        (Hit::RemoveCondition, _, true) => "移除",
        (Hit::NextCondition, _, true) => "下一项",
        (Hit::Edit, _, true) => "编辑",
        (Hit::Run, _, true) => "运行",
        (Hit::Delete, _, true) => "删除",
        (Hit::Close, _, true) => "关闭",
        (Hit::NextSave, WizardStep::Review, false) => "Save",
        (Hit::NextSave, _, false) => "Next",
        (Hit::Predicate, _, false) => "When",
        (Hit::Action, _, false) => "Action",
        (Hit::RunMode, _, false) => "Run",
        (Hit::Combine, _, false) => "All/Any",
        (Hit::AddCondition, _, false) => "Add",
        (Hit::RemoveCondition, _, false) => "Remove",
        (Hit::NextCondition, _, false) => "Next",
        (Hit::Edit, _, false) => "Edit",
        (Hit::Run, _, false) => "Run",
        (Hit::Delete, _, false) => "Delete",
        (Hit::Close, _, false) => "Close",
        (Hit::ConditionRow(_) | Hit::Row(_), _, _) => "",
    }
}

pub(super) fn wizard_step_label(step: WizardStep, zh: bool) -> &'static str {
    match (step, zh) {
        (WizardStep::Conditions, true) => "条件",
        (WizardStep::Action, true) => "操作",
        (WizardStep::Preview, true) => "预览",
        (WizardStep::Name, true) => "命名",
        (WizardStep::Review, true) => "确认",
        (WizardStep::Conditions, false) => "Conditions",
        (WizardStep::Action, false) => "Action",
        (WizardStep::Preview, false) => "Preview",
        (WizardStep::Name, false) => "Name",
        (WizardStep::Review, false) => "Review",
    }
}

pub(super) fn combine_label(mode: rules_wizard::CombineMode, zh: bool) -> &'static str {
    match (mode, zh) {
        (rules_wizard::CombineMode::All, true) => "全部满足",
        (rules_wizard::CombineMode::Any, true) => "任一满足",
        (rules_wizard::CombineMode::All, false) => "all",
        (rules_wizard::CombineMode::Any, false) => "any",
    }
}

pub(super) fn predicate_label(kind: PredicateKind, zh: bool) -> &'static str {
    match (kind, zh) {
        (PredicateKind::NameStartsWith, true) => "名称开头是",
        (PredicateKind::NameContains, true) => "名称包含",
        (PredicateKind::NameEndsWith, true) => "名称结尾是",
        (PredicateKind::ExtensionIn, true) => "扩展名属于",
        (PredicateKind::CreatedBefore, true) => "创建时间早于指定天数",
        (PredicateKind::ModifiedBefore, true) => "修改时间早于指定天数",
        (PredicateKind::SizeGreaterThan, true) => "文件大于指定大小",
        (PredicateKind::InZone, true) => "位于区域",
        (PredicateKind::OnDesktop, true) => "位于桌面",
        (PredicateKind::NameStartsWith, false) => "name starts with",
        (PredicateKind::NameContains, false) => "name contains",
        (PredicateKind::NameEndsWith, false) => "name ends with",
        (PredicateKind::ExtensionIn, false) => "extension in",
        (PredicateKind::CreatedBefore, false) => "created before days",
        (PredicateKind::ModifiedBefore, false) => "modified before days",
        (PredicateKind::SizeGreaterThan, false) => "size greater than",
        (PredicateKind::InZone, false) => "in zone",
        (PredicateKind::OnDesktop, false) => "on desktop",
    }
}

pub(super) fn action_label(kind: ActionKind, zh: bool) -> &'static str {
    match (kind, zh) {
        (ActionKind::MoveToZone, true) => "移动到区域",
        (ActionKind::MoveToFolder, true) => "移动到文件夹",
        (ActionKind::DeleteToRecycleBin, true) => "移入回收站",
        (ActionKind::Tag, true) => "添加标签",
        (ActionKind::Notify, true) => "发送通知",
        (ActionKind::MoveToZone, false) => "move to zone",
        (ActionKind::MoveToFolder, false) => "move to folder",
        (ActionKind::DeleteToRecycleBin, false) => "delete to recycle bin",
        (ActionKind::Tag, false) => "tag",
        (ActionKind::Notify, false) => "notify",
    }
}

pub(super) fn run_mode_label(mode: RunModeChoice, zh: bool) -> &'static str {
    match (mode, zh) {
        (RunModeChoice::OnDemand, true) => "手动运行",
        (RunModeChoice::OnFileChange, true) => "文件变化时运行",
        (RunModeChoice::Interval, true) => "定时运行",
        (RunModeChoice::OnDemand, false) => "on demand",
        (RunModeChoice::OnFileChange, false) => "on file change",
        (RunModeChoice::Interval, false) => "interval",
    }
}

pub(super) fn rules_preview_hit_label(hit: &str, index: usize, zh: bool) -> SmolStr {
    SmolStr::new(if zh {
        format!("命中项 {}：{hit}", index + 1)
    } else {
        format!("Match {}: {hit}", index + 1)
    })
}

pub(super) fn confidence_tone_label(
    tone: smart_group_suggestor::ConfidenceTone,
    zh: bool,
) -> &'static str {
    use smart_group_suggestor::ConfidenceTone;
    match (tone, zh) {
        (ConfidenceTone::Low, true) => "低",
        (ConfidenceTone::Medium, true) => "中",
        (ConfidenceTone::High, true) => "高",
        (ConfidenceTone::Low, false) => "Low",
        (ConfidenceTone::Medium, false) => "Medium",
        (ConfidenceTone::High, false) => "High",
    }
}

pub(super) fn localized_suggestor_group_name(name: &str, zh: bool) -> &str {
    if !zh {
        return name;
    }
    match name {
        "Documents" => "文档",
        "Images" => "图片",
        "Videos" => "视频",
        "Audio" => "音频",
        "Code" => "代码",
        "Archives" => "压缩包",
        "Executables" => "程序",
        "Shortcuts" => "快捷方式",
        "Today" => "今天",
        "This Week" => "本周",
        "This Month" => "本月",
        "Older" => "更早",
        _ => name,
    }
}

pub(super) fn localized_suggestor_rule_summary(
    suggestion: &bentodesk_backend::grouping::SuggestedGroup,
    zh: bool,
) -> SmolStr {
    use bentodesk_backend::layout::GroupRuleType;
    match suggestion.rule.rule_type {
        GroupRuleType::Extension => suggestion
            .rule
            .extensions
            .as_ref()
            .filter(|extensions| !extensions.is_empty())
            .map(|extensions| SmolStr::new(extensions.join(", ")))
            .unwrap_or_else(|| SmolStr::new_static(if zh { "按扩展名" } else { "Extension" })),
        GroupRuleType::NamePattern => suggestion
            .rule
            .pattern
            .as_deref()
            .filter(|pattern| !pattern.trim().is_empty())
            .map(SmolStr::new)
            .unwrap_or_else(|| {
                SmolStr::new_static(if zh {
                    "按名称模式"
                } else {
                    "Name pattern"
                })
            }),
        GroupRuleType::ModifiedDate => SmolStr::new_static(if zh {
            "按修改时间"
        } else {
            "Modified date"
        }),
    }
}
