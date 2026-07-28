use std::{env, ptr};

use windows_sys::Win32::{
    Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM},
    Graphics::Gdi::UpdateWindow,
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CREATESTRUCTW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW,
        GetMessageW, IDC_ARROW, LoadCursorW, MSG, PostQuitMessage, RegisterClassExW, SW_SHOW,
        ShowWindow, TranslateMessage, WM_CREATE, WM_DESTROY, WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
        WS_VISIBLE,
    },
};

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let _ = lparam as *const CREATESTRUCTW;
            0
        }
        WM_DESTROY => {
            unsafe {
                PostQuitMessage(0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn main() {
    let title = env::args()
        .nth(1)
        .unwrap_or_else(|| "Capsule external window proof".to_owned());
    let class_name = to_wide("CapsuleProofExternalWindow");
    let window_title = to_wide(&title);
    let hinstance = unsafe { GetModuleHandleW(ptr::null()) } as HINSTANCE;
    let wndclass = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: 0,
        lpfnWndProc: Some(wndproc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: ptr::null_mut(),
        hCursor: unsafe { LoadCursorW(ptr::null_mut(), IDC_ARROW) },
        hbrBackground: ptr::null_mut(),
        lpszMenuName: ptr::null(),
        lpszClassName: class_name.as_ptr(),
        hIconSm: ptr::null_mut(),
    };
    let atom = unsafe { RegisterClassExW(&wndclass) };
    const ERROR_CLASS_ALREADY_EXISTS: u32 = 1410;
    if atom == 0 {
        let error = std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_default() as u32;
        if error != ERROR_CLASS_ALREADY_EXISTS {
            eprintln!("capsule_external_window RegisterClassExW failed: {error}");
            std::process::exit(1);
        }
    }
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            640,
            420,
            ptr::null_mut(),
            ptr::null_mut(),
            hinstance,
            ptr::null_mut(),
        )
    };
    if hwnd.is_null() {
        let error = std::io::Error::last_os_error();
        eprintln!("capsule_external_window CreateWindowExW failed: {error}");
        std::process::exit(1);
    }
    println!(
        "capsule_external_window hwnd={} title={}",
        hwnd as isize, title
    );
    unsafe {
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
    }
    let mut message = MSG {
        hwnd: ptr::null_mut(),
        message: 0,
        wParam: 0,
        lParam: 0,
        time: 0,
        pt: POINT { x: 0, y: 0 },
    };
    loop {
        let result = unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) };
        if result <= 0 {
            break;
        }
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
