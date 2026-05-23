use std::borrow::Cow;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use bento_nano_platform::storage::{read_zones, write_zones_atomic};
use bento_nano_zone::{Zone, ZoneId, ZoneItemId, ZoneList};

const ZONE_COUNT: u64 = 5;
const ITEMS_PER_ZONE: u64 = 10;
const ICONS: [&str; 5] = ["folder", "file", "terminal", "settings", "search"];
const ACCENTS: [&str; 5] = ["#3b82f6", "#22c55e", "#f59e0b", "#ec4899", "#8b5cf6"];
const ITEM_ROOT_ENV: &str = "BENTODESK_NANO_BENCHMARK_ITEM_ROOT";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let zones_path = zones_path_from_args(env::args().skip(1))?;
    if let Some(parent) = zones_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let item_root = env::var_os(ITEM_ROOT_ENV).map(PathBuf::from);
    if let Some(root) = item_root.as_deref() {
        fs::create_dir_all(root)?;
    }

    let zones = benchmark_scene_with_item_root(item_root.as_deref());
    if let Some(root) = item_root.as_deref() {
        write_benchmark_item_files(root)?;
    }
    write_zones_atomic(&zones_path, &zones)?;
    let decoded = read_zones(&zones_path)?;
    validate_scene(&decoded)?;

    println!(
        "seeded {} zones / {} items at {}",
        decoded.len(),
        decoded.iter().map(|zone| zone.items.len()).sum::<usize>(),
        zones_path.display()
    );
    Ok(())
}

fn zones_path_from_args<I>(mut args: I) -> Result<PathBuf, String>
where
    I: Iterator<Item = String>,
{
    let Some(first) = args.next() else {
        return Err(
            "usage: cargo run -p bento-nano-platform --example seed_benchmark_scene -- <state-dir-or-zones.bin>"
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

fn benchmark_scene_with_item_root(item_root: Option<&Path>) -> ZoneList {
    let mut zones = ZoneList::new();
    for zone_index in 0..ZONE_COUNT {
        let mut zone = Zone::new(
            ZoneId(zone_index + 1),
            Cow::Owned(format!("Benchmark Zone {}", zone_index + 1)),
            64 + (zone_index as i32 % 3) * 360,
            72 + (zone_index as i32 / 3) * 260,
            320,
            220,
        );
        zone.set_icon(Cow::Borrowed(ICONS[zone_index as usize % ICONS.len()]));
        zone.set_accent_color(Some(Cow::Borrowed(
            ACCENTS[zone_index as usize % ACCENTS.len()],
        )));
        zone.set_grid_columns(5);
        zone.set_capsule_size(Cow::Borrowed("medium"));
        zone.set_capsule_shape(Cow::Borrowed("pill"));
        // RC-1 (05-20 visual parity) — leave `display_mode = None` so the
        // app-level default (`Hover`, collapsed pill at rest) drives the
        // Tauri-faithful "5 collapsed capsules" main scene. Setting `"always"`
        // here forced every benchmark zone into expanded-grid form, which
        // bypassed the Wave C pill render path.
        zone.set_display_mode(None);

        for item_index in 0..ITEMS_PER_ZONE {
            let Some(item_id) = zone.add_item(
                Cow::Owned(benchmark_item_path(item_root, zone_index, item_index)),
                Cow::Owned(format!(
                    "builtin:{}",
                    ICONS[(zone_index + item_index) as usize % ICONS.len()]
                )),
            ) else {
                continue;
            };
            position_item(&mut zone, item_id, item_index);
        }
        zones.add(zone);
    }

    if zones.stack(ZoneId(1), ZoneId(2)) {
        let _ = zones.stack(ZoneId(1), ZoneId(3));
    }
    zones
}

fn benchmark_item_path(item_root: Option<&Path>, zone_index: u64, item_index: u64) -> String {
    if let Some(root) = item_root {
        return benchmark_item_file_path(root, zone_index, item_index)
            .to_string_lossy()
            .into_owned();
    }
    legacy_benchmark_item_path(zone_index, item_index)
}

fn legacy_benchmark_item_path(zone_index: u64, item_index: u64) -> String {
    format!(
        r"C:\Users\Public\Desktop\BentoDesk Benchmark\zone-{zone:02}\item-{item:02}.lnk",
        zone = zone_index + 1,
        item = item_index + 1
    )
}

fn benchmark_item_file_path(root: &Path, zone_index: u64, item_index: u64) -> PathBuf {
    root.join(format!("zone-{zone:02}", zone = zone_index + 1))
        .join(format!("item-{item:02}.txt", item = item_index + 1))
}

fn write_benchmark_item_files(root: &Path) -> Result<(), std::io::Error> {
    for zone_index in 0..ZONE_COUNT {
        let zone_dir = root.join(format!("zone-{zone:02}", zone = zone_index + 1));
        fs::create_dir_all(&zone_dir)?;
        for item_index in 0..ITEMS_PER_ZONE {
            let path = benchmark_item_file_path(root, zone_index, item_index);
            fs::write(
                path,
                format!(
                    "BentoDesk Nano benchmark item zone={} item={}\n",
                    zone_index + 1,
                    item_index + 1
                ),
            )?;
        }
    }
    Ok(())
}

fn position_item(zone: &mut Zone, item_id: ZoneItemId, item_index: u64) {
    let x = (item_index % 5) as i32;
    let y = (item_index / 5) as i32;
    let _ = zone.move_item(item_id, x, y);
    if item_index % 4 == 0 {
        let _ = zone.toggle_item_wide(item_id);
    }
}

fn validate_scene(zones: &ZoneList) -> Result<(), String> {
    if zones.len() != ZONE_COUNT as usize {
        return Err(format!("expected {ZONE_COUNT} zones, got {}", zones.len()));
    }
    let total_items = zones.iter().map(|zone| zone.items.len()).sum::<usize>();
    let expected_items = (ZONE_COUNT * ITEMS_PER_ZONE) as usize;
    if total_items != expected_items {
        return Err(format!(
            "expected {expected_items} items, got {total_items}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn benchmark_scene_has_required_zone_and_item_counts() {
        let zones = benchmark_scene_with_item_root(None);
        validate_scene(&zones).expect("valid benchmark scene");
        let first = zones.get(ZoneId(1)).expect("first zone");
        assert_eq!(first.stack_members.as_slice(), &[ZoneId(2), ZoneId(3)]);
        // RC-1: benchmark scene no longer hard-codes "always"; collapsed pill
        // is the default visual form.
        assert_eq!(first.display_mode, None);
    }

    #[test]
    fn benchmark_scene_can_point_to_real_item_root() {
        let root = Path::new(r"C:\Temp\bento-proof-items");
        let zones = benchmark_scene_with_item_root(Some(root));
        validate_scene(&zones).expect("valid benchmark scene");
        let first = zones.get(ZoneId(1)).expect("first zone");
        assert_eq!(
            first.items[1].path.as_ref(),
            r"C:\Temp\bento-proof-items\zone-01\item-02.txt"
        );
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
