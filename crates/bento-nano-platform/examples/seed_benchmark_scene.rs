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
const CAPSULE_VARIANTS_ENV: &str = "BENTODESK_NANO_BENCHMARK_CAPSULE_VARIANTS";
const REFERENCE_0602_ENV: &str = "BENTODESK_NANO_BENCHMARK_REFERENCE_0602";
const REFERENCE_0602_BROWSER_ITEM_COUNT_ENV: &str =
    "BENTODESK_NANO_BENCHMARK_REFERENCE_0602_BROWSER_ITEM_COUNT";
const CAPSULE_VARIANTS: [(&str, &str); 5] = [
    ("small", "pill"),
    ("medium", "rounded"),
    ("large", "minimal"),
    ("large", "pill"),
    ("small", "circle"),
];
const REFERENCE_0602_ZONES: [ReferenceZoneSeed; 11] = [
    ReferenceZoneSeed::new(
        "浏览器",
        6,
        384,
        320,
        220,
        "copy",
        "#3b82f6",
        4,
        "large",
        "pill",
    ),
    ReferenceZoneSeed::new(
        "新建区域",
        644,
        604,
        320,
        220,
        "folder",
        "#3b82f6",
        0,
        "medium",
        "pill",
    ),
    ReferenceZoneSeed::new(
        "ai", 639, 343, 320, 220, "code", "#3b82f6", 8, "large", "pill",
    ),
    ReferenceZoneSeed::new(
        "新建区域 3",
        977,
        68,
        320,
        220,
        "folder",
        "#3b82f6",
        0,
        "medium",
        "pill",
    ),
    ReferenceZoneSeed::new(
        "学习", 1440, 95, 320, 220, "bookmark", "#3b82f6", 9, "medium", "pill",
    ),
    ReferenceZoneSeed::new(
        "文件", 1440, 171, 320, 220, "file", "#3b82f6", 5, "medium", "pill",
    ),
    ReferenceZoneSeed::new(
        "网络", 1440, 229, 320, 220, "globe", "#3b82f6", 2, "medium", "pill",
    ),
    ReferenceZoneSeed::new(
        "工具", 1442, 348, 320, 220, "grid", "#3b82f6", 1, "medium", "pill",
    ),
    ReferenceZoneSeed::new(
        "Compiler", 1092, 443, 320, 220, "code", "#22c55e", 4, "large", "pill",
    ),
    ReferenceZoneSeed::new(
        "net及逆向",
        894,
        648,
        320,
        220,
        "globe",
        "#3b82f6",
        4,
        "medium",
        "pill",
    ),
    ReferenceZoneSeed::new(
        "游戏", 1440, 648, 320, 220, "gamepad", "#3b82f6", 2, "medium", "pill",
    ),
];
const REFERENCE_0602_ITEM_NAMES: [&[&str]; 11] = [
    &["115浏览器", "Roxy Browser", "Tor Browser", "Tor Browser 2"],
    &[],
    &[],
    &[],
    &[],
    &[],
    &[],
    &[],
    &[],
    &[],
    &[],
];

#[derive(Clone, Copy)]
struct ReferenceZoneSeed {
    title: &'static str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    icon: &'static str,
    accent: &'static str,
    item_count: u64,
    capsule_size: &'static str,
    capsule_shape: &'static str,
}

