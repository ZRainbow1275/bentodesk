//! Build script (Mc-1a / W13-A) — embeds the application manifest and native
//! BentoDesk icon resources into the shell EXE.
//!
//! The manifest declares per-monitor DPI awareness, `supportedOS` GUIDs (so the
//! version query is truthful), `longPathAware`, and `asInvoker` execution level.
//! It is embedded via the MSVC linker (`/MANIFEST:EMBED` + `/MANIFESTINPUT`),
//! which MERGES it with the default manifest rustc injects — no `winres` /
//! `embed-resource` / `embed-manifest` crate (spec §8 no-new-crate).
//!
//! `app.res` is the architecture-neutral output of the Windows SDK resource
//! compiler for `app.rc`; linking the checked-in resource keeps blank-machine
//! builds dependency-free (no `winres` / `embed-resource` crate).
//!
//! `unwrap_or_default()` here is build-time only and is NOT part of the §11
//! runtime panic-form surface.

fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=app.res");
    println!("cargo:rerun-if-changed=app-icon.ico");
    println!("cargo:rerun-if-changed=tray-icon.ico");

    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
        let mani = std::path::Path::new(&dir).join("app.manifest");
        let resources = std::path::Path::new(&dir).join("app.res");
        println!("cargo:rustc-link-arg-bin=BentoDesk=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bin=BentoDesk=/MANIFESTINPUT:{}",
            mani.display()
        );
        println!("cargo:rustc-link-arg-bin=BentoDesk={}", resources.display());
    }
}
