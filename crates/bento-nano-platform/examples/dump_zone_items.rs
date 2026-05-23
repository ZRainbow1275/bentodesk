use std::env;
use std::path::PathBuf;

use bento_nano_platform::storage::read_zones;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let zones_path = zones_path_from_args(env::args().skip(1))?;
    let zones = read_zones(&zones_path)?;

    println!("zone_id\titem_id\tname\tpath\toriginal_path\thidden_path\tmissing");
    for zone in zones.iter() {
        for item in &zone.items {
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                zone.id.0,
                item.id.0,
                tsv_escape(item.name.as_ref()),
                tsv_escape(item.path.as_ref()),
                tsv_escape(item.original_path.as_deref().unwrap_or_default()),
                tsv_escape(item.hidden_path.as_deref().unwrap_or_default()),
                item.file_missing
            );
        }
    }
    Ok(())
}

fn zones_path_from_args<I>(mut args: I) -> Result<PathBuf, String>
where
    I: Iterator<Item = String>,
{
    let Some(first) = args.next() else {
        return Err(
            "usage: cargo run -p bento-nano-platform --example dump_zone_items -- <state-dir-or-zones.bin>"
                .to_owned(),
        );
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

fn tsv_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\t' => escaped.push_str("\\t"),
            '\r' => escaped.push_str("\\r"),
            '\n' => escaped.push_str("\\n"),
            '\\' => escaped.push_str("\\\\"),
            _ => escaped.push(ch),
        }
    }
    escaped
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

    #[test]
    fn tsv_escape_keeps_rows_machine_readable() {
        assert_eq!(tsv_escape("a\tb\r\nc\\d"), "a\\tb\\r\\nc\\\\d");
    }
}
