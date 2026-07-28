//! Native shell owner: `context_menu_model`.

use super::*;

pub(super) fn handle_rbutton_up(root: &AppRoot, hwnd: HWND, x: f32, y: f32) {
    let app = root.app.borrow();
    if app.settings_open.get() {
        return;
    }
    if app
        .active_context_menu
        .borrow()
        .as_ref()
        .is_some_and(|session| popover::context_menu_contains(session, x, y))
    {
        return;
    }
    if app.active_context_menu.borrow().is_some() {
        drop(app);
        close_context_menu_surface(root);
        return handle_rbutton_up(root, hwnd, x, y);
    }
    if let Some((_, zone_id, item_id)) = stack_bloom_preview_item_hit_for_point(&app, x, y) {
        let path = app.zones.get(zone_id).and_then(|zone| {
            zone.items
                .iter()
                .find(|item| item.id == item_id)
                .map(|item| item.path.to_string())
        });
        drop(app);
        if let Some(path) = path {
            show_item_context_menu(root, hwnd, x, y, zone_id, item_id, &path);
        }
        return;
    }
    if let Some((zone_id, item_id, path)) = ui::hit_test_zone_item(&app, x, y) {
        drop(app);
        show_item_context_menu(root, hwnd, x, y, zone_id, item_id, &path);
        return;
    }
    if let Some(id) = ui::hit_test_zone(&app, x, y) {
        drop(app);
        show_zone_context_menu(root, hwnd, x, y, id);
    }
}

#[inline]
pub(super) fn context_menu_text(zh_cn: &'static str, en_us: &'static str) -> &'static str {
    if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
        zh_cn
    } else {
        en_us
    }
}

pub(super) const ZONE_CONTEXT_EDIT_ID: usize = 1;
pub(super) const ZONE_CONTEXT_SMART_SUGGESTOR_ID: usize = 2;
pub(super) const ZONE_CONTEXT_AUTO_ORGANIZE_ID: usize = 3;
pub(super) const ZONE_CONTEXT_PIN_MINIBAR_ID: usize = 4;
pub(super) const ZONE_CONTEXT_UNSTACK_ID: usize = 5;
pub(super) const ZONE_CONTEXT_SAVE_SNAPSHOT_ID: usize = 6;
pub(super) const ZONE_CONTEXT_OPEN_SNAPSHOT_PICKER_ID: usize = 7;
pub(super) const ZONE_CONTEXT_DELETE_ID: usize = 8;
pub(super) const ZONE_CONTEXT_BIND_LIVE_FOLDER_ID: usize = 9;
pub(super) const ZONE_CONTEXT_REFRESH_LIVE_FOLDER_ID: usize = 10;
pub(super) const ZONE_CONTEXT_UNBIND_LIVE_FOLDER_ID: usize = 11;
pub(super) const ZONE_CONTEXT_OPEN_STACK_TRAY_ID: usize = 12;
pub(super) const ZONE_CONTEXT_SEARCH_ID: usize = 13;
pub(super) const ZONE_CONTEXT_CAPSULES_ID: usize = 14;
pub(super) const ZONE_CONTEXT_BULK_MANAGER_ID: usize = 15;
pub(super) const ZONE_CONTEXT_STACK_BASE_ID: usize = 100;

