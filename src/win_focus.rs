//! Windows can leave our active window without the keyboard focus (after a
//! popped-out pane or a file dialog closes, or a restore from the taskbar).
//! Windows then turns every key press into a "system" key: it plays the error
//! sound, the typed text never arrives, and egui hides the text cursor because
//! the window reports it has no focus. Clicking the window does not fix it,
//! since it is already active. This hands the focus back to that window.

/// Gives the keyboard focus back to our foreground window when no window has it.
/// Call once per frame; it costs two system calls and does nothing elsewhere.
#[cfg(windows)]
pub fn repair() {
    use std::ffi::c_void;
    type Hwnd = *mut c_void;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetForegroundWindow() -> Hwnd;
        fn GetFocus() -> Hwnd;
        fn SetFocus(hwnd: Hwnd) -> Hwnd;
        fn GetWindowThreadProcessId(hwnd: Hwnd, process_id: *mut u32) -> u32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThreadId() -> u32;
    }

    // SAFETY: plain Win32 calls with no pointers kept; a null process id pointer
    // is allowed, and SetFocus is only called on a window of this thread.
    unsafe {
        let window = GetForegroundWindow();
        if window.is_null() || !GetFocus().is_null() {
            return;
        }
        if GetWindowThreadProcessId(window, std::ptr::null_mut()) == GetCurrentThreadId() {
            SetFocus(window);
        }
    }
}

#[cfg(not(windows))]
pub fn repair() {}
