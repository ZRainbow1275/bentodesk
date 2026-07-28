use std::borrow::Cow;
use std::env;
use std::path::PathBuf;

use bento_nano_platform::storage::{read_zones, write_zones_atomic};
use bento_nano_zone::{Zone, ZoneId, ZoneList};

const MIN_PROOF_FILES: usize = 3;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (zones_path, proof_files) = args_to_paths(env::args().skip(1))?;
    if let Some(parent) = zones_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    for proof_file in &proof_files {
        if !proof_file.exists() {
            return Err(format!("proof file does not exist: {}", proof_file.display()).into());
        }
    }

    let zones = search_suggestor_scene(&proof_files);
    write_zones_atomic(&zones_path, &zones)?;
    let decoded = read_zones(&zones_path)?;
    validate_scene(&decoded, proof_files.len())?;

    println!(
        "seeded Search/Suggestor scene with {} zone(s), {} item(s) at {}",
        decoded.len(),
        decoded.iter().map(|zone| zone.items.len()).sum::<usize>(),
        zones_path.display()
    );
    Ok(())
}

fn args_to_paths<I>(mut args: I) -> Result<(PathBuf, Vec<PathBuf>), String>
where
    I: Iterator<Item = String>,
{
    let Some(first) = args.next() else {
        return Err(
            "usage: cargo run -p bento-nano-platform --example seed_search_suggestor_scene -- <state-dir-or-zones.bin> <desktop-file>...".to_owned(),
        );
    };
    let zones_path = zones_path_from_arg(&first);
    let proof_files = args.map(PathBuf::from).collect::<Vec<_>>();
    if proof_files.len() < MIN_PROOF_FILES {
        return Err(format!(
            "expected at least {MIN_PROOF_FILES} proof files, got {}",
            proof_files.len()
        ));
    }
    Ok((zones_path, proof_files))
}

fn zones_path_from_arg(arg: &str) -> PathBuf {
    let path = PathBuf::from(arg);
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("zones.bin"))
    {
        path
    } else {
        path.join("zones.bin")
    }
}

fn search_suggestor_scene(proof_files: &[PathBuf]) -> ZoneList {
    let mut zones = ZoneList::new();
    let mut zone = Zone::new(
        ZoneId(1),
        Cow::Borrowed("Proof Search SmartGroup"),
        64,
        72,
        560,
        260,
    );
    zone.set_icon(Cow::Borrowed("search"));
    zone.set_accent_color(Some(Cow::Borrowed("#3b82f6")));
    zone.set_grid_columns(6);
    zone.set_display_mode(Some(Cow::Borrowed("always")));

    for proof_file in proof_files {
        let _ = zone.add_item(
            Cow::Owned(proof_file.to_string_lossy().into_owned()),
            Cow::Borrowed("builtin:file"),
        );
    }

    zones.add(zone);
    zones
}

fn validate_scene(zones: &ZoneList, expected_items: usize) -> Result<(), String> {
    if zones.len() != 1 {
        return Err(format!("expected 1 zone, got {}", zones.len()));
    }
    let Some(zone) = zones.get(ZoneId(1)) else {
        return Err("expected zone 1 to exist".to_owned());
    };
    if zone.items.len() != expected_items {
        return Err(format!(
            "expected {expected_items} items, got {}",
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
    fn zones_path_accepts_directory_or_file() {
        assert_eq!(
            zones_path_from_arg(r"C:\Temp\bento-search-proof"),
            Path::new(r"C:\Temp\bento-search-proof").join("zones.bin")
        );
        assert_eq!(
            zones_path_from_arg(r"C:\Temp\bento-search-proof\zones.bin"),
            PathBuf::from(r"C:\Temp\bento-search-proof\zones.bin")
        );
    }

    #[test]
    fn args_require_real_proof_files_argument_count() {
        let err = args_to_paths([String::from(r"C:\Temp\bento-search-proof")].into_iter())
            .expect_err("missing proof files rejected");
        assert!(err.contains("expected at least"));
    }

    #[test]
    fn scene_uses_every_supplied_desktop_file() {
        let files = vec![
            PathBuf::from(r"C:\Users\BentoDeskTest\Desktop\bento-proof-a.pdf"),
            PathBuf::from(r"C:\Users\BentoDeskTest\Desktop\bento-proof-b.txt"),
            PathBuf::from(r"C:\Users\BentoDeskTest\Desktop\bento-proof-c.png"),
        ];
        let zones = search_suggestor_scene(&files);
        validate_scene(&zones, files.len()).expect("valid scene");
        let zone = zones.get(ZoneId(1)).expect("zone 1");
        assert_eq!(zone.items[0].path.as_ref(), files[0].to_string_lossy());
        assert_eq!(zone.display_mode.as_deref(), Some("always"));
    }
}