pub(super) fn zone_context_menu_rows(
    live_folder_bound: bool,
    stack_tray_available: bool,
    has_stack_targets: bool,
) -> popover::ContextMenuRows {
    let mut entries = popover::ContextMenuRows::new();
    entries.push(popover::ContextMenuRow::command(
        ZONE_CONTEXT_EDIT_ID,
        context_menu_text("编辑区域与样式", "Edit zone & style"),
        IconKind::Edit,
    ));
    entries.push(popover::ContextMenuRow::command(
        ZONE_CONTEXT_SEARCH_ID,
        context_menu_text("在区域内搜索", "Search in zone"),
        IconKind::Search,
    ));
    entries.push(popover::ContextMenuRow::command(
        ZONE_CONTEXT_SMART_SUGGESTOR_ID,
        context_menu_text("智能分组建议", "Smart suggestions"),
        IconKind::Lightning,
    ));
    entries.push(popover::ContextMenuRow::command(
        ZONE_CONTEXT_AUTO_ORGANIZE_ID,
        context_menu_text("自动排列项目", "Auto organize items"),
        IconKind::Grid,
    ));
    entries.push(popover::ContextMenuRow::command(
        ZONE_CONTEXT_PIN_MINIBAR_ID,
        context_menu_text("固定为迷你栏", "Pin as minibar"),
        IconKind::Pin,
    ));

    entries.push(popover::ContextMenuRow::separator());
    entries.push(popover::ContextMenuRow::command(
        ZONE_CONTEXT_BIND_LIVE_FOLDER_ID,
        if live_folder_bound {
            context_menu_text("更换绑定文件夹…", "Change bound folder…")
        } else {
            context_menu_text("绑定文件夹…", "Bind folder…")
        },
        IconKind::FolderOpen,
    ));
    if live_folder_bound {
        entries.push(popover::ContextMenuRow::command(
            ZONE_CONTEXT_REFRESH_LIVE_FOLDER_ID,
            context_menu_text("刷新文件夹内容", "Refresh folder contents"),
            IconKind::Folder,
        ));
        entries.push(popover::ContextMenuRow::command(
            ZONE_CONTEXT_UNBIND_LIVE_FOLDER_ID,
            context_menu_text("解除文件夹绑定", "Unbind folder"),
            IconKind::X,
        ));
    }

    if has_stack_targets || stack_tray_available {
        entries.push(popover::ContextMenuRow::separator());
        if has_stack_targets {
            entries.push(popover::ContextMenuRow::submenu(
                context_menu_text("与其他 Zone 叠放", "Stack with another Zone"),
                IconKind::Columns,
            ));
        }
        if stack_tray_available {
            entries.push(popover::ContextMenuRow::command(
                ZONE_CONTEXT_OPEN_STACK_TRAY_ID,
                context_menu_text("打开集合托盘", "Open stack tray"),
                IconKind::Columns,
            ));
            entries.push(popover::ContextMenuRow::command(
                ZONE_CONTEXT_UNSTACK_ID,
                context_menu_text("解除集合", "Unstack"),
                IconKind::Square,
            ));
        }
    }

    entries.push(popover::ContextMenuRow::separator());
    entries.push(popover::ContextMenuRow::command(
        ZONE_CONTEXT_BULK_MANAGER_ID,
        context_menu_text("批量管理项目…", "Bulk manage items…"),
        IconKind::Grid,
    ));
    entries.push(popover::ContextMenuRow::command(
        ZONE_CONTEXT_CAPSULES_ID,
        context_menu_text("上下文胶囊…", "Context capsules…"),
        IconKind::Archive,
    ));
    entries.push(popover::ContextMenuRow::command(
        ZONE_CONTEXT_SAVE_SNAPSHOT_ID,
        context_menu_text("保存布局快照", "Save layout snapshot"),
        IconKind::Camera,
    ));
    entries.push(popover::ContextMenuRow::command(
        ZONE_CONTEXT_OPEN_SNAPSHOT_PICKER_ID,
        context_menu_text("浏览布局快照…", "Browse layout snapshots…"),
        IconKind::Archive,
    ));

    entries.push(popover::ContextMenuRow::separator());
    entries.push(popover::ContextMenuRow::danger(
        ZONE_CONTEXT_DELETE_ID,
        context_menu_text("删除区域…", "Delete zone…"),
        IconKind::Trash,
    ));
    entries
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingZoneContextMenu {
    pub(super) zone_id: ZoneId,
    pub(super) stack_targets: Vec<(usize, ZoneId)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingItemContextMenu {
    pub(super) zone_id: ZoneId,
    pub(super) item_id: ZoneItemId,
    pub(super) path: SmolStr,
    pub(super) move_targets: Vec<(usize, ZoneId)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PendingTrayContextMenu {
    pub(super) main_visible: bool,
    pub(super) origin: DispatchPoint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ZoneContextAction {
    Edit,
    Search,
    SmartSuggestor,
    AutoOrganize,
    PinMinibar,
    OpenLiveFolderPicker,
    RefreshLiveFolder,
    UnbindLiveFolder,
    OpenStackTray,
    Unstack,
    SaveSnapshot,
    OpenSnapshotPicker,
    OpenCapsules,
    OpenBulkManager,
    StackWith(ZoneId),
    Delete,
}

pub(super) const ITEM_CONTEXT_OPEN_ID: usize = 1;
pub(super) const ITEM_CONTEXT_REVEAL_ID: usize = 2;
pub(super) const ITEM_CONTEXT_COPY_PATH_ID: usize = 3;
pub(super) const ITEM_CONTEXT_RENAME_FILE_ID: usize = 4;
pub(super) const ITEM_CONTEXT_DELETE_FILE_ID: usize = 5;
pub(super) const ITEM_CONTEXT_TOGGLE_WIDE_ID: usize = 6;
pub(super) const ITEM_CONTEXT_REMOVE_ID: usize = 7;
pub(super) const ITEM_CONTEXT_MOVE_ZONE_BASE_ID: usize = 100;

pub(super) fn item_context_menu_rows(has_move_targets: bool) -> popover::ContextMenuRows {
    let mut entries = popover::ContextMenuRows::new();
    entries.push(popover::ContextMenuRow::command(
        ITEM_CONTEXT_OPEN_ID,
        context_menu_text("打开", "Open"),
        IconKind::ExternalLink,
    ));
    entries.push(popover::ContextMenuRow::command(
        ITEM_CONTEXT_REVEAL_ID,
        context_menu_text("在资源管理器中显示", "Show in File Explorer"),
        IconKind::FolderOpen,
    ));
    entries.push(popover::ContextMenuRow::command(
        ITEM_CONTEXT_COPY_PATH_ID,
        context_menu_text("复制路径", "Copy path"),
        IconKind::Copy,
    ));

    entries.push(popover::ContextMenuRow::separator());
    entries.push(popover::ContextMenuRow::command(
        ITEM_CONTEXT_RENAME_FILE_ID,
        context_menu_text("重命名文件…", "Rename file…"),
        IconKind::Edit,
    ));
    entries.push(popover::ContextMenuRow::command(
        ITEM_CONTEXT_TOGGLE_WIDE_ID,
        context_menu_text("切换宽卡片", "Toggle wide card"),
        IconKind::Columns,
    ));
    if has_move_targets {
        entries.push(popover::ContextMenuRow::submenu(
            context_menu_text("移动到 Zone", "Move to Zone"),
            IconKind::ArrowRight,
        ));
    }
    entries.push(popover::ContextMenuRow::command(
        ITEM_CONTEXT_REMOVE_ID,
        context_menu_text("从当前区域移除", "Remove from this Zone"),
        IconKind::X,
    ));

    entries.push(popover::ContextMenuRow::separator());
    entries.push(popover::ContextMenuRow::danger(
        ITEM_CONTEXT_DELETE_FILE_ID,
        context_menu_text("移入回收站…", "Move to Recycle Bin…"),
        IconKind::Trash,
    ));
    entries
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ItemContextAction {
    Open,
    Reveal,
    CopyPath,
    RenameFile,
    DeleteFile,
    ToggleWide,
    MoveToZone(ZoneId),
    Remove,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum ItemContextDispatch {
    OpenPath { verb: &'static str, path: SmolStr },
    RevealPath(SmolStr),
    Command(Command),
}

#[cfg(test)]
pub(super) fn item_context_action_for_choice(
    choice: usize,
    move_targets: &[(usize, ZoneId, SmolStr)],
) -> Option<ItemContextAction> {
    item_context_action_for_choice_with(choice, |command_id| {
        move_targets
            .iter()
            .find(|(target_command_id, _, _)| *target_command_id == command_id)
            .map(|(_, zone_id, _)| *zone_id)
    })
}

pub(super) fn item_context_action_for_choice_with<F>(
    choice: usize,
    move_target_for_choice: F,
) -> Option<ItemContextAction>
where
    F: FnOnce(usize) -> Option<ZoneId>,
{
    match choice {
        ITEM_CONTEXT_OPEN_ID => Some(ItemContextAction::Open),
        ITEM_CONTEXT_REVEAL_ID => Some(ItemContextAction::Reveal),
        ITEM_CONTEXT_COPY_PATH_ID => Some(ItemContextAction::CopyPath),
        ITEM_CONTEXT_RENAME_FILE_ID => Some(ItemContextAction::RenameFile),
        ITEM_CONTEXT_DELETE_FILE_ID => Some(ItemContextAction::DeleteFile),
        ITEM_CONTEXT_TOGGLE_WIDE_ID => Some(ItemContextAction::ToggleWide),
        ITEM_CONTEXT_REMOVE_ID => Some(ItemContextAction::Remove),
        command_id if command_id >= ITEM_CONTEXT_MOVE_ZONE_BASE_ID => {
            move_target_for_choice(command_id).map(ItemContextAction::MoveToZone)
        }
        _ => None,
    }
}

pub(super) fn item_context_dispatch_for_action(
    zone_id: ZoneId,
    item_id: bentodesk_zone::ZoneItemId,
    path: &str,
    action: ItemContextAction,
) -> ItemContextDispatch {
    let item_id = bentodesk_app::ItemId(item_id.0);
    match action {
        ItemContextAction::Open => ItemContextDispatch::OpenPath {
            verb: "open",
            path: SmolStr::new(path),
        },
        ItemContextAction::Reveal => ItemContextDispatch::RevealPath(SmolStr::new(path)),
        ItemContextAction::CopyPath => {
            ItemContextDispatch::Command(Command::CopyItemPath(bentodesk_app::ItemPath::new(path)))
        }
        ItemContextAction::RenameFile => {
            ItemContextDispatch::Command(Command::OpenItemFileRename(zone_id, item_id))
        }
        ItemContextAction::DeleteFile => {
            ItemContextDispatch::Command(Command::DeleteItemFileToRecycleBin(zone_id, item_id))
        }
        ItemContextAction::ToggleWide => {
            ItemContextDispatch::Command(Command::ToggleItemWide(zone_id, item_id))
        }
        ItemContextAction::MoveToZone(target_zone_id) => {
            ItemContextDispatch::Command(Command::MoveItemToZone(zone_id, target_zone_id, item_id))
        }
        ItemContextAction::Remove => {
            ItemContextDispatch::Command(Command::RemoveItem(zone_id, item_id))
        }
    }
}

pub(super) fn apply_item_context_dispatch(root: &AppRoot, dispatch: ItemContextDispatch) {
    apply_item_context_dispatch_with(root, dispatch, shell_execute_path, reveal_path_in_explorer);
}

pub(super) fn apply_item_context_dispatch_with<OpenPath, RevealPath>(
    root: &AppRoot,
    dispatch: ItemContextDispatch,
    mut open_path: OpenPath,
    mut reveal_path: RevealPath,
) where
    OpenPath: FnMut(&str, &str, Option<&str>) -> Result<(), i32>,
    RevealPath: FnMut(&str) -> Result<(), i32>,
{
    match dispatch {
        ItemContextDispatch::OpenPath { verb, path } => {
            let result = open_path(verb, path.as_str(), None);
            set_shell_launch_status(root, "Open", path.as_str(), result);
        }
        ItemContextDispatch::RevealPath(path) => {
            let result = reveal_path(path.as_str());
            set_shell_launch_status(root, "Reveal", path.as_str(), result);
        }
        ItemContextDispatch::Command(command) => {
            root.dispatcher.push(command);
        }
    }
}

pub(super) fn copy_item_path_with<CopyText>(
    root: &AppRoot,
    path: &str,
    mut copy_text: CopyText,
) -> bool
where
    CopyText: FnMut(HWND, &str) -> bool,
{
    let owner = find_main_hwnd(root).unwrap_or(ptr::null_mut());
    let leaf = item_operation_leaf(path);
    if copy_text(owner, path) {
        tracing::info!(
            target: "bentodesk::items",
            path,
            "CopyItemPath: copied item path to clipboard"
        );
        log_static(format!("item-file: CopyItemPath copied path={path}\n").as_str());
        set_item_operation_status(
            root,
            localized_current(
                format!("已复制路径：{leaf}"),
                format!("Copied path: {leaf}"),
            ),
        );
        true
    } else {
        tracing::warn!(
            target: "bentodesk::items",
            path,
            "CopyItemPath failed: OpenClipboard/SetClipboardData refused"
        );
        log_static(format!("item-file: CopyItemPath failed path={path}\n").as_str());
        set_item_operation_status(
            root,
            localized_current(
                format!("复制路径失败：{leaf}"),
                format!("Copy path failed: {leaf}"),
            ),
        );
        false
    }
}

pub(super) fn apply_zone_context_action(
    root: &AppRoot,
    zone_id: ZoneId,
    action: ZoneContextAction,
) {
    match action {
        ZoneContextAction::Edit => {
            root.dispatcher.push(Command::OpenZoneEditor(zone_id));
        }
        ZoneContextAction::Search => {
            if let Some(hwnd) = find_main_hwnd(root) {
                open_inline_zone_search(root, zone_id, hwnd);
            }
        }
        ZoneContextAction::SmartSuggestor => {
            root.app.borrow().selected_zone.set(Some(zone_id));
            root.dispatcher.push(Command::ShowSuggestor);
        }
        ZoneContextAction::AutoOrganize => {
            root.dispatcher.push(Command::AutoArrangeZone(zone_id));
        }
        ZoneContextAction::PinMinibar => {
            root.dispatcher.push(Command::PinZoneAsMinibar(zone_id));
        }
        ZoneContextAction::OpenLiveFolderPicker => {
            root.dispatcher.push(Command::OpenLiveFolderPicker(zone_id));
        }
        ZoneContextAction::RefreshLiveFolder => {
            root.dispatcher.push(Command::RefreshLiveFolder(zone_id));
        }
        ZoneContextAction::UnbindLiveFolder => {
            root.dispatcher.push(Command::UnbindZoneFolder(zone_id));
        }
        ZoneContextAction::OpenStackTray => {
            root.dispatcher.push(Command::OpenStackTray(zone_id));
        }
        ZoneContextAction::Unstack => {
            root.dispatcher.push(Command::UnstackZone(zone_id));
        }
        ZoneContextAction::SaveSnapshot => {
            root.dispatcher.push(Command::SaveSnapshot {
                name: Some(snapshot_capture_name(root)),
            });
        }
        ZoneContextAction::OpenSnapshotPicker => {
            root.dispatcher.push(Command::OpenSnapshotPicker);
        }
        ZoneContextAction::OpenCapsules => {
            root.dispatcher.push(Command::OpenCapsulePicker);
        }
        ZoneContextAction::OpenBulkManager => {
            root.app.borrow().selected_zone.set(Some(zone_id));
            root.dispatcher.push(Command::OpenBulkManager);
        }
        ZoneContextAction::StackWith(target_zone_id) => {
            root.dispatcher
                .push(Command::StackZone(zone_id, target_zone_id));
        }
        ZoneContextAction::Delete => {
            root.dispatcher.push(Command::DeleteZone(zone_id));
        }
    }
}

pub(super) fn zone_context_action_for_choice(
    choice: usize,
    stack_targets: &[(usize, ZoneId)],
) -> Option<ZoneContextAction> {
    match choice {
        ZONE_CONTEXT_EDIT_ID => Some(ZoneContextAction::Edit),
        ZONE_CONTEXT_SEARCH_ID => Some(ZoneContextAction::Search),
        ZONE_CONTEXT_SMART_SUGGESTOR_ID => Some(ZoneContextAction::SmartSuggestor),
        ZONE_CONTEXT_AUTO_ORGANIZE_ID => Some(ZoneContextAction::AutoOrganize),
        ZONE_CONTEXT_PIN_MINIBAR_ID => Some(ZoneContextAction::PinMinibar),
        ZONE_CONTEXT_BIND_LIVE_FOLDER_ID => Some(ZoneContextAction::OpenLiveFolderPicker),
        ZONE_CONTEXT_REFRESH_LIVE_FOLDER_ID => Some(ZoneContextAction::RefreshLiveFolder),
        ZONE_CONTEXT_UNBIND_LIVE_FOLDER_ID => Some(ZoneContextAction::UnbindLiveFolder),
        ZONE_CONTEXT_OPEN_STACK_TRAY_ID => Some(ZoneContextAction::OpenStackTray),
        ZONE_CONTEXT_UNSTACK_ID => Some(ZoneContextAction::Unstack),
        ZONE_CONTEXT_SAVE_SNAPSHOT_ID => Some(ZoneContextAction::SaveSnapshot),
        ZONE_CONTEXT_OPEN_SNAPSHOT_PICKER_ID => Some(ZoneContextAction::OpenSnapshotPicker),
        ZONE_CONTEXT_CAPSULES_ID => Some(ZoneContextAction::OpenCapsules),
        ZONE_CONTEXT_BULK_MANAGER_ID => Some(ZoneContextAction::OpenBulkManager),
        choice if choice >= ZONE_CONTEXT_STACK_BASE_ID => stack_targets
            .iter()
            .find(|(command_id, _)| *command_id == choice)
            .map(|(_, target_zone_id)| ZoneContextAction::StackWith(*target_zone_id)),
        ZONE_CONTEXT_DELETE_ID => Some(ZoneContextAction::Delete),
        _ => None,
    }
}

pub(super) fn close_context_menu_surface(root: &AppRoot) {
    let changed = root
        .app
        .borrow()
        .active_context_menu
        .borrow_mut()
        .take()
        .is_some();
    root.zone_context_menu.borrow_mut().take();
    root.item_context_menu.borrow_mut().take();
    if let Some(hwnd) = find_main_hwnd(root) {
        unsafe {
            KillTimer(hwnd, CONTEXT_MENU_INPUT_TIMER_ID);
            if changed {
                ReleaseCapture();
            }
        };
        if changed {
            request_redraw(hwnd);
        }
    }
}

pub(super) fn resize_context_menu_for_submenu(root: &AppRoot, hwnd: HWND, open: bool) {
    {
        let app = root.app.borrow();
        let viewport = app.viewport;
        let mut active = app.active_context_menu.borrow_mut();
        let Some(session) = active.as_mut() else {
            return;
        };
        let open = open && !session.submenu_rows.is_empty();
        if session.submenu_open == open {
            return;
        }
        let previous = popover::context_menu_window_size(session);
        session.submenu_open = open;
        if !open
            && session
                .hovered
                .is_some_and(|hit| hit.column == popover::ContextMenuColumn::Submenu)
        {
            session.hovered = session
                .main_rows
                .iter()
                .position(|row| row.kind == popover::ContextMenuRowKind::Submenu)
                .map(|row| popover::ContextMenuHit {
                    column: popover::ContextMenuColumn::Main,
                    row,
                });
        }
        let next = popover::context_menu_window_size(session);
        if session.submenu_on_left {
            session.origin_x -= (next.width - previous.width).round() as i32;
        }
        session.origin_x = session
            .origin_x
            .clamp(0, (viewport.width - next.width).max(0.0).round() as i32);
        session.origin_y = session
            .origin_y
            .clamp(0, (viewport.height - next.height).max(0.0).round() as i32);
    }
    request_redraw(hwnd);
}
