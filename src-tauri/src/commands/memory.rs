//! WebView2 process-group memory sampling.
//!
//! Theme B — the Tauri host process is only one half of the story; the
//! WebView2 renderer/GPU/utility children account for the majority of
//! the working set in practice. This command enumerates every process
//! in the descendant tree rooted at our host PID and aggregates
//! working-set + peak-working-set so the frontend can render a real
//! "total app memory" number.
//!
//! The implementation avoids WebView2's COM-based `GetProcessInfos`
//! because that requires an `ICoreWebView2Environment` handle that
//! Tauri does not publicly surface. Walking the Toolhelp32 snapshot
//! works without extra WebView2 plumbing and captures every generation
//! of WebView2 child (renderer / gpu / utility / crashpad) uniformly.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct WebView2ProcessSample {
    pub pid: u32,
    pub name: String,
    /// Best-effort classification: "host", "webview2",
    /// "webview2-other", "child".
    pub kind: String,
    pub working_set_bytes: usize,
    pub peak_working_set_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct WebView2MemoryInfo {
    pub host_pid: u32,
    pub processes: Vec<WebView2ProcessSample>,
    pub total_working_set_bytes: usize,
    pub total_peak_working_set_bytes: usize,
}

#[tauri::command]
pub async fn get_webview2_memory() -> Result<WebView2MemoryInfo, String> {
    Ok(query_webview2_memory())
}

fn query_webview2_memory() -> WebView2MemoryInfo {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::GetCurrentProcessId;

    let host_pid = unsafe { GetCurrentProcessId() };

    // SAFETY: CreateToolhelp32Snapshot returns an owned handle that we
    // close below. On failure we return the host-only info.
    let snap = match unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) } {
        Ok(h) => h,
        Err(_) => {
            return WebView2MemoryInfo {
                host_pid,
                ..Default::default()
            }
        }
    };

    let mut entries: Vec<PROCESSENTRY32W> = Vec::new();
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: Process32FirstW/NextW are called with a valid snapshot
    // handle and a fresh PROCESSENTRY32W whose dwSize has been set.
    unsafe {
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                entries.push(entry);
                entry = PROCESSENTRY32W {
                    dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                    ..Default::default()
                };
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = windows::Win32::Foundation::CloseHandle(snap);
    }

    // Compute the descendant closure rooted at our host PID. WebView2's
    // msedgewebview2.exe parent tree can be multi-generation (host →
    // manager → renderer/gpu/utility). A fix-point on `th32ParentProcessID`
    // captures every generation.
    let mut descendants: std::collections::HashSet<u32> = std::collections::HashSet::new();
    descendants.insert(host_pid);
    let mut changed = true;
    while changed {
        changed = false;
        for e in &entries {
            if descendants.contains(&e.th32ParentProcessID)
                && !descendants.contains(&e.th32ProcessID)
            {
                descendants.insert(e.th32ProcessID);
                changed = true;
            }
        }
    }

    let mut info = WebView2MemoryInfo {
        host_pid,
        ..Default::default()
    };

    for e in &entries {
        if !descendants.contains(&e.th32ProcessID) {
            continue;
        }
        let name = pwide_to_string(&e.szExeFile);
        let kind = classify_process(&name, e.th32ProcessID == host_pid);
        let (ws, peak) = sample_process_memory(e.th32ProcessID);

        info.total_working_set_bytes = info.total_working_set_bytes.saturating_add(ws);
        info.total_peak_working_set_bytes = info.total_peak_working_set_bytes.saturating_add(peak);

        info.processes.push(WebView2ProcessSample {
            pid: e.th32ProcessID,
            name,
            kind,
            working_set_bytes: ws,
            peak_working_set_bytes: peak,
        });
    }

    info
}

fn sample_process_memory(pid: u32) -> (usize, usize) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    let mut pmc = PROCESS_MEMORY_COUNTERS::default();
    // SAFETY: PROCESS_QUERY_LIMITED_INFORMATION is the documented
    // minimum access right for GetProcessMemoryInfo. We close the
    // handle in every branch. OpenProcess returning Err means we
    // don't have access (e.g. elevated system service) — return zeros.
    unsafe {
        if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            let _ = GetProcessMemoryInfo(
                handle,
                &mut pmc,
                std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            );
            let _ = CloseHandle(handle);
        }
    }
    (pmc.WorkingSetSize, pmc.PeakWorkingSetSize)
}

fn pwide_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

fn classify_process(name: &str, is_host: bool) -> String {
    if is_host {
        return "host".to_string();
    }
    let lower = name.to_ascii_lowercase();
    if lower == "msedgewebview2.exe" {
        "webview2".to_string()
    } else if lower.contains("webview") {
        "webview2-other".to_string()
    } else {
        "child".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_host_is_host() {
        assert_eq!(classify_process("bentodesk.exe", true), "host");
    }

    #[test]
    fn classify_webview2_child() {
        assert_eq!(classify_process("msedgewebview2.exe", false), "webview2");
    }

    #[test]
    fn classify_unknown_child() {
        assert_eq!(classify_process("notepad.exe", false), "child");
    }

    #[test]
    fn query_returns_host_pid_at_minimum() {
        let info = query_webview2_memory();
        assert!(info.host_pid > 0);
    }
}
