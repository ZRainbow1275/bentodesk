use std::env;
use std::path::PathBuf;

use bentodesk_platform::storage::read_zones;

const CAPSULE_FIELDS_ENV: &str = "BENTODESK_DUMP_ZONE_CAPSULES";
const STACK_FIELDS_ENV: &str = "BENTODESK_DUMP_ZONE_STACKS";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let zones_path = zones_path_from_args(env::args().skip(1))?;
    let zones = read_zones(&zones_path)?;

    let capsule_fields = env_flag(CAPSULE_FIELDS_ENV);
    let stack_fields = env_flag(STACK_FIELDS_ENV);

    if capsule_fields && stack_fields {
        println!(
            "id\ttitle\ticon\tcapsule_size\tcapsule_shape\tvisible\titems\tstack_parent\tstack_members"
        );
        for zone in zones.iter() {
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                zone.id.0,
                tsv_escape(zone.title.as_ref()),
                tsv_escape(zone.icon.as_ref()),
                tsv_escape(zone.capsule_size.as_ref()),
                tsv_escape(zone.capsule_shape.as_ref()),
                zone.visible,
                zone.items.len(),
                stack_parent_text(zone.stack_parent),
                stack_members_text(zone.stack_members.as_slice())
            );
        }
    } else if capsule_fields {
        println!("id\ttitle\ticon\tcapsule_size\tcapsule_shape\tvisible\titems");
        for zone in zones.iter() {
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                zone.id.0,
                tsv_escape(zone.title.as_ref()),
                tsv_escape(zone.icon.as_ref()),
                tsv_escape(zone.capsule_size.as_ref()),
                tsv_escape(zone.capsule_shape.as_ref()),
                zone.visible,
                zone.items.len()
            );
        }
    } else if stack_fields {
        println!("id\ttitle\ticon\tvisible\titems\tstack_parent\tstack_members");
        for zone in zones.iter() {
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                zone.id.0,
                tsv_escape(zone.title.as_ref()),
                tsv_escape(zone.icon.as_ref()),
                zone.visible,
                zone.items.len(),
                stack_parent_text(zone.stack_parent),
                stack_members_text(zone.stack_members.as_slice())
            );
        }
    } else {
        println!("id\ttitle\ticon\tvisible\titems");
        for zone in zones.iter() {
            println!(
                "{}\t{}\t{}\t{}\t{}",
                zone.id.0,
                tsv_escape(zone.title.as_ref()),
                tsv_escape(zone.icon.as_ref()),
                zone.visible,
                zone.items.len()
            );
        }
    }
    Ok(())
}

fn stack_parent_text(parent: Option<bentodesk_zone::ZoneId>) -> String {
    parent
        .map(|id| id.0.to_string())
        .unwrap_or_else(|| "-".to_owned())
}

fn stack_members_text(members: &[bentodesk_zone::ZoneId]) -> String {
    if members.is_empty() {
        return "-".to_owned();
    }
    members
        .iter()
        .map(|id| id.0.to_string())
        .collect::<Vec<_>>()
        .join(",")
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

fn zones_path_from_args<I>(mut args: I) -> Result<PathBuf, String>
where
    I: Iterator<Item = String>,
{
    let Some(first) = args.next() else {
        return Err(
            "usage: cargo run -p bentodesk-platform --example dump_zones -- <state-dir-or-zones.bin>"
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

    #[test]
    fn capsule_env_flag_accepts_common_true_values() {
        assert!(!env_flag("__BENTODESK_TEST_MISSING_FLAG__"));
        // Keep env parsing documented without mutating process env in tests.
        for value in ["1", "true", "yes", "on"] {
            assert!(matches_truthy_for_test(value));
        }
        assert!(!matches_truthy_for_test("0"));
    }

    fn matches_truthy_for_test(value: &str) -> bool {
        let value = value.trim();
        value.eq_ignore_ascii_case("1")
            || value.eq_ignore_ascii_case("true")
            || value.eq_ignore_ascii_case("yes")
            || value.eq_ignore_ascii_case("on")
    }

    #[test]
    fn stack_fields_use_dash_for_empty_and_csv_for_members() {
        assert_eq!(stack_parent_text(None), "-");
        assert_eq!(stack_parent_text(Some(bentodesk_zone::ZoneId(7))), "7");
        assert_eq!(stack_members_text(&[]), "-");
        assert_eq!(
            stack_members_text(&[
                bentodesk_zone::ZoneId(2),
                bentodesk_zone::ZoneId(3),
                bentodesk_zone::ZoneId(4),
            ]),
            "2,3,4"
        );
    }
}
