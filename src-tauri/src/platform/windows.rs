use arboard::Clipboard;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use tauri::{App, AppHandle, Manager, Runtime, WebviewWindow};

static QUICK_WINDOW_DISMISS_MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);
static QUICK_WINDOW_OUTSIDE_CLICKED: AtomicBool = AtomicBool::new(false);
static QUICK_WINDOW_HWND: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

pub fn setup_main_window<R: Runtime>(_app: &mut App<R>, window: &WebviewWindow<R>) {
    let _ = window.set_decorations(false);
}

pub fn show_quick_window_no_activate<R: Runtime>(
    app: &AppHandle<R>,
    window: &WebviewWindow<R>,
) -> tauri::Result<()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    let hwnd = window.hwnd()?.0 as windows_sys::Win32::Foundation::HWND;

    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let no_activate_style = ex_style | WS_EX_NOACTIVATE as isize | WS_EX_TOOLWINDOW as isize;

        if no_activate_style != ex_style {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, no_activate_style);
        }
    }

    window.set_focusable(false)?;
    window.show()?;
    set_window_topmost(window, true)?;
    start_quick_window_dismiss_monitor(app.clone());

    Ok(())
}

pub fn hide_quick_window<R: Runtime>(window: &WebviewWindow<R>) {
    let _ = set_window_topmost(window, false);
    let _ = window.hide();
}

pub fn restore_window_activation<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    };

    let hwnd = window.hwnd()?.0 as windows_sys::Win32::Foundation::HWND;
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let active_style = ex_style & !(WS_EX_NOACTIVATE as isize);
        if active_style != ex_style {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, active_style);
        }
    }

    window.set_focusable(true)?;
    Ok(())
}

pub fn paste_clipboard_item<R: Runtime>(
    _app_handle: &AppHandle<R>,
    _item_type: &str,
    _text: Option<&str>,
) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL, VK_V,
    };

    fn keyboard_input(vk: u16, key_up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if key_up { KEYEVENTF_KEYUP } else { 0 },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    std::thread::sleep(std::time::Duration::from_millis(80));

    let inputs = [
        keyboard_input(VK_CONTROL, false),
        keyboard_input(VK_V, false),
        keyboard_input(VK_V, true),
        keyboard_input(VK_CONTROL, true),
    ];

    unsafe {
        let _ = SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

pub fn write_file_path_to_clipboard(clipboard: &mut Clipboard, path: &str) -> Result<(), String> {
    clipboard.set_text(path).map_err(|e| e.to_string())
}

#[allow(dead_code)]
pub fn clipboard_change_count() -> isize {
    0
}

#[allow(dead_code)]
pub fn read_clipboard_file_paths() -> Vec<String> {
    Vec::new()
}

pub fn get_frontmost_app() -> Option<String> {
    None
}

fn set_window_topmost<R: Runtime>(
    window: &tauri::WebviewWindow<R>,
    topmost: bool,
) -> tauri::Result<()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SWP_SHOWWINDOW,
    };

    let hwnd = window.hwnd()?.0 as windows_sys::Win32::Foundation::HWND;
    let insert_after = if topmost {
        HWND_TOPMOST
    } else {
        HWND_NOTOPMOST
    };

    unsafe {
        SetWindowPos(
            hwnd,
            insert_after,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }

    Ok(())
}

fn start_quick_window_dismiss_monitor<R: Runtime>(app: AppHandle<R>) {
    if QUICK_WINDOW_DISMISS_MONITOR_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    std::thread::spawn(move || {
        let mut was_escape_down = false;
        let mut has_seen_visible = false;
        let started_at = std::time::Instant::now();
        QUICK_WINDOW_OUTSIDE_CLICKED.store(false, Ordering::SeqCst);
        let mouse_hook_thread = start_quick_window_mouse_hook_thread();

        loop {
            let Some(window) = app.get_webview_window("main") else {
                break;
            };

            let is_visible = window.is_visible().unwrap_or(false);
            if is_visible {
                has_seen_visible = true;
                if let Ok(hwnd) = window.hwnd() {
                    QUICK_WINDOW_HWND.store(hwnd.0 as _, Ordering::SeqCst);
                }
            } else if has_seen_visible || started_at.elapsed() > std::time::Duration::from_secs(1) {
                break;
            }

            if is_visible && should_dismiss_quick_window(&mut was_escape_down) {
                let _ = set_window_topmost(&window, false);
                let _ = window.hide();
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(35));
        }

        stop_quick_window_mouse_hook_thread(mouse_hook_thread);
        QUICK_WINDOW_HWND.store(std::ptr::null_mut(), Ordering::SeqCst);
        QUICK_WINDOW_OUTSIDE_CLICKED.store(false, Ordering::SeqCst);
        QUICK_WINDOW_DISMISS_MONITOR_RUNNING.store(false, Ordering::SeqCst);
    });
}

fn should_dismiss_quick_window(was_escape_down: &mut bool) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE};

    let escape_down = unsafe { GetAsyncKeyState(VK_ESCAPE as i32) } < 0;
    let escape_pressed = escape_down && !*was_escape_down;
    *was_escape_down = escape_down;

    escape_pressed || QUICK_WINDOW_OUTSIDE_CLICKED.swap(false, Ordering::SeqCst)
}

