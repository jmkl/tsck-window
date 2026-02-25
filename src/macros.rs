#[macro_export]
macro_rules! h {
    ($hwnd:expr) => {
        windows::Win32::Foundation::HWND($hwnd as *mut std::ffi::c_void)
    };
}
