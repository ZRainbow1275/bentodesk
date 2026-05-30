//! Build script (Mc-1a) — embeds `app.manifest` into the shell EXE.
//!
//! The manifest declares per-monitor DPI awareness, `supportedOS` GUIDs (so the
//! version query is truthful), `longPathAware`, and `asInvoker` execution level.
//! It is embedded via the MSVC linker (`/MANIFEST:EMBED` + `/MANIFESTINPUT`),
//! which MERGES it with the default manifest rustc injects — no `winres` /
//! `embed-resource` / `embed-manifest` crate (spec §8 no-new-crate).
//!
//! `unwrap_or_default()` here is build-time only and is NOT part of the §11
//! runtime panic-form surface.

fn main() {
    println!("cargo:rerun-if-changed=app.manifest");

    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
        let mani = std::path::Path::new(&dir).join("app.manifest");
        println!("cargo:rustc-link-arg-bin=bento-nano-shell=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bin=bento-nano-shell=/MANIFESTINPUT:{}",
            mani.display()
        );
    }
}
