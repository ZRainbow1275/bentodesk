use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use bentodesk_backend::icon_positions::SavedIconLayout;
use bentodesk_backend::recovery_bundle;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (zones_path, zone_count, icon_layout_path) = args(env::args().skip(1))?;
    let data_root = recovery_bundle::data_root_for_state_file(&zones_path)?;
    let zones_bin = fs::read(&zones_path)?;
    let vault_path = data_root.join("vault.bin");
    let vault_bin = if vault_path.exists() {
        Some(fs::read(&vault_path)?)
    } else {
        None
    };
    let vault_payload = vault_bin
        .as_ref()
        .map(|bytes| (vault_path.as_path(), bytes.as_slice()));
    let icon_backup = match icon_layout_path {
        Some(path) => Some(read_icon_layout(&path)?),
        None => None,
    };
    let summary = recovery_bundle::refresh_bundle(
        &data_root,
        &zones_path,
        &zones_bin,
        zone_count,
        vault_payload,
        &[],
        icon_backup,
    )?;
    let bundle = recovery_bundle::load_bundle(&data_root)?.ok_or("bundle was not written")?;
    if bundle.zone_count != zone_count {
        return Err(format!(
            "bundle zone_count mismatch: expected {}, got {}",
            zone_count, bundle.zone_count
        )
        .into());
    }
    println!(
        "seeded recovery bundle zones={} vault={} icon_backup={} path={}",
        summary.zone_count,
        summary.vault_included,
        summary.icon_backup_included,
        summary.path.display()
    );
    Ok(())
}

fn read_icon_layout(path: &Path) -> Result<SavedIconLayout, Box<dyn std::error::Error>> {
    let json = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&json)?)
}

fn args<I>(mut args: I) -> Result<(PathBuf, u32, Option<PathBuf>), String>
where
    I: Iterator<Item = String>,
{
    let Some(path) = args.next() else {
        return Err(
            "usage: cargo run -p bentodesk-backend --example seed_recovery_bundle -- <state-dir-or-zones.bin> <zone-count> [icon-layout-backup-json]"
                .to_owned(),
        );
    };
    let Some(zone_count) = args.next() else {
        return Err("missing zone count".to_owned());
    };
    let zone_count = zone_count
        .parse::<u32>()
        .map_err(|error| format!("invalid zone count: {error}"))?;
    let icon_layout = args.next().map(PathBuf::from);
    if args.next().is_some() {
        return Err("expected two or three arguments".to_owned());
    }
    Ok((
        zones_path_from_arg(PathBuf::from(path)),
        zone_count,
        icon_layout,
    ))
}

fn zones_path_from_arg(path: PathBuf) -> PathBuf {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn zones_path_accepts_state_dir_or_file() {
        assert_eq!(
            zones_path_from_arg(PathBuf::from(r"C:\Temp\bento-state")),
            Path::new(r"C:\Temp\bento-state").join("zones.bin")
        );
        assert_eq!(
            zones_path_from_arg(PathBuf::from(r"C:\Temp\bento-state\zones.bin")),
            PathBuf::from(r"C:\Temp\bento-state\zones.bin")
        );
    }

    #[test]
    fn args_parse_zone_count() {
        let parsed = args([String::from(r"C:\Temp\bento-state"), String::from("5")].into_iter())
            .expect("valid args");
        assert_eq!(
            parsed.0,
            Path::new(r"C:\Temp\bento-state").join("zones.bin")
        );
        assert_eq!(parsed.1, 5);
        assert!(parsed.2.is_none());
    }

    #[test]
    fn args_parse_optional_icon_backup_path() {
        let parsed = args(
            [
                String::from(r"C:\Temp\bento-state"),
                String::from("5"),
                String::from(r"C:\Temp\icon-layout.json"),
            ]
            .into_iter(),
        )
        .expect("valid args");
        assert_eq!(
            parsed.2.as_deref(),
            Some(Path::new(r"C:\Temp\icon-layout.json"))
        );
    }
}
