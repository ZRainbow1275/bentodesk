use std::env;
use std::path::PathBuf;

use bentodesk_platform::storage::read_zones;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let zones_path = zones_path_from_args(env::args().skip(1))?;
    let zones = read_zones(&zones_path)?;

    println!("zone_id\titem_id\tname\tgrid_x\tgrid_y\tis_wide\tpath");
    for zone in zones.iter() {
        for item in &zone.items {
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                zone.id.0,
                item.id.0,
                tsv_escape(item.name.as_ref()),
                item.x,
                item.y,
                item.is_wide,
                tsv_escape(item.path.as_ref()),
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
            "usage: cargo run -p bentodesk-platform --example dump_zone_item_grid -- <state-dir-or-zones.bin>"
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
            zones_path_from_args([String::from(r"C:\Temp\bento-state")].into_iter()).expect("dir"),
            Path::new(r"C:\Temp\bento-state").join("zones.bin")
        );
        assert_eq!(
            zones_path_from_args([String::from(r"C:\Temp\bento-state\zones.bin")].into_iter())
                .expect("file"),
            PathBuf::from(r"C:\Temp\bento-state\zones.bin")
        );
    }

    #[test]
    fn tsv_escape_keeps_rows_machine_readable() {
        assert_eq!(tsv_escape("a\tb\r\nc\\d"), "a\\tb\\r\\nc\\\\d");
    }
}
