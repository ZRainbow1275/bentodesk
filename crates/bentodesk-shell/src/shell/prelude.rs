//! Imports shared by the native shell owner modules.

pub(super) use std::borrow::Cow;
pub(super) use std::cell::{Cell, RefCell};
pub(super) use std::os::windows::ffi::OsStrExt;
pub(super) use std::path::{Path, PathBuf};
pub(super) use std::ptr;
pub(super) use std::sync::OnceLock;
pub(super) use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};
pub(super) use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(super) use bentodesk_app::{
    AppState, BulkLayoutAlgorithm, BulkZoneUpdate, Command, EventDispatcher, IconPickerSession,
    ItemDragCandidate, ItemFileRenameSession, PalettePickerSession, PanelHeaderButtonHover,
    PanelHeaderButtonKind, PassphraseEntryPurpose, Renderer, SettingsBackupEntry,
    SettingsBackupStatus, SettingsEncryptionMode, SettingsKeybindingFeedback, SettingsPluginEntry,
    SettingsUpdaterStatus, ThemeOption, WindowRegistry, WindowSlot, WindowState, ZoneDisplayMode,
    ZoneEditorSession,
    animator::{AnimChannel, Easing, INLINE_SEARCH_IN_DURATION_MS, INLINE_SEARCH_OUT_DURATION_MS},
    business::minibar::{self, MAX_MINIBARS as BUSINESS_MAX_MINIBARS, MiniBar, MiniBarRoster},
    business::{
        bulk_manager_panel::{
            self, BulkManagerAction, BulkManagerPointerHit, BulkTextEditField, ZoneRow,
        },
        capsule_picker::{self, CapsuleEntry, CapsulePickerHit},
        highlight_overlay::{self, HighlightPulse, HighlightRect},
        icons::{ALL_ICON_KINDS, IconKind},
        palette_picker, popover,
        rules_wizard::{
            self, ActionKind, PredicateKind, RulesWizardAction, RulesWizardPointerHit,
            RunModeChoice, WizardStep,
        },
        search_bar::{self, SearchBarPointerHit},
        settings::keybindings_section,
        smart_group_suggestor::{self, SuggestorPointerHit},
        stack_tray::{self, StackTrayDragState, StackTrayPointerHit, StackTrayState},
        timeline::{
            panel::{self as timeline_panel, TimelinePointerHit},
            snapshot_picker::{self, SnapshotPickerPointerHit},
        },
        tray_menu::TrayMenuItem,
        zone_editor::{
            ACCENT_PALETTE, CapsuleShapeChoice, CapsuleSizeChoice, GRID_COLUMNS_MAX,
            GRID_COLUMNS_MIN, NAME_MAX_LEN,
        },
    },
    dispatcher::{PaletteTarget, Point as DispatchPoint, Size as DispatchSize, ZoneSpec},
    item_file_rename_geometry::{self, ItemFileRenameHit},
    picker_geometry::{self, IconPickerHit, PalettePickerHit},
    state::SettingsSnapshot,
    zone_editor_geometry::{self, ZoneEditorHit},
    zone_pill_geometry,
};
pub(super) use bentodesk_backend::{
    layout::{
        BentoItem, BentoZone, DesktopSnapshot, GridPosition, ItemType, LayoutData, LayoutError,
        RelativePosition, RelativeSize, Resolution, SnapshotManager,
    },
    plugins::{self, PluginRegistry, PluginType},
    rules::{
        self, ExecutionReport, Rule,
        executor::{self as rule_executor, ActionEffect, ExecutionPlan},
        scheduler::SchedulerEvent,
    },
    search::{Index as SearchIndex, SearchItem, SearchItemKind},
    themes::{self, ThemeError},
    timeline::{
        self, AutoCoalesceMode, Checkpoint, CheckpointError, CheckpointStore, DeltaSummary,
        TimelineBuffer,
    },
    updater::{UpdateCheckFrequency, UpdateEvent, Updater},
};
pub(super) use bentodesk_platform::{
    PlatformError, WindowDesc, WindowKind, create_window, default_size, message_loop, storage,
    to_windows_hwnd,
};
pub(super) use bentodesk_tree::NodeId;
pub(super) use bentodesk_widget::WidgetNode;
pub(super) use bentodesk_zone::{
    DEFAULT_ZONE_CAPSULE_SHAPE, DEFAULT_ZONE_CAPSULE_SIZE, DEFAULT_ZONE_DISPLAY_MODE,
    DEFAULT_ZONE_GRID_COLUMNS, DEFAULT_ZONE_ICON, Zone, ZoneId, ZoneItem, ZoneItemId, ZoneList,
    display_name_for_path,
};
pub(super) use mimalloc::MiMalloc;
pub(super) use serde::{Deserialize, Serialize};
pub(super) use smol_str::SmolStr;
pub(super) use windows_sys::Win32::Foundation::{
    BOOL, COLORREF, CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, GlobalFree, HANDLE, HWND,
    LPARAM, LRESULT, POINT, RECT, WPARAM,
};
// #19-B (2026-05-31) — OS UI-language default. kernel32 Vista+; static import
// safe on our Win7+ target.
pub(super) use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
pub(super) use windows_sys::Win32::Graphics::Dwm::DwmFlush;
pub(super) use windows_sys::Win32::Graphics::Gdi::{
    ClientToScreen, InvalidateRect, ScreenToClient, UpdateWindow,
};
// Wave 15 — Tier 0 #29/#31. `EmptyWorkingSet(GetCurrentProcess())` is the
// one-shot, post-first-paint working-set trim that pushes cold pages onto
// the standby list so Windows can reclaim them under memory pressure.
pub(super) use windows::Win32::Foundation::HWND as WindowsHwnd;
pub(super) use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
pub(super) use windows::Win32::System::Ole::{OleInitialize, OleUninitialize};
pub(super) use windows::Win32::UI::Shell::{
    FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog,
    IFileOpenDialog, SIGDN_FILESYSPATH,
};
pub(super) use windows::core::{Error as WindowsError, PCWSTR};
pub(super) use windows_sys::Win32::System::Com::CoTaskMemFree;
pub(super) use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
pub(super) use windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringA;
pub(super) use windows_sys::Win32::System::Memory::{
    GMEM_MOVEABLE, GetProcessHeaps, GlobalAlloc, GlobalFlags, GlobalLock, GlobalSize, GlobalUnlock,
    HeapCompact, MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE,
    PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_NOACCESS, PAGE_READONLY, PAGE_READWRITE,
    PAGE_WRITECOPY, VirtualQuery,
};
pub(super) use windows_sys::Win32::System::ProcessStatus::{EmptyWorkingSet, GetModuleFileNameExW};
pub(super) use windows_sys::Win32::System::Recovery::{
    RESTART_NO_HANG, RESTART_NO_PATCH, RESTART_NO_REBOOT, RegisterApplicationRestart,
    UnregisterApplicationRestart,
};
pub(super) use windows_sys::Win32::System::SystemInformation::GetTickCount;
pub(super) use windows_sys::Win32::System::Threading::{
    ABOVE_NORMAL_PRIORITY_CLASS, AttachThreadInput, CreateMutexW, GetCurrentProcess,
    GetCurrentThreadId, NORMAL_PRIORITY_CLASS, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    SetPriorityClass,
};
pub(super) use windows_sys::Win32::UI::Controls::Dialogs::{
    CC_ANYCOLOR, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW, ChooseColorW, CommDlgExtendedError,
    GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_PATHMUSTEXIST,
    OPENFILENAMEW,
};
pub(super) use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
pub(super) use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey,
    ReleaseCapture, SetActiveWindow, SetCapture, SetFocus, TME_LEAVE, TRACKMOUSEEVENT,
    TrackMouseEvent, UnregisterHotKey, VK_CONTROL,
};
pub(super) use windows_sys::Win32::UI::Shell::{
    DragAcceptFiles, DragFinish, DragQueryFileW, DragQueryPoint, FO_DELETE, FOF_ALLOWUNDO,
    FOF_NOERRORUI, FOF_SILENT, FOF_WANTNUKEWARNING, HDROP, NIF_GUID, NIF_ICON, NIF_MESSAGE,
    NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_SETVERSION, NIN_SELECT, NOTIFYICON_VERSION_4,
    NOTIFYICONDATAW, SHFILEOPSTRUCTW, SHFileOperationW, Shell_NotifyIconW, ShellExecuteW,
};
pub(super) use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, BringWindowToTop, CREATESTRUCTW, CreatePopupMenu, DefWindowProcW, DestroyMenu,
    DestroyWindow, EnumWindows, FindWindowW, GWL_EXSTYLE, GWLP_USERDATA, GetClassNameW,
    GetClientRect, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, HTTRANSPARENT, HWND_NOTOPMOST,
    HWND_TOP, HWND_TOPMOST, IsIconic, IsWindow, IsWindowVisible, IsZoomed, KillTimer, MB_ICONERROR,
    MB_OK, MF_SEPARATOR, MF_STRING, MessageBoxW, PostMessageW, PostQuitMessage,
    RegisterWindowMessageW, SW_HIDE, SW_MAXIMIZE, SW_RESTORE, SW_SHOW, SW_SHOWNOACTIVATE,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow,
    SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, TPM_LEFTALIGN, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WA_INACTIVE, WM_ACTIVATE, WM_APP, WM_CHAR,
    WM_COMMAND, WM_CONTEXTMENU, WM_CREATE, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED,
    WM_DROPFILES, WM_HOTKEY, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_NCHITTEST, WM_PAINT, WM_POWERBROADCAST,
    WM_RBUTTONUP, WM_SETTINGCHANGE, WM_SHOWWINDOW, WM_SIZE, WM_SYSKEYDOWN, WM_TIMER,
    WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WindowFromPoint,
};

pub(super) use bentodesk_shell::{hotkey, ui};