struct QuickMouseHookThread {
    thread_id: u32,
    handle: Option<std::thread::JoinHandle<()>>,
}

fn start_quick_window_mouse_hook_thread() -> Option<QuickMouseHookThread> {
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        let thread_id = unsafe {
            use windows_sys::Win32::System::Threading::GetCurrentThreadId;
            GetCurrentThreadId()
        };

        let hook = install_quick_window_mouse_hook();
        let _ = tx.send(if hook.is_null() {
            None
        } else {
            Some(thread_id)
        });

        if hook.is_null() {
            return;
        }

        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                DispatchMessageW, GetMessageW, TranslateMessage, UnhookWindowsHookEx, MSG,
            };

            let mut msg = std::mem::zeroed::<MSG>();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            UnhookWindowsHookEx(hook);
        }
    });

    match rx.recv_timeout(std::time::Duration::from_millis(500)) {
        Ok(Some(thread_id)) => Some(QuickMouseHookThread {
            thread_id,
            handle: Some(handle),
        }),
        _ => {
            let _ = handle.join();
            None
        }
    }
}

fn stop_quick_window_mouse_hook_thread(mouse_hook_thread: Option<QuickMouseHookThread>) {
    let Some(mut mouse_hook_thread) = mouse_hook_thread else {
        return;
    };

    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
        PostThreadMessageW(mouse_hook_thread.thread_id, WM_QUIT, 0, 0);
    }

    if let Some(handle) = mouse_hook_thread.handle.take() {
        let _ = handle.join();
    }
}

fn install_quick_window_mouse_hook() -> windows_sys::Win32::UI::WindowsAndMessaging::HHOOK {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowsHookExW, WH_MOUSE_LL};

    unsafe {
        SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(quick_window_mouse_hook_proc),
            GetModuleHandleW(std::ptr::null()),
            0,
        )
    }
}

unsafe extern "system" fn quick_window_mouse_hook_proc(
    code: i32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::Foundation::{RECT, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetWindowRect, MSLLHOOKSTRUCT, WM_LBUTTONDOWN,
    };

    if code >= 0 && wparam == WM_LBUTTONDOWN as WPARAM {
        let hwnd = QUICK_WINDOW_HWND.load(Ordering::SeqCst);
        if !hwnd.is_null() {
            let hook = &*(lparam as *const MSLLHOOKSTRUCT);
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };

            if GetWindowRect(hwnd, &mut rect) != 0 {
                let point = hook.pt;
                let is_outside = point.x < rect.left
                    || point.x >= rect.right
                    || point.y < rect.top
                    || point.y >= rect.bottom;

                if is_outside {
                    QUICK_WINDOW_OUTSIDE_CLICKED.store(true, Ordering::SeqCst);
                }
            }
        }
    }

    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}
