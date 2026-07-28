use std::borrow::Cow;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use bentodesk_platform::storage::{read_zones, write_zones_atomic};
use bentodesk_zone::{Zone, ZoneId, ZoneList};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse(env::args().skip(1))?;
    if let Some(parent) = args.zones_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir_all(&args.live_folder)?;
    write_live_folder_seed_files(&args.live_folder)?;

    let zones = live_folder_scene(&args.live_folder);
    write_zones_atomic(&args.zones_path, &zones)?;
    let decoded = read_zones(&args.zones_path)?;
    validate_scene(&decoded, &args.live_folder)?;

    println!(
        "seeded live-folder proof scene at {} bound to {}",
        args.zones_path.display(),
        args.live_folder.display()
    );
    Ok(())
}

struct Args {
    zones_path: PathBuf,
    live_folder: PathBuf,
}

impl Args {
    fn parse<I>(mut args: I) -> Result<Self, String>
    where
        I: Iterator<Item = String>,
    {
        let Some(state_or_zones_path) = args.next() else {
            return Err(
                "usage: cargo run -p bentodesk-platform --example seed_live_folder_scene -- <state-dir-or-zones.bin> <live-folder>"
                    .to_owned(),
            );
        };
        let Some(live_folder) = args.next() else {
            return Err("expected a live-folder path".to_owned());
        };
        if args.next().is_some() {
            return Err("expected exactly two arguments".to_owned());
        }
        Ok(Self {
            zones_path: zones_path_from_input(Path::new(&state_or_zones_path)),
            live_folder: PathBuf::from(live_folder),
        })
    }
}

fn zones_path_from_input(input: &Path) -> PathBuf {
    if input
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("zones.bin"))
    {
        input.to_path_buf()
    } else {
        input.join("zones.bin")
    }
}

fn live_folder_scene(live_folder: &Path) -> ZoneList {
    let mut zones = ZoneList::new();
    let mut zone = Zone::new(
        ZoneId(1),
        Cow::Borrowed("Live Folder Proof Zone"),
        64,
        72,
        420,
        260,
    );
    zone.set_icon(Cow::Borrowed("folder"));
    zone.set_accent_color(Some(Cow::Borrowed("#22c55e")));
    zone.set_grid_columns(4);
    zone.set_capsule_size(Cow::Borrowed("medium"));
    zone.set_capsule_shape(Cow::Borrowed("pill"));
    zone.set_display_mode(Some(Cow::Borrowed("always")));
    zone.set_live_folder_path(Some(Cow::Owned(live_folder.to_string_lossy().to_string())));
    zones.add(zone);
    zones
}

fn write_live_folder_seed_files(live_folder: &Path) -> Result<(), std::io::Error> {
    fs::write(
        live_folder.join("alpha.txt"),
        "BentoDesk selected-stack live folder proof alpha\n",
    )?;
    fs::write(
        live_folder.join("beta.txt"),
        "BentoDesk selected-stack live folder proof beta\n",
    )?;
    Ok(())
}

fn validate_scene(zones: &ZoneList, live_folder: &Path) -> Result<(), String> {
    if zones.len() != 1 {
        return Err(format!("expected 1 zone, got {}", zones.len()));
    }
    let Some(zone) = zones.get(ZoneId(1)) else {
        return Err("expected zone 1".to_owned());
    };
    let expected = live_folder.to_string_lossy();
    if zone.live_folder_path.as_deref() != Some(expected.as_ref()) {
        return Err(format!(
            "expected live folder path {}",
            live_folder.display()
        ));
    }
    if !zone.items.is_empty() {
        return Err(format!(
            "expected startup refresh to own items, got {} seeded items",
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
    fn scene_binds_zone_without_preseeding_items() {
        let live_folder = Path::new(r"C:\Temp\bento-live-proof");
        let zones = live_folder_scene(live_folder);
        validate_scene(&zones, live_folder).expect("valid scene");
        let zone = zones.get(ZoneId(1)).expect("zone 1");
        assert_eq!(zone.title.as_ref(), "Live Folder Proof Zone");
        assert_eq!(
            zone.live_folder_path.as_deref(),
            Some(r"C:\Temp\bento-live-proof")
        );
        assert!(zone.items.is_empty());
    }

    #[test]
    fn output_path_accepts_directory_or_zones_bin() {
        assert_eq!(
            zones_path_from_input(Path::new(r"C:\Temp\bento-state")),
            Path::new(r"C:\Temp\bento-state").join("zones.bin")
        );
        assert_eq!(
            zones_path_from_input(Path::new(r"C:\Temp\bento-state\zones.bin")),
            PathBuf::from(r"C:\Temp\bento-state\zones.bin")
        );
    }

    #[test]
    fn args_require_state_and_folder_only() {
        assert!(Args::parse([String::from(r"C:\state")].into_iter()).is_err());
        assert!(
            Args::parse(
                [
                    String::from(r"C:\state"),
                    String::from(r"C:\live"),
                    String::from("extra"),
                ]
                .into_iter(),
            )
            .is_err()
        );
        let args = Args::parse([String::from(r"C:\state"), String::from(r"C:\live")].into_iter())
            .expect("args");
        assert_eq!(args.zones_path, Path::new(r"C:\state").join("zones.bin"));
        assert_eq!(args.live_folder, PathBuf::from(r"C:\live"));
    }
}
