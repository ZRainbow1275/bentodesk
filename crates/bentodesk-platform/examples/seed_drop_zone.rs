use std::borrow::Cow;
use std::env;
use std::fs;
use std::path::PathBuf;

use bentodesk_platform::storage::{read_zones, write_zones_atomic};
use bentodesk_zone::{Zone, ZoneId, ZoneList};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let zones_path = zones_path_from_args(env::args().skip(1))?;
    if let Some(parent) = zones_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let zones = drop_zone_scene();
    write_zones_atomic(&zones_path, &zones)?;
    let decoded = read_zones(&zones_path)?;
    validate_scene(&decoded)?;

    println!(
        "seeded drop proof scene at {} with {} zone(s)",
        zones_path.display(),
        decoded.len()
    );
    Ok(())
}

fn drop_zone_scene() -> ZoneList {
    let mut zones = ZoneList::new();
    let mut zone = Zone::new(
        ZoneId(1),
        Cow::Borrowed("Drop Proof Zone"),
        64,
        72,
        340,
        220,
    );
    zone.set_icon(Cow::Borrowed("folder"));
    zone.set_accent_color(Some(Cow::Borrowed("#3b82f6")));
    zone.set_grid_columns(5);
    zone.set_capsule_size(Cow::Borrowed("medium"));
    zone.set_capsule_shape(Cow::Borrowed("pill"));
    zone.set_display_mode(Some(Cow::Borrowed("always")));
    zones.add(zone);
    zones
}

fn zones_path_from_args<I>(mut args: I) -> Result<PathBuf, String>
where
    I: Iterator<Item = String>,
{
    let Some(first) = args.next() else {
        return Err(
            "usage: cargo run -p bentodesk-platform --example seed_drop_zone -- <state-dir-or-zones.bin>"
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

fn validate_scene(zones: &ZoneList) -> Result<(), String> {
    if zones.len() != 1 {
        return Err(format!("expected 1 zone, got {}", zones.len()));
    }
    let Some(zone) = zones.get(ZoneId(1)) else {
        return Err("expected zone 1".to_owned());
    };
    if !zone.items.is_empty() {
        return Err(format!(
            "expected empty drop zone, got {}",
            zone.items.len()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn drop_scene_has_one_empty_visible_zone() {
        let zones = drop_zone_scene();
        validate_scene(&zones).expect("valid scene");
        let zone = zones.get(ZoneId(1)).expect("zone 1");
        assert_eq!(zone.title.as_ref(), "Drop Proof Zone");
        assert_eq!(zone.display_mode.as_deref(), Some("always"));
        assert!(zone.items.is_empty());
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
