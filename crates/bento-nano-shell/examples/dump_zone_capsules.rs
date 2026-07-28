use std::env;
use std::path::PathBuf;

use bento_nano_app::zone_gesture_geometry::zone_drag_capsule_rect;
use bento_nano_platform::storage::read_zones;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let zones_path = zones_path_from_args(env::args().skip(1))?;
    let zones = read_zones(&zones_path)?;

    println!(
        "zone_id\tx\ty\twidth\theight\tvisible\tstack_parent\tstack_member_count\tcapsule_size\tcapsule_shape"
    );
    for zone in zones.iter() {
        let (x, y, width, height) = zone_drag_capsule_rect(&zones, zone);
        let stack_parent = zone
            .stack_parent
            .map(|id| id.0.to_string())
            .unwrap_or_else(|| "-".to_owned());
        println!(
            "{}\t{x}\t{y}\t{width}\t{height}\t{}\t{stack_parent}\t{}\t{}\t{}",
            zone.id.0,
            zone.visible,
            zone.stack_members.len(),
            zone.capsule_size,
            zone.capsule_shape
        );
    }
    Ok(())
}

fn zones_path_from_args<I>(mut args: I) -> Result<PathBuf, String>
where
    I: Iterator<Item = String>,
{
    let Some(first) = args.next() else {
        return Err("usage: dump_zone_capsules <state-dir-or-zones.bin>".to_owned());
    };
    if args.next().is_some() {
        return Err("expected exactly one input path".to_owned());
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
    use std::path::Path;

    #[test]
    fn output_path_accepts_directory_or_zones_bin() {
        assert_eq!(
            zones_path_from_args([String::from(r"C:\Temp\bento-state")].into_iter()),
            Ok(Path::new(r"C:\Temp\bento-state").join("zones.bin"))
        );
        assert_eq!(
            zones_path_from_args([String::from(r"C:\Temp\bento-state\zones.bin")].into_iter()),
            Ok(PathBuf::from(r"C:\Temp\bento-state\zones.bin"))
        );
    }
}
