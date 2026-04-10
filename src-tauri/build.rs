use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    // Copy guardian binary to the binaries/ directory for Tauri externalBin bundling.
    // Tauri expects binaries at `src-tauri/binaries/<name>-<target_triple>[.exe]`.
    let target_triple = env::var("TARGET").unwrap_or_else(|_| {
        // Fallback for common Windows target
        "x86_64-pc-windows-msvc".to_string()
    });

    let profile = env::var("PROFILE").unwrap_or_else(|_| "release".to_string());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // Guardian binary is built as part of the same workspace
    let guardian_src = manifest_dir
        .join("target")
        .join(&profile)
        .join("guardian.exe");

    let binaries_dir = manifest_dir.join("binaries");
    let guardian_dst = binaries_dir.join(format!("guardian-{target_triple}.exe"));

    // Create binaries directory if it doesn't exist
    let _ = fs::create_dir_all(&binaries_dir);

    // Copy if source exists (it will exist after the first full build)
    if guardian_src.exists() {
        if let Err(e) = fs::copy(&guardian_src, &guardian_dst) {
            println!(
                "cargo:warning=Failed to copy guardian binary from {} to {}: {}",
                guardian_src.display(),
                guardian_dst.display(),
                e
            );
        }
    } else {
        // Create a placeholder so Tauri build doesn't fail on first compile.
        // The real binary will be placed by a two-pass build.
        if !guardian_dst.exists() {
            let _ = fs::write(&guardian_dst, b"placeholder");
        }
    }

    tauri_build::build();
}