impl ReferenceZoneSeed {
    #[allow(clippy::too_many_arguments)]
    const fn new(
        title: &'static str,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        icon: &'static str,
        accent: &'static str,
        item_count: u64,
        capsule_size: &'static str,
        capsule_shape: &'static str,
    ) -> Self {
        Self {
            title,
            x,
            y,
            w,
            h,
            icon,
            accent,
            item_count,
            capsule_size,
            capsule_shape,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let zones_path = zones_path_from_args(env::args().skip(1))?;
    if let Some(parent) = zones_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let item_root = env::var_os(ITEM_ROOT_ENV).map(PathBuf::from);
    if let Some(root) = item_root.as_deref() {
        fs::create_dir_all(root)?;
    }

    let capsule_variants = env_flag(CAPSULE_VARIANTS_ENV);
    let reference_0602 = env_flag(REFERENCE_0602_ENV);
    let reference_0602_browser_item_count = if reference_0602 {
        reference_0602_browser_item_count_override()?
    } else {
        None
    };
    let zones = if reference_0602 {
        reference_0602_scene_with_options(item_root.as_deref(), reference_0602_browser_item_count)
    } else {
        benchmark_scene_with_options(item_root.as_deref(), capsule_variants)
    };
    if let Some(root) = item_root.as_deref() {
        if reference_0602 {
            write_reference_0602_item_files(root, reference_0602_browser_item_count)?;
        } else {
            write_benchmark_item_files(root)?;
        }
    }
    write_zones_atomic(&zones_path, &zones)?;
    let decoded = read_zones(&zones_path)?;
    if reference_0602 {
        validate_scene_counts(
            &decoded,
            REFERENCE_0602_ZONES.len(),
            reference_0602_item_count(reference_0602_browser_item_count) as usize,
        )?;
    } else {
        validate_scene(&decoded)?;
    }

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

#[cfg(test)]
fn benchmark_scene_with_item_root(item_root: Option<&Path>) -> ZoneList {
    benchmark_scene_with_options(item_root, false)
}

fn benchmark_scene_with_options(item_root: Option<&Path>, capsule_variants: bool) -> ZoneList {
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
        if capsule_variants {
            let (size, shape) = CAPSULE_VARIANTS[zone_index as usize % CAPSULE_VARIANTS.len()];
            zone.set_capsule(Cow::Borrowed(size), Cow::Borrowed(shape));
        } else {
            zone.set_capsule(Cow::Borrowed("medium"), Cow::Borrowed("pill"));
        }
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

fn reference_0602_scene_with_options(
    item_root: Option<&Path>,
    browser_item_count_override: Option<u64>,
) -> ZoneList {
    let mut zones = ZoneList::new();
    for (zone_index, spec) in REFERENCE_0602_ZONES.iter().enumerate() {
        let mut zone = Zone::new(
            ZoneId(zone_index as u64 + 1),
            Cow::Borrowed(spec.title),
            spec.x,
            spec.y,
            spec.w,
            spec.h,
        );
        zone.set_icon(Cow::Borrowed(spec.icon));
        zone.set_accent_color(reference_0602_zone_accent(zone_index, spec));
        zone.set_grid_columns(5);
        zone.set_capsule(
            Cow::Borrowed(spec.capsule_size),
            Cow::Borrowed(spec.capsule_shape),
        );
        zone.set_display_mode(None);

        let item_count =
            reference_0602_zone_item_count(zone_index, spec, browser_item_count_override);
        for item_index in 0..item_count {
            let Some(item_id) = zone.add_item(
                Cow::Owned(reference_0602_item_path(
                    item_root,
                    zone_index as u64,
                    item_index,
                )),
                Cow::Owned(format!(
                    "builtin:{}",
                    ICONS[(zone_index as u64 + item_index) as usize % ICONS.len()]
                )),
            ) else {
                continue;
            };
            position_reference_0602_item(&mut zone, zone_index, item_id, item_index);
        }
        zones.add(zone);
    }
    zones
}

fn env_flag(name: &str) -> bool {
    env::var(name).is_ok_and(|value| {
        let value = value.trim();
        value.eq_ignore_ascii_case("1")
            || value.eq_ignore_ascii_case("true")
            || value.eq_ignore_ascii_case("yes")
            || value.eq_ignore_ascii_case("on")
    })
}

fn reference_0602_browser_item_count_override() -> Result<Option<u64>, Box<dyn std::error::Error>> {
    let Some(raw) = env::var_os(REFERENCE_0602_BROWSER_ITEM_COUNT_ENV) else {
        return Ok(None);
    };
    let raw = raw.to_string_lossy();
    let value = raw.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let parsed = value.parse::<u64>().map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{REFERENCE_0602_BROWSER_ITEM_COUNT_ENV} must be an integer: {error}"),
        )
    })?;
    let max_browser_items = REFERENCE_0602_ZONES[0].item_count;
    if parsed > max_browser_items {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "{REFERENCE_0602_BROWSER_ITEM_COUNT_ENV}={parsed} exceeds browser reference item cap {max_browser_items}"
            ),
        )
        .into());
    }
    Ok(Some(parsed))
}

