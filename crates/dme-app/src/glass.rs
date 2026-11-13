use raw_window_handle::HasWindowHandle;

#[cfg(windows)]
pub fn frost(window: &impl HasWindowHandle, night: bool) -> bool {
    use std::ffi::c_void;

    use raw_window_handle::RawWindowHandle;
    use windows_sys::Win32::Foundation::{HWND, S_OK};
    use windows_sys::Win32::Graphics::Dwm::{
        DWM_SYSTEMBACKDROP_TYPE, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
    };
    use windows_sys::Win32::UI::Controls::MARGINS;

    let Ok(handle) = window.window_handle() else { return false };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else { return false };
    let hwnd = win32.hwnd.get() as HWND;
    let dark: i32 = night.into();
    let everywhere = MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 };
    let backdrop: DWM_SYSTEMBACKDROP_TYPE = DWMSBT_TRANSIENTWINDOW;
    unsafe {
        DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, (&raw const dark).cast::<c_void>(), size_of::<i32>() as u32);
        let set = DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE as u32, (&raw const backdrop).cast::<c_void>(), size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32);
        set == S_OK && DwmExtendFrameIntoClientArea(hwnd, &everywhere) == S_OK
    }
}

#[cfg(not(windows))]
pub fn frost(_window: &impl HasWindowHandle, _night: bool) -> bool {
    false
}
