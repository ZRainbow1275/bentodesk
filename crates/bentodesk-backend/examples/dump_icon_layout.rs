use std::env;
use std::fs;
use std::path::PathBuf;

use bentodesk_backend::icon_positions;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output_path = output_path(env::args().skip(1))?;
    let layout = icon_positions::save_layout()?;
    let json = serde_json::to_string_pretty(&layout)?;
    match output_path {
        Some(path) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, json)?;
            println!(
                "saved icon layout icons={} path={}",
                layout.icons.len(),
                path.display()
            );
        }
        None => {
            println!("{json}");
        }
    }
    Ok(())
}

fn output_path<I>(mut args: I) -> Result<Option<PathBuf>, String>
where
    I: Iterator<Item = String>,
{
    match args.next() {
        None => Ok(None),
        Some(flag) if flag == "--out" => {
            let Some(path) = args.next() else {
                return Err("missing path after --out".to_owned());
            };
            if args.next().is_some() {
                return Err("expected at most --out <path>".to_owned());
            }
            Ok(Some(PathBuf::from(path)))
        }
        Some(value) => Err(format!(
            "unknown argument: {value}; usage: dump_icon_layout [--out <path>]"
        )),
    }
}
