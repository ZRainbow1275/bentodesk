//! Seed selected-stack MiniBar pins into the real config vault format.
//!
//! This is a validation/proof utility, not a runtime shortcut: the shell still
//! restores pins through `Vault::global()` and `Command::PinZoneAsMinibar`.

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bentodesk_backend::config_vault::{SettingValue, Vault};
use smol_str::SmolStr;

const SETTING_MINIBAR_PINNED_ZONES: &str = "minibar.pinned_zones";
const MAX_MINIBARS: usize = 8;

fn main() -> ExitCode {
    match run() {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<String, String> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        return Err(format!(
            "usage: {} <state-dir-or-vault.bin> <comma-separated-zone-ids|clear>",
            args.first().map_or("seed_minibar_pins", String::as_str)
        ));
    }
    let vault_path = resolve_vault_path(Path::new(&args[1]));
    let pins = parse_pins(&args[2])?;
    let mut vault =
        Vault::open(&vault_path).map_err(|error| format!("open vault failed: {error}"))?;
    match pins {
        Some(value) => vault.set_setting(SETTING_MINIBAR_PINNED_ZONES, SettingValue::Str(value)),
        None => {
            let _ = vault.remove_setting(SETTING_MINIBAR_PINNED_ZONES);
        }
    }
    vault
        .flush()
        .map_err(|error| format!("flush vault failed: {error}"))?;
    let value = match vault.get_setting(SETTING_MINIBAR_PINNED_ZONES) {
        Some(SettingValue::Str(value)) => value.to_string(),
        Some(other) => format!("{other:?}"),
        None => "clear".to_string(),
    };
    Ok(format!(
        "seeded {SETTING_MINIBAR_PINNED_ZONES}={value} at {}",
        vault_path.display()
    ))
}

fn resolve_vault_path(input: &Path) -> PathBuf {
    if input
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("vault.bin"))
    {
        return input.to_path_buf();
    }
    input.join("vault.bin")
}

fn parse_pins(raw: &str) -> Result<Option<SmolStr>, String> {
    if raw.trim().eq_ignore_ascii_case("clear") {
        return Ok(None);
    }
    let mut ids = Vec::<u64>::new();
    for token in raw.split(',') {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        let id = trimmed
            .parse::<u64>()
            .map_err(|_| format!("invalid zone id: {trimmed}"))?;
        if id == 0 {
            return Err("zone id 0 is reserved".to_string());
        }
        if ids.contains(&id) {
            continue;
        }
        if ids.len() >= MAX_MINIBARS {
            return Err(format!("too many minibar pins; max is {MAX_MINIBARS}"));
        }
        ids.push(id);
    }
    if ids.is_empty() {
        return Ok(None);
    }
    let mut csv = String::new();
    for id in ids {
        if !csv.is_empty() {
            csv.push(',');
        }
        csv.push_str(&id.to_string());
    }
    Ok(Some(SmolStr::new(csv)))
}

#[cfg(test)]
mod tests {
    use super::{parse_pins, resolve_vault_path};
    use std::path::Path;

    #[test]
    fn parse_pins_deduplicates_and_normalizes_csv() {
        let pins = parse_pins(" 1, 2,1,3 ").expect("pins").expect("value");
        assert_eq!(pins.as_str(), "1,2,3");
    }

    #[test]
    fn parse_pins_rejects_zero_invalid_and_overflow() {
        assert!(parse_pins("0").is_err());
        assert!(parse_pins("abc").is_err());
        assert!(parse_pins("1,2,3,4,5,6,7,8,9").is_err());
    }

    #[test]
    fn parse_pins_clear_removes_key() {
        assert_eq!(parse_pins("clear").expect("clear"), None);
        assert_eq!(parse_pins("  ").expect("empty"), None);
    }

    #[test]
    fn resolve_vault_path_accepts_dir_or_file() {
        assert_eq!(
            resolve_vault_path(Path::new(r"C:\state")),
            Path::new(r"C:\state").join("vault.bin")
        );
        assert_eq!(
            resolve_vault_path(Path::new(r"C:\state\vault.bin")),
            Path::new(r"C:\state\vault.bin")
        );
    }
}
