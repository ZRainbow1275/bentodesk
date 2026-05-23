//! Local updater proof installer.
//!
//! This is a deliberately tiny executable used only by runtime proof scripts.
//! The selected-stack updater launches staged installers with `/S`; this helper
//! records that launch and can relaunch the selected-stack shell using paths
//! passed through environment variables.

use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

fn append_marker() -> std::io::Result<Option<PathBuf>> {
    let Some(marker) = env::var_os("BENTODESK_NANO_UPDATE_PROOF_MARKER") else {
        return Ok(None);
    };
    let marker = PathBuf::from(marker);
    if let Some(parent) = marker.parent() {
        fs::create_dir_all(parent)?;
    }
    let args = env::args().collect::<Vec<_>>().join(" ");
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mut file = OpenOptions::new().create(true).append(true).open(&marker)?;
    writeln!(
        file,
        "proof_update_installer launched pid={} timestamp={} args={}",
        std::process::id(),
        timestamp,
        args
    )?;
    Ok(Some(marker))
}

fn restart_shell() -> std::io::Result<()> {
    let Some(exe) = env::var_os("BENTODESK_NANO_UPDATE_PROOF_RESTART_EXE") else {
        return Ok(());
    };
    let mut command = Command::new(exe);
    if let Some(state_dir) = env::var_os("BENTODESK_NANO_STATE_DIR") {
        command.env("BENTODESK_NANO_STATE_DIR", state_dir);
    }
    command.env("BENTODESK_NANO_UPDATE_PROOF_RESTARTED", "1");
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.spawn().map(|_| ())
}

fn main() {
    if let Err(error) = append_marker().and_then(|_| restart_shell()) {
        let _ = writeln!(std::io::stderr(), "proof_update_installer failed: {error}");
        std::process::exit(1);
    }
}