fn reference_0602_zone_accent(
    zone_index: usize,
    spec: &ReferenceZoneSeed,
) -> Option<Cow<'static, str>> {
    // N164 (2026-07-05): the 0602 Browser expanded-header reference crop has
    // no blue count-badge occupancy in the header-controls probe. Keep the
    // historical accent table for the other zones, but let Browser use the
    // neutral badge fallback so same-state expanded-panel proof is not polluted
    // by a reference-state-only accent mismatch.
    if zone_index == 0 {
        None
    } else {
        Some(Cow::Borrowed(spec.accent))
    }
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

fn reference_0602_item_path(item_root: Option<&Path>, zone_index: u64, item_index: u64) -> String {
    if let Some(root) = item_root {
        return reference_0602_item_file_path(root, zone_index, item_index)
            .to_string_lossy()
            .into_owned();
    }
    legacy_reference_0602_item_path(zone_index, item_index)
}

fn legacy_reference_0602_item_path(zone_index: u64, item_index: u64) -> String {
    format!(
        r"C:\Users\Public\Desktop\BentoDesk 0602 Reference\zone-{zone:02}\{name}",
        zone = zone_index + 1,
        name = reference_0602_item_file_name(zone_index, item_index)
    )
}

fn reference_0602_item_file_path(root: &Path, zone_index: u64, item_index: u64) -> PathBuf {
    root.join(format!("zone-{zone:02}", zone = zone_index + 1))
        .join(reference_0602_item_file_name(zone_index, item_index))
}

fn reference_0602_item_file_name(zone_index: u64, item_index: u64) -> String {
    REFERENCE_0602_ITEM_NAMES
        .get(zone_index as usize)
        .and_then(|names| names.get(item_index as usize))
        .map_or_else(
            || format!("item-{item:02}.txt", item = item_index + 1),
            |name| (*name).to_owned(),
        )
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
                    "BentoDesk benchmark item zone={} item={}\n",
                    zone_index + 1,
                    item_index + 1
                ),
            )?;
        }
    }
    Ok(())
}

fn write_reference_0602_item_files(
    root: &Path,
    browser_item_count_override: Option<u64>,
) -> Result<(), std::io::Error> {
    for (zone_index, spec) in REFERENCE_0602_ZONES.iter().enumerate() {
        let zone_dir = root.join(format!("zone-{zone:02}", zone = zone_index + 1));
        fs::create_dir_all(&zone_dir)?;
        let item_count =
            reference_0602_zone_item_count(zone_index, spec, browser_item_count_override);
        for item_index in 0..item_count {
            let path = reference_0602_item_file_path(root, zone_index as u64, item_index);
            fs::write(
                path,
                format!(
                    "BentoDesk 0602 reference-aligned item zone={} item={}\n",
                    zone_index + 1,
                    item_index + 1
                ),
            )?;
        }
    }
    Ok(())
}

fn reference_0602_zone_item_count(
    zone_index: usize,
    spec: &ReferenceZoneSeed,
    browser_item_count_override: Option<u64>,
) -> u64 {
    if zone_index == 0 {
        browser_item_count_override.unwrap_or(spec.item_count)
    } else {
        spec.item_count
    }
}

fn position_item(zone: &mut Zone, item_id: ZoneItemId, item_index: u64) {
    let x = (item_index % 5) as i32;
    let y = (item_index / 5) as i32;
    let _ = zone.move_item(item_id, x, y);
    if item_index % 4 == 0 {
        let _ = zone.toggle_item_wide(item_id);
    }
}

fn position_reference_0602_item(
    zone: &mut Zone,
    zone_index: usize,
    item_id: ZoneItemId,
    item_index: u64,
) {
    let x = (item_index % 5) as i32;
    let y = (item_index / 5) as i32;
    let _ = zone.move_item(item_id, x, y);
    if zone_index != 0 && item_index % 4 == 0 {
        let _ = zone.toggle_item_wide(item_id);
    }
}

