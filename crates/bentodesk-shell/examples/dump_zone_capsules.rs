use std::env;
use std::path::PathBuf;

use bentodesk_app::zone_gesture_geometry::zone_drag_capsule_rect;
use bentodesk_app::zone_pill_geometry::{expanded_zone_placement, morph_pill_to_rect};
use bentodesk_platform::storage::read_zones;
use bentodesk_style::{Rect, Size};

#[derive(Debug, Clone, PartialEq)]
struct DumpArgs {
    zones_path: PathBuf,
    viewport: Size,
    morph: f32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = dump_args(env::args().skip(1))?;
    let zones = read_zones(&args.zones_path)?;

    println!(
        "zone_id\thome_x\thome_y\tstored_width\tstored_height\tcapsule_x\tcapsule_y\tcapsule_width\tcapsule_height\tanchor_right\tanchor_bottom\tpanel_x\tpanel_y\tpanel_width\tpanel_height\teffective_x\teffective_y\teffective_width\teffective_height\tvisible\tstack_parent\tstack_member_count\tcapsule_size\tcapsule_shape"
    );
    for zone in zones.iter() {
        let (capsule_x, capsule_y, capsule_width, capsule_height) =
            zone_drag_capsule_rect(&zones, zone);
        let capsule = Rect {
            x: capsule_x as f32,
            y: capsule_y as f32,
            width: capsule_width as f32,
            height: capsule_height as f32,
        };
        let placement =
            expanded_zone_placement(capsule, zone.w as f32, zone.h as f32, args.viewport);
        let effective = morph_pill_to_rect(capsule, placement.panel, args.morph);
        let stack_parent = zone
            .stack_parent
            .map(|id| id.0.to_string())
            .unwrap_or_else(|| "-".to_owned());
        println!(
            "{}\t{}\t{}\t{}\t{}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{}\t{}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{}\t{stack_parent}\t{}\t{}\t{}",
            zone.id.0,
            zone.x,
            zone.y,
            zone.w,
            zone.h,
            capsule.x,
            capsule.y,
            capsule.width,
            capsule.height,
            placement.anchor_right,
            placement.anchor_bottom,
            placement.panel.x,
            placement.panel.y,
            placement.panel.width,
            placement.panel.height,
            effective.x,
            effective.y,
            effective.width,
            effective.height,
            zone.visible,
            zone.stack_members.len(),
            zone.capsule_size,
            zone.capsule_shape
        );
    }
    Ok(())
}

fn dump_args<I>(mut args: I) -> Result<DumpArgs, String>
where
    I: Iterator<Item = String>,
{
    let Some(first) = args.next() else {
        return Err(
            "usage: dump_zone_capsules <state-dir-or-zones.bin> <viewport-width> <viewport-height> [morph-0..1]"
                .to_owned(),
        );
    };
    let width = parse_positive_f32(args.next(), "viewport-width")?;
    let height = parse_positive_f32(args.next(), "viewport-height")?;
    let morph = match args.next() {
        Some(raw) => raw
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
            .ok_or_else(|| "morph must be a finite number in 0..=1".to_owned())?,
        None => 1.0,
    };
    if args.next().is_some() {
        return Err("too many arguments".to_owned());
    }
    let path = PathBuf::from(first);
    let zones_path = if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("zones.bin"))
    {
        path
    } else {
        path.join("zones.bin")
    };
    Ok(DumpArgs {
        zones_path,
        viewport: Size { width, height },
        morph,
    })
}

fn parse_positive_f32(raw: Option<String>, name: &str) -> Result<f32, String> {
    raw.and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| format!("{name} must be a finite positive number"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn args_accept_directory_or_zones_bin_and_default_or_explicit_morph() {
        assert_eq!(
            dump_args(
                [
                    String::from(r"C:\Temp\bento-state"),
                    String::from("1707"),
                    String::from("912"),
                ]
                .into_iter()
            ),
            Ok(DumpArgs {
                zones_path: Path::new(r"C:\Temp\bento-state").join("zones.bin"),
                viewport: Size {
                    width: 1707.0,
                    height: 912.0,
                },
                morph: 1.0,
            })
        );
        assert_eq!(
            dump_args(
                [
                    String::from(r"C:\Temp\bento-state\zones.bin"),
                    String::from("1280"),
                    String::from("720"),
                    String::from("0.5"),
                ]
                .into_iter()
            ),
            Ok(DumpArgs {
                zones_path: PathBuf::from(r"C:\Temp\bento-state\zones.bin"),
                viewport: Size {
                    width: 1280.0,
                    height: 720.0,
                },
                morph: 0.5,
            })
        );
    }

    #[test]
    fn args_reject_invalid_viewport_and_morph() {
        assert!(dump_args(["state", "0", "720"].map(String::from).into_iter()).is_err());
        assert!(
            dump_args(
                ["state", "1280", "720", "1.5"]
                    .map(String::from)
                    .into_iter()
            )
            .is_err()
        );
    }
}
