use arboard::Clipboard;
use tauri::{App, AppHandle, Runtime, WebviewWindow};

pub fn setup_main_window<R: Runtime>(app: &mut App<R>, window: &WebviewWindow<R>) {
    use objc2_app_kit::{NSWindow, NSWindowButton, NSWindowCollectionBehavior};
    use tauri::ActivationPolicy;

    app.set_activation_policy(ActivationPolicy::Accessory);

    let Ok(ns_window) = window.ns_window() else {
        return;
    };

    unsafe {
        let ns_window = &*(ns_window as *mut NSWindow);

        let behavior = ns_window.collectionBehavior()
            | NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Transient
            | NSWindowCollectionBehavior::IgnoresCycle;
        ns_window.setCollectionBehavior(behavior);

        if let Some(close_button) =
            ns_window.standardWindowButton(NSWindowButton::NSWindowCloseButton)
        {
            close_button.setHidden(true);
        }
        if let Some(minimize_button) =
            ns_window.standardWindowButton(NSWindowButton::NSWindowMiniaturizeButton)
        {
            minimize_button.setHidden(true);
        }
        if let Some(zoom_button) =
            ns_window.standardWindowButton(NSWindowButton::NSWindowZoomButton)
        {
            zoom_button.setHidden(true);
        }
    }
}

pub fn show_quick_window_no_activate<R: Runtime>(
    _app: &AppHandle<R>,
    window: &WebviewWindow<R>,
) -> tauri::Result<()> {
    use objc2_app_kit::{NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior};

    let ns_window = window.ns_window()? as *mut NSWindow;

    unsafe {
        let ns_window = &*ns_window;
        ns_window.setLevel(NSStatusWindowLevel);
        ns_window.setAcceptsMouseMovedEvents(true);

        let behavior = ns_window.collectionBehavior()
            | NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Transient
            | NSWindowCollectionBehavior::IgnoresCycle;
        ns_window.setCollectionBehavior(behavior);
    }

    window.show()?;

    unsafe {
        let ns_window = &*ns_window;
        ns_window.orderFrontRegardless();
    }

    window.set_focus()?;

    Ok(())
}

pub fn hide_quick_window<R: Runtime>(window: &WebviewWindow<R>) {
    let _ = window.hide();
}

pub fn restore_window_activation<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<()> {
    use objc2_app_kit::{NSNormalWindowLevel, NSWindow};

    let ns_window = window.ns_window()? as *mut NSWindow;
    unsafe {
        let ns_window = &*ns_window;
        ns_window.setLevel(NSNormalWindowLevel);
    }

    window.set_focusable(true)?;
    Ok(())
}

pub fn paste_clipboard_item<R: Runtime>(
    app_handle: &AppHandle<R>,
    _item_type: &str,
    _text: Option<&str>,
) {
    let _ = app_handle.hide();

    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(120));
        send_maccy_style_paste();
    });
}

fn send_maccy_style_paste() {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, KeyCode};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        return;
    };

    let flags =
        CGEventFlags::from_bits_truncate(CGEventFlags::CGEventFlagCommand.bits() | 0x000008);

    if let Ok(event_down) = CGEvent::new_keyboard_event(source.clone(), KeyCode::ANSI_V, true) {
        event_down.set_flags(flags);
        event_down.post(CGEventTapLocation::Session);
    }

    if let Ok(event_up) = CGEvent::new_keyboard_event(source, KeyCode::ANSI_V, false) {
        event_up.set_flags(flags);
        event_up.post(CGEventTapLocation::Session);
    }
}

pub fn write_file_path_to_clipboard(_clipboard: &mut Clipboard, path: &str) -> Result<(), String> {
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::{NSArray, NSString};

    unsafe {
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        let ns_str = NSString::from_str(path);
        let array = NSArray::from_id_slice(&[ns_str]);
        let filenames_type = NSString::from_str("NSFilenamesPboardType");
        pb.setPropertyList_forType(&array, &filenames_type);
    }

    Ok(())
}

pub fn clipboard_change_count() -> isize {
    use objc2_app_kit::NSPasteboard;

    let pb = unsafe { NSPasteboard::generalPasteboard() };
    unsafe { pb.changeCount() }
}

pub fn read_clipboard_file_paths() -> Vec<String> {
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::{NSArray, NSString};

    let pb = unsafe { NSPasteboard::generalPasteboard() };
    let filenames_type = NSString::from_str("NSFilenamesPboardType");
    let Some(plist_obj) = pb.propertyListForType(&filenames_type) else {
        return Vec::new();
    };

    let array_ptr = objc2::rc::Id::as_ptr(&plist_obj) as *const NSArray<NSString>;
    let array = unsafe { &*array_ptr };
    let mut paths = Vec::new();

    for i in 0..array.count() {
        let ns_str = unsafe { array.objectAtIndex(i) };
        paths.push(ns_str.to_string());
    }

    paths
}

pub fn get_frontmost_app() -> Option<String> {
    use objc2_app_kit::NSWorkspace;

    let workspace = unsafe { NSWorkspace::sharedWorkspace() };
    let app = unsafe { workspace.frontmostApplication()? };

    if let Some(bundle_id) = unsafe { app.bundleIdentifier() } {
        return Some(bundle_id.to_string());
    }

    if let Some(name) = unsafe { app.localizedName() } {
        return Some(name.to_string());
    }

    None
}
