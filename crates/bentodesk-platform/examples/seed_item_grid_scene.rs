use std::borrow::Cow;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use bentodesk_platform::storage::{read_zones, write_zones_atomic};
use bentodesk_zone::{Zone, ZoneId, ZoneItemId, ZoneList};

const ITEM_ROOT_ENV: &str = "BENTODESK_ITEM_GRID_ITEM_ROOT";
const URL_FILE_ENV: &str = "BENTODESK_ITEM_GRID_URL_FILE";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let zones_path = zones_path_from_args(env::args().skip(1))?;
    if let Some(parent) = zones_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let item_root = env::var_os(ITEM_ROOT_ENV).map(PathBuf::from);
    let url_file = env::var_os(URL_FILE_ENV).map(PathBuf::from);
    if let Some(root) = item_root.as_deref() {
        fs::create_dir_all(root)?;
        write_item_grid_files(root)?;
    }
    if let Some(path) = url_file.as_deref()
        && !path.is_file()
    {
        return Err(format!(
            "{URL_FILE_ENV} does not point to a file: {}",
            path.display()
        )
        .into());
    }

    let zones = item_grid_scene(item_root.as_deref(), url_file.as_deref());
    write_zones_atomic(&zones_path, &zones)?;
    let decoded = read_zones(&zones_path)?;
    validate_scene(&decoded, if url_file.is_some() { 3 } else { 2 })?;

    println!(
        "seeded item-grid proof scene at {} with {} zone(s)",
        zones_path.display(),
        decoded.len()
    );
    Ok(())
}

fn item_grid_scene(item_root: Option<&Path>, url_file: Option<&Path>) -> ZoneList {
    let mut zones = ZoneList::new();
    let mut zone = Zone::new(
        ZoneId(1),
        Cow::Borrowed("Item Grid Proof"),
        64,
        72,
        360,
        240,
    );
    zone.set_icon(Cow::Borrowed("file"));
    zone.set_accent_color(Some(Cow::Borrowed("#3b82f6")));
    zone.set_grid_columns(5);
    zone.set_capsule_size(Cow::Borrowed("medium"));
    zone.set_capsule_shape(Cow::Borrowed("pill"));
    zone.set_display_mode(Some(Cow::Borrowed("always")));

    if let Some(first_id) = zone.add_item(
        Cow::Owned(item_path(item_root, "grid-alpha.txt")),
        Cow::Borrowed("builtin:file"),
    ) {
        position_item(&mut zone, first_id, 0, 0);
    }
    if let Some(second_id) = zone.add_item(
        Cow::Owned(item_path(item_root, "grid-beta.txt")),
        Cow::Borrowed("builtin:terminal"),
    ) {
        position_item(&mut zone, second_id, 1, 0);
    }
    if let Some(url_file) = url_file
        && let Some(url_id) = zone.add_item(
            Cow::Owned(url_file.to_string_lossy().into_owned()),
            Cow::Borrowed(""),
        )
    {
        // An empty hash deliberately routes the real .url through startup icon
        // rehydration. The production extractor must parse IconFile/IconIndex,
        // cache the ICO resource and update this item before runtime capture.
        position_item(&mut zone, url_id, 2, 0);
    }

    zones.add(zone);
    zones
}

fn item_path(item_root: Option<&Path>, name: &str) -> String {
    item_root
        .map(|root| root.join(name).to_string_lossy().into_owned())
        .unwrap_or_else(|| format!(r"C:\Users\Public\Desktop\BentoDesk Item Grid Proof\{name}"))
}

fn write_item_grid_files(root: &Path) -> Result<(), std::io::Error> {
    fs::write(
        root.join("grid-alpha.txt"),
        b"BentoDesk item-grid proof alpha\n",
    )?;
    fs::write(
        root.join("grid-beta.txt"),
        b"BentoDesk item-grid proof beta\n",
    )?;
    Ok(())
}

fn position_item(zone: &mut Zone, item_id: ZoneItemId, x: i32, y: i32) {
    let _ = zone.move_item(item_id, x, y);
}

fn validate_scene(zones: &ZoneList, expected_items: usize) -> Result<(), String> {
    if zones.len() != 1 {
        return Err(format!("expected 1 zone, got {}", zones.len()));
    }
    let Some(zone) = zones.get(ZoneId(1)) else {
        return Err("expected zone 1".to_owned());
    };
    if zone.items.len() != expected_items {
        return Err(format!(
            "expected {expected_items} items, got {}",
            zone.items.len()
        ));
    }
    if zone.items[0].is_wide || zone.items[1].is_wide {
        return Err("proof items must start as standard cards".to_owned());
    }
    if (zone.items[0].x, zone.items[0].y) != (0, 0) {
        return Err("first item should start at grid (0,0)".to_owned());
    }
    if (zone.items[1].x, zone.items[1].y) != (1, 0) {
        return Err("second item should start at grid (1,0)".to_owned());
    }
    Ok(())
}

fn zones_path_from_args<I>(mut args: I) -> Result<PathBuf, String>
where
    I: Iterator<Item = String>,
{
    let Some(first) = args.next() else {
        return Err(
            "usage: cargo run -p bentodesk-platform --example seed_item_grid_scene -- <state-dir-or-zones.bin>"
                .to_owned(),
        );
    };
    if args.next().is_some() {
        return Err("expected exactly one output path".to_owned());
    }
    let path = PathBuf::from(first);
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("zones.bin"))
    {
        Ok(path)
    } else {
        Ok(path.join("zones.bin"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_grid_scene_has_two_standard_items() {
        let root = Path::new(r"C:\Temp\bento-item-grid-proof");
        let zones = item_grid_scene(Some(root), None);
        validate_scene(&zones, 2).expect("valid scene");
        let zone = zones.get(ZoneId(1)).expect("zone 1");
        assert_eq!(zone.title.as_ref(), "Item Grid Proof");
        assert_eq!(zone.grid_columns, 5);
        assert_eq!(
            zone.items[1].path.as_ref(),
            r"C:\Temp\bento-item-grid-proof\grid-beta.txt"
        );
    }

    #[test]
    fn optional_url_item_is_seeded_for_real_icon_rehydration() {
        let root = Path::new(r"C:\Temp\bento-item-grid-proof");
        let shortcut = root.join("Super Animal Royale.url");
        let zones = item_grid_scene(Some(root), Some(&shortcut));
        validate_scene(&zones, 3).expect("valid scene");
        let zone = zones.get(ZoneId(1)).expect("zone 1");
        assert_eq!(zone.items[2].path.as_ref(), shortcut.to_string_lossy());
        assert!(zone.items[2].icon_hash.is_empty());
        assert_eq!((zone.items[2].x, zone.items[2].y), (2, 0));
    }

    #[test]
    fn output_path_accepts_directory_or_zones_bin() {
        assert_eq!(
            zones_path_from_args([String::from(r"C:\Temp\bento-state")].into_iter()).expect("dir"),
            Path::new(r"C:\Temp\bento-state").join("zones.bin")
        );
        assert_eq!(
            zones_path_from_args([String::from(r"C:\Temp\bento-state\zones.bin")].into_iter())
                .expect("file"),
            PathBuf::from(r"C:\Temp\bento-state\zones.bin")
        );
    }
}
