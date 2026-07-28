use std::env;

use bentodesk_backend::icon_positions;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (name, x, y) = args(env::args().skip(1))?;
    icon_positions::set_single_icon_position(&name, x, y)?;
    println!("set desktop icon position name={name} x={x} y={y}");
    Ok(())
}

fn args<I>(mut args: I) -> Result<(String, i32, i32), String>
where
    I: Iterator<Item = String>,
{
    let Some(name) = args.next() else {
        return Err("usage: set_desktop_icon_position <display-name> <x> <y>".to_owned());
    };
    let Some(x) = args.next() else {
        return Err("missing x".to_owned());
    };
    let Some(y) = args.next() else {
        return Err("missing y".to_owned());
    };
    if args.next().is_some() {
        return Err("expected exactly three arguments".to_owned());
    }
    let x = x
        .parse::<i32>()
        .map_err(|error| format!("invalid x coordinate: {error}"))?;
    let y = y
        .parse::<i32>()
        .map_err(|error| format!("invalid y coordinate: {error}"))?;
    Ok((name, x, y))
}
