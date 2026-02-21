use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, RECT},
        Graphics::Gdi::{
            EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTOPRIMARY,
            MONITORINFO, MonitorFromWindow,
        },
    },
    core::BOOL,
};

#[derive(Debug, Clone)]
pub struct StatusbarMonitorInfo {
    pub handle: isize,
    pub index: usize,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub is_primary: bool,
}
pub fn get_monitors() -> Vec<StatusbarMonitorInfo> {
    unsafe {
        let mut monitors: Vec<StatusbarMonitorInfo> = Vec::new();
        let ptr = &mut monitors as *mut Vec<StatusbarMonitorInfo>;

        unsafe extern "system" fn enum_proc(
            hmonitor: HMONITOR,
            _hdc: HDC,
            _rect: *mut RECT,
            lparam: LPARAM,
        ) -> BOOL {
            let monitors = unsafe { &mut *(lparam.0 as *mut Vec<StatusbarMonitorInfo>) };
            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if unsafe { GetMonitorInfoW(hmonitor, &mut mi).as_bool() } {
                monitors.push(StatusbarMonitorInfo {
                    handle: hmonitor.0 as isize,
                    index: monitors.len(),
                    x: mi.rcMonitor.left,
                    y: mi.rcMonitor.top,
                    width: mi.rcMonitor.right - mi.rcMonitor.left,
                    height: mi.rcMonitor.bottom - mi.rcMonitor.top,
                    is_primary: mi.dwFlags & 1 != 0,
                });
            }
            true.into()
        }

        let _ = EnumDisplayMonitors(None, None, Some(enum_proc), LPARAM(ptr as isize));
        monitors
    }
}
pub fn resolve_monitor_rect(hwnd: HWND, monitor_index: Option<usize>) -> RECT {
    if let Some(index) = monitor_index {
        let monitors = get_monitors();
        if let Some(m) = monitors.get(index) {
            return RECT {
                left: m.x,
                top: m.y,
                right: m.x + m.width,
                bottom: m.y + m.height,
            };
        }
    }
    unsafe {
        let hmonitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(hmonitor, &mut mi);
        mi.rcMonitor
    }
}
