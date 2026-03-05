use anyhow::{Result, anyhow};
use std::ffi::c_void;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MonitorFromPoint, MonitorFromWindow,
};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
use windows::{
    Win32::Foundation::{HWND, LPARAM},
    core::BOOL,
};

#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub index: usize,
    pub handle: isize,
    pub top: i32,
    pub left: i32,
    pub bottom: i32,
    pub right: i32,
    pub width: i32,
    pub height: i32,
    pub is_primary: bool,
}

pub struct MonitorManager {
    monitors: Vec<MonitorInfo>,
}
impl MonitorManager {
    pub fn new() -> Self {
        Self {
            monitors: Self::fetch_all_monitors(),
        }
    }
    pub fn next(&self, monitor_index: usize) -> Result<&MonitorInfo> {
        let next_monitor = (monitor_index + 1) % self.monitors.len();

        let monitor = self
            .monitors
            .iter()
            .find(|m| m.index == next_monitor)
            .ok_or(anyhow!("Cant find current monitor"))?;
        Ok(monitor)
    }
    pub fn get_app_monitor(&self, hwnd: HWND) -> Result<&MonitorInfo> {
        let current = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
        let position = self
            .monitors
            .iter()
            .position(|m| HMONITOR(m.handle as *mut c_void) == current)
            .ok_or(anyhow!("Cant find monitor for app {}", hwnd.0 as isize))?;
        let monitor = &self.monitors[position];
        Ok(monitor)
    }
    pub fn get_monitor_in_cursor(&self) -> Result<&MonitorInfo> {
        let mut point = POINT { x: 0, y: 0 };
        _ = unsafe { GetCursorPos(&mut point) };
        let hmonitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
        let position = self
            .monitors
            .iter()
            .position(|m| HMONITOR(m.handle as *mut c_void) == hmonitor)
            .ok_or(anyhow!("Cant find monitor at current cursor"))?;
        let monitor = &self.monitors[position];
        Ok(monitor)
    }

    pub fn fetch_all_monitors() -> Vec<MonitorInfo> {
        let mut v: Vec<MonitorInfo> = Vec::new();
        unsafe {
            _ = EnumDisplayMonitors(
                Some(HDC(0 as *mut c_void)),
                None,
                Some(Self::monitor_enum_proc),
                LPARAM(&mut v as *mut _ as isize),
            );
        };
        v.sort_by_key(|m| m.left);
        v
    }
    unsafe extern "system" fn monitor_enum_proc(
        hmonitor: HMONITOR,
        _hdc: HDC,
        _lprc_monitor: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        let monitors = unsafe { &mut *(lparam.0 as *mut Vec<MonitorInfo>) };

        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };

        if unsafe { GetMonitorInfoW(hmonitor, &mut mi as *mut _ as *mut _).as_bool() } {
            let monitor = mi.rcWork;
            monitors.push(MonitorInfo {
                handle: hmonitor.0 as isize,
                top: monitor.top,
                index: monitors.len(),
                left: monitor.left,
                bottom: monitor.bottom,
                right: monitor.right,
                width: monitor.right - monitor.left,
                height: monitor.bottom - monitor.top,
                is_primary: mi.dwFlags & 1 != 0,
            })
        }

        true.into()
    }
}