fn validate_scene(zones: &ZoneList) -> Result<(), String> {
    validate_scene_counts(
        zones,
        ZONE_COUNT as usize,
        (ZONE_COUNT * ITEMS_PER_ZONE) as usize,
    )
}

fn validate_scene_counts(
    zones: &ZoneList,
    expected_zones: usize,
    expected_items: usize,
) -> Result<(), String> {
    if zones.len() != expected_zones {
        return Err(format!(
            "expected {expected_zones} zones, got {}",
            zones.len()
        ));
    }
    let total_items = zones.iter().map(|zone| zone.items.len()).sum::<usize>();
    if total_items != expected_items {
        return Err(format!(
            "expected {expected_items} items, got {total_items}"
        ));
    }
    Ok(())
}

fn reference_0602_item_count(browser_item_count_override: Option<u64>) -> u64 {
    REFERENCE_0602_ZONES
        .iter()
        .enumerate()
        .map(|(zone_index, zone)| {
            reference_0602_zone_item_count(zone_index, zone, browser_item_count_override)
        })
        .sum()
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
    fn benchmark_scene_can_seed_capsule_variants_for_visual_proof() {
        let zones = benchmark_scene_with_options(None, true);
        validate_scene(&zones).expect("valid benchmark scene");
        let first = zones.get(ZoneId(1)).expect("first zone");
        let third = zones.get(ZoneId(3)).expect("third zone");
        let fourth = zones.get(ZoneId(4)).expect("fourth zone");
        let fifth = zones.get(ZoneId(5)).expect("fifth zone");

        assert_eq!(first.capsule_size.as_ref(), "small");
        assert_eq!(first.capsule_shape.as_ref(), "pill");
        assert_eq!(third.capsule_size.as_ref(), "large");
        assert_eq!(third.capsule_shape.as_ref(), "minimal");
        assert_eq!(fourth.capsule_size.as_ref(), "large");
        assert_eq!(fourth.capsule_shape.as_ref(), "pill");
        assert_eq!(fifth.capsule_size.as_ref(), "small");
        assert_eq!(fifth.capsule_shape.as_ref(), "circle");
    }

    #[test]
    fn reference_0602_scene_matches_video_capsule_distribution() {
        let zones = reference_0602_scene_with_options(None, None);
        validate_scene_counts(
            &zones,
            REFERENCE_0602_ZONES.len(),
            reference_0602_item_count(None) as usize,
        )
        .expect("valid reference-aligned scene");
        let browser = zones.get(ZoneId(1)).expect("browser zone");
        let bottom_new_zone = zones.get(ZoneId(2)).expect("bottom new zone");
        let ai = zones.get(ZoneId(3)).expect("ai zone");
        let top_new_zone = zones.get(ZoneId(4)).expect("top new zone");
        let learning = zones.get(ZoneId(5)).expect("learning zone");
        let files = zones.get(ZoneId(6)).expect("files zone");
        let network = zones.get(ZoneId(7)).expect("network zone");
        let tools = zones.get(ZoneId(8)).expect("tools zone");
        let compiler = zones.get(ZoneId(9)).expect("compiler zone");
        let net_reverse = zones.get(ZoneId(10)).expect("net reverse zone");
        let games = zones.get(ZoneId(11)).expect("games zone");

        assert_eq!(browser.title.as_ref(), "浏览器");
        assert_eq!(browser.icon.as_ref(), "copy");
        assert_eq!(browser.capsule_size.as_ref(), "large");
        assert_eq!(browser.capsule_shape.as_ref(), "pill");
        assert_eq!((browser.x, browser.y), (6, 384));
        assert_eq!(browser.items.len(), 4);
        assert_eq!(
            browser.accent_color.as_deref(),
            None,
            "0602 Browser reference header uses the neutral badge fallback, not an injected blue zone accent"
        );
        assert!(
            browser.items.iter().all(|item| !item.is_wide),
            "0602 Browser reference items render as standard icon-over-label cards, not wide row cards"
        );
        assert!(browser.items[0].path.ends_with(r"zone-01\115浏览器"));
        assert!(browser.items[1].path.ends_with(r"zone-01\Roxy Browser"));
        assert!(browser.items[2].path.ends_with(r"zone-01\Tor Browser"));
        assert!(browser.items[3].path.ends_with(r"zone-01\Tor Browser 2"));
        assert_eq!(bottom_new_zone.title.as_ref(), "新建区域");
        assert_eq!((bottom_new_zone.x, bottom_new_zone.y), (644, 604));
        assert_eq!(bottom_new_zone.capsule_size.as_ref(), "medium");
        assert_eq!(bottom_new_zone.capsule_shape.as_ref(), "pill");
        assert_eq!(bottom_new_zone.items.len(), 0);
        assert_eq!(
            bottom_new_zone.accent_color.as_deref(),
            Some("#3b82f6"),
            "0602 bottom New Zone reference badge uses the default blue accent, not the slate palette swatch"
        );
        assert_eq!(ai.title.as_ref(), "ai");
        assert_eq!(ai.icon.as_ref(), "code");
        assert_eq!((ai.x, ai.y), (639, 343));
        assert_eq!(ai.capsule_size.as_ref(), "large");
        assert_eq!(ai.capsule_shape.as_ref(), "pill");
        assert_eq!(ai.items.len(), 8);
        assert_eq!(top_new_zone.title.as_ref(), "新建区域 3");
        assert_eq!((top_new_zone.x, top_new_zone.y), (977, 68));
        assert_eq!(learning.title.as_ref(), "学习");
        assert_eq!(learning.icon.as_ref(), "bookmark");
        assert_eq!((learning.x, learning.y), (1440, 95));
        assert_eq!(learning.items.len(), 9);
        assert_eq!(files.title.as_ref(), "文件");
        assert_eq!(files.icon.as_ref(), "file");
        assert_eq!((files.x, files.y), (1440, 171));
        assert_eq!(network.title.as_ref(), "网络");
        assert_eq!(network.icon.as_ref(), "globe");
        assert_eq!((network.x, network.y), (1440, 229));
        assert_eq!(tools.title.as_ref(), "工具");
        assert_eq!(tools.icon.as_ref(), "grid");
        assert_eq!((tools.x, tools.y), (1442, 348));
        assert_eq!(compiler.title.as_ref(), "Compiler");
        assert_eq!(compiler.icon.as_ref(), "code");
        assert_eq!(compiler.accent_color.as_deref(), Some("#22c55e"));
        assert_eq!((compiler.x, compiler.y), (1092, 443));
        assert_eq!(compiler.capsule_size.as_ref(), "large");
        assert_eq!(compiler.capsule_shape.as_ref(), "pill");
        assert_eq!(compiler.items.len(), 4);
        assert_eq!(net_reverse.title.as_ref(), "net及逆向");
        assert_eq!(net_reverse.icon.as_ref(), "globe");
        assert_eq!((net_reverse.x, net_reverse.y), (894, 648));
        assert_eq!(games.title.as_ref(), "游戏");
        assert_eq!(games.icon.as_ref(), "gamepad");
        assert_eq!((games.x, games.y), (1440, 648));
    }

    #[test]
    fn reference_0602_scene_can_omit_browser_lower_row_for_state_alignment() {
        let zones = reference_0602_scene_with_options(None, Some(3));
        validate_scene_counts(
            &zones,
            REFERENCE_0602_ZONES.len(),
            reference_0602_item_count(Some(3)) as usize,
        )
        .expect("valid state-aligned reference scene");
        let browser = zones.get(ZoneId(1)).expect("browser zone");
        assert_eq!(browser.items.len(), 3);
        assert_eq!(
            browser.accent_color.as_deref(),
            None,
            "0602 Browser 3-item seed keeps the header badge neutral for same-state alignment"
        );
        assert!(
            browser.items.iter().all(|item| !item.is_wide),
            "0602 Browser 3-item seed keeps all visible Browser cards standard"
        );
        assert!(browser.items[1].path.ends_with(r"zone-01\Roxy Browser"));
        assert!(browser.items[2].path.ends_with(r"zone-01\Tor Browser"));
        assert!(
            browser
                .items
                .iter()
                .all(|item| !item.path.as_ref().ends_with(r"zone-01\Tor Browser 2"))
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
