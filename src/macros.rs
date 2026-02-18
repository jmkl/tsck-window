#[macro_export]
macro_rules! hwnd {
    ($self:expr) => {
        windows::Win32::Foundation::HWND($self as *mut std::ffi::c_void)
    };
}

#[macro_export]
macro_rules! is_hwnd {
    ($self:expr, |$yes:ident| $block:block) => {
        if let Some(hwnd) = $self.hwnd {
            let $yes = hwnd!(hwnd);
            $block
        }
    };
}

#[macro_export]
macro_rules! with_handler {
    ($handler:expr, |$hd:ident| $block:block) => {
        let mut $hd = $handler.lock();
        $block
    };
}
