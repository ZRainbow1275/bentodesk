//! Operations owned by one Zone and its item collection.

use super::*;

impl Zone {
    /// Construct a zone with a borrowed title.
    pub fn new(
        id: ZoneId,
        title: impl Into<Cow<'static, str>>,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    ) -> Self {
        Self {
            id,
            title: title.into(),
            icon: Cow::Borrowed(DEFAULT_ZONE_ICON),
            accent_color: None,
            visible: true,
            x,
            y,
            w,
            h,
            grid_columns: DEFAULT_ZONE_GRID_COLUMNS,
            capsule_size: Cow::Borrowed(DEFAULT_ZONE_CAPSULE_SIZE),
            capsule_shape: Cow::Borrowed(DEFAULT_ZONE_CAPSULE_SHAPE),
            locked: false,
            alias: None,
            display_mode: None,
            live_folder_path: None,
            stack_parent: None,
            stack_members: SmallVec::new(),
            items: SmallVec::new(),
        }
    }

    pub fn is_stack_anchor(&self) -> bool {
        !self.stack_members.is_empty()
    }

    pub fn is_stacked_child(&self) -> bool {
        self.stack_parent.is_some()
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn set_visible(&mut self, visible: bool) -> bool {
        if self.visible == visible {
            return false;
        }
        self.visible = visible;
        true
    }

    fn next_item_id(&self) -> Option<ZoneItemId> {
        self.items
            .iter()
            .map(|item| item.id.0)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .map(ZoneItemId)
    }

    pub fn add_item(
        &mut self,
        path: impl Into<Cow<'static, str>>,
        icon_hash: impl Into<Cow<'static, str>>,
    ) -> Option<ZoneItemId> {
        self.add_item_with_metadata(path, None, icon_hash, None, None)
    }

    pub fn add_item_with_metadata(
        &mut self,
        path: impl Into<Cow<'static, str>>,
        display_path: Option<&str>,
        icon_hash: impl Into<Cow<'static, str>>,
        original_path: Option<Cow<'static, str>>,
        hidden_path: Option<Cow<'static, str>>,
    ) -> Option<ZoneItemId> {
        let id = self.next_item_id()?;
        let columns = self.grid_columns.max(1) as i32;
        let col = self.items.len() as i32 % columns;
        let row = self.items.len() as i32 / columns;
        let mut item = ZoneItem::new(id, path, icon_hash, col, row);
        if let Some(display_path) = display_path {
            item.name = Cow::Owned(display_name_for_path(display_path));
        }
        item.original_path = original_path;
        item.hidden_path = hidden_path;
        self.items.push(item);
        Some(id)
    }

    pub fn item(&self, id: ZoneItemId) -> Option<&ZoneItem> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn remove_item(&mut self, id: ZoneItemId) -> bool {
        let Some(idx) = self.items.iter().position(|item| item.id == id) else {
            return false;
        };
        self.items.remove(idx);
        true
    }

    /// Alphabetically arrange the current zone's items and rebuild their
    /// grid coordinates. This is the zone-scoped counterpart of Tauri's
    /// `reorderItems(zone.id, sortedIds)` context-menu action; it must not
    /// invoke the process-wide Desktop auto-grouping command.
    pub fn auto_arrange_items(&mut self) -> bool {
        let previous_ids: Vec<ZoneItemId> = self.items.iter().map(|item| item.id).collect();
        self.items.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.id.0.cmp(&right.id.0))
        });
        let mut changed = previous_ids
            .iter()
            .copied()
            .ne(self.items.iter().map(|item| item.id));
        let columns = self.grid_columns.max(1) as i32;
        for (index, item) in self.items.iter_mut().enumerate() {
            let next_x = index as i32 % columns;
            let next_y = index as i32 / columns;
            changed |= item.x != next_x || item.y != next_y;
            item.x = next_x;
            item.y = next_y;
        }
        changed
    }

    pub fn update_item_file_metadata(
        &mut self,
        id: ZoneItemId,
        effective_path: impl Into<Cow<'static, str>>,
        display_path: Option<&str>,
        original_path: Option<Cow<'static, str>>,
        hidden_path: Option<Cow<'static, str>>,
    ) -> bool {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        let effective_path = effective_path.into();
        item.name = Cow::Owned(display_name_for_path(
            display_path.unwrap_or(effective_path.as_ref()),
        ));
        item.path = effective_path;
        item.original_path = original_path;
        item.hidden_path = hidden_path;
        item.file_missing = false;
        true
    }

    pub fn move_item(&mut self, id: ZoneItemId, x: i32, y: i32) -> bool {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        item.x = x.max(0);
        item.y = y.max(0);
        true
    }

    pub fn toggle_item_wide(&mut self, id: ZoneItemId) -> bool {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        item.is_wide = !item.is_wide;
        true
    }

    pub fn set_item_icon_hash(
        &mut self,
        path: &str,
        icon_hash: impl Into<Cow<'static, str>>,
    ) -> bool {
        let Some(item) = self
            .items
            .iter_mut()
            .find(|item| item.path.as_ref() == path)
        else {
            return false;
        };
        item.icon_hash = icon_hash.into();
        true
    }

    pub fn set_icon(&mut self, icon: impl Into<Cow<'static, str>>) {
        self.icon = icon.into();
    }

    pub fn set_accent_color(&mut self, accent_color: Option<Cow<'static, str>>) {
        self.accent_color = accent_color;
    }

    pub fn set_grid_columns(&mut self, grid_columns: u32) {
        self.grid_columns = grid_columns.max(1);
    }

    pub fn set_capsule_size(&mut self, capsule_size: impl Into<Cow<'static, str>>) {
        self.capsule_size = capsule_size.into();
    }

    pub fn set_capsule_shape(&mut self, capsule_shape: impl Into<Cow<'static, str>>) {
        self.capsule_shape = capsule_shape.into();
    }

    pub fn set_capsule(
        &mut self,
        capsule_size: impl Into<Cow<'static, str>>,
        capsule_shape: impl Into<Cow<'static, str>>,
    ) {
        self.capsule_size = capsule_size.into();
        self.capsule_shape = capsule_shape.into();
    }

    pub fn set_locked(&mut self, locked: bool) -> bool {
        if self.locked == locked {
            return false;
        }
        self.locked = locked;
        true
    }

    pub fn set_alias(&mut self, alias: Option<Cow<'static, str>>) -> bool {
        if self.alias == alias {
            return false;
        }
        self.alias = alias;
        true
    }

    /// Visible Zone title used by capsules, expanded headers, stack petals,
    /// stack trays, search, and management panels.
    #[inline]
    pub fn display_title(&self) -> &str {
        self.alias
            .as_deref()
            .filter(|alias| !alias.is_empty())
            .unwrap_or(self.title.as_ref())
    }

    pub fn set_display_mode(&mut self, display_mode: Option<Cow<'static, str>>) -> bool {
        if self.display_mode == display_mode {
            return false;
        }
        self.display_mode = display_mode;
        true
    }

    pub fn set_live_folder_path(&mut self, live_folder_path: Option<Cow<'static, str>>) -> bool {
        if self.live_folder_path == live_folder_path {
            return false;
        }
        self.live_folder_path = live_folder_path;
        true
    }

    pub fn replace_item_path(
        &mut self,
        old_path: &str,
        new_path: impl Into<Cow<'static, str>>,
    ) -> bool {
        let Some(item) = self
            .items
            .iter_mut()
            .find(|item| item.path.as_ref() == old_path)
        else {
            return false;
        };
        let new_path = new_path.into();
        item.name = Cow::Owned(display_name_for_path(new_path.as_ref()));
        item.path = new_path;
        item.file_missing = false;
        true
    }

    pub fn mark_item_missing(&mut self, path: &str, missing: bool) -> bool {
        let Some(item) = self
            .items
            .iter_mut()
            .find(|item| item.path.as_ref() == path)
        else {
            return false;
        };
        item.file_missing = missing;
        true
    }
}
