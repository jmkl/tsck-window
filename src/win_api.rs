#![allow(unused)]
use anyhow::{Context, Result};
use std::{
    ffi::{OsString, c_void},
    os::windows::ffi::OsStringExt,
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, FALSE, HANDLE, HWND, LPARAM, RECT, TRUE},
        Graphics::{
            Dwm::{
                DWM_WINDOW_CORNER_PREFERENCE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
                DwmSetWindowAttribute,
            },
            Gdi::{
                EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST,
                MONITORINFO, MonitorFromWindow,
            },
        },
        System::Threading::{
            AttachThreadInput, GetCurrentThreadId, OpenProcess, PROCESS_ACCESS_RIGHTS,
            PROCESS_NAME_FORMAT, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
            QueryFullProcessImageNameW,
        },
        UI::{
            Input::KeyboardAndMouse::{SetActiveWindow, SetFocus},
            WindowsAndMessaging::{
                BringWindowToTop, EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowRect,
                GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, HWND_NOTOPMOST,
                HWND_TOP, HWND_TOPMOST, IsIconic, IsWindow, IsWindowVisible, IsZoomed,
                MONITORINFOF_PRIMARY, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, SW_SHOW, SWP_NOMOVE,
                SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow, SetWindowPos, ShowWindow,
            },
        },
    },
    core::{BOOL, PWSTR},
};

use crate::wh_handler::TOOLBAR_HEIGHT;
pub static PADDING: i32 = 0;

pub fn move_window(hwnd: HWND, x: i32, y: i32) {
    if unsafe { IsWindow(Some(hwnd)) } == FALSE {
        return;
    }
    unsafe { SetWindowPos(hwnd, Some(HWND_TOP), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER) };
}
pub fn resize_window(hwnd: HWND, width: i32, height: i32) {
    if unsafe { IsWindow(Some(hwnd)) } == FALSE {
        return;
    }
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOP),
            0,
            0,
            width,
            height,
            SWP_NOMOVE | SWP_NOZORDER,
        )
    };
}
pub fn disable_rounding(hwnd: HWND) -> Result<()> {
    unsafe {
        let pref = DWMWCP_DONOTROUND;

        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as _,
            std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        )?;

        Ok(())
    }
}
pub fn resize_and_move_window(hwnd: HWND, width: i32, height: i32, x: i32, y: i32) {
    if unsafe { IsWindow(Some(hwnd)) } == FALSE {
        return;
    }
    disable_rounding(hwnd);
    // unsafe { ShowWindow(hwnd, SW_RESTORE) };
    let w = (width - PADDING * 2).max(0);
    let h = (height - PADDING * 2).max(0);
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOP),
            x + PADDING,
            y + PADDING,
            w,
            h,
            SWP_NOZORDER,
        )
    };
}

#[derive(Debug)]
pub struct MonitorInfo {
    pub top: i32,
    pub left: i32,
    pub bottom: i32,
    pub right: i32,
    pub width: i32,
    pub height: i32,
}
impl std::fmt::Display for MonitorInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{\n\tt:{},\n\tb:{},\n\tr:{},\n\tl:{},\n\tw:{},\n\th:{}\n}}",
            self.top, self.bottom, self.right, self.left, self.width, self.height
        )
    }
}

pub fn get_monitor_info(hwnd: HWND) {
    unsafe {
        // Get monitor handle from window
        let hmonitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);

        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };

        GetMonitorInfoW(hmonitor, &mut mi as *mut _ as *mut _).unwrap();

        let monitor = mi.rcMonitor; // full monitor area
        let work = mi.rcWork; // minus taskbar
        let monitor_data = MonitorInfo {
            top: monitor.top,
            left: monitor.left,
            bottom: monitor.bottom,
            right: monitor.right,
            width: monitor.right - monitor.left,
            height: monitor.bottom - monitor.top,
        };
        let work_data = MonitorInfo {
            top: work.top,
            left: work.left,
            bottom: work.bottom,
            right: work.right,
            width: work.right - work.left,
            height: work.bottom - work.top,
        };
        println!("Full Monitor:\n{}", monitor_data);

        println!("Work Area (minus taskbar):\n{}", work_data);
    }
}

pub fn get_all_monitors() -> Vec<MonitorInfo> {
    let mut v: Vec<MonitorInfo> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            Some(HDC(0 as *mut c_void)),
            None,
            Some(monitor_enum_proc),
            LPARAM(&mut v as *mut _ as isize),
        );
    }
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
        // let monitor = mi.rcMonitor;
        let monitor = mi.rcWork;

        let monitor = MonitorInfo {
            top: monitor.top + TOOLBAR_HEIGHT,
            left: monitor.left,
            bottom: monitor.bottom,
            right: monitor.right,
            width: monitor.right - monitor.left,
            height: monitor.bottom - monitor.top - TOOLBAR_HEIGHT,
        };
        monitors.push(monitor);
    }

    true.into()
}

pub fn transform(hwnd: HWND, x: i32, y: i32, width: i32, height: i32) {
    resize_and_move_window(hwnd, width, height, x, y);
}
pub fn toggle_top_most(hwnd: HWND, top_most: bool) {
    if unsafe { IsWindow(Some(hwnd)) } == FALSE {
        return;
    }
    let hwnd_after = match top_most {
        true => HWND_TOPMOST,
        false => HWND_NOTOPMOST,
    };
    unsafe { SetWindowPos(hwnd, Some(hwnd_after), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE) };
}
pub fn maximize_window(hwnd: HWND) {
    unsafe {
        if IsWindow(Some(hwnd)) == FALSE {
            return;
        }
        if IsZoomed(hwnd) == TRUE {
            ShowWindow(hwnd, SW_RESTORE);
        } else {
            ShowWindow(hwnd, SW_MAXIMIZE);
        }
    }
}
pub fn bring_to_front(hwnd: HWND) {
    unsafe {
        if IsWindow(Some(hwnd)) == FALSE {
            return;
        }
        if IsIconic(hwnd) == TRUE {
            ShowWindow(hwnd, SW_RESTORE);
        }
        if SetForegroundWindow(hwnd).as_bool() == TRUE {
            return;
        }
        force_bring_to_front(hwnd);
    }
}

fn force_bring_to_front(hwnd: HWND) {
    unsafe {
        let hwnd_target: HWND = hwnd;
        let hwnd_foreground = GetForegroundWindow();

        let current_thread = GetCurrentThreadId();
        let fg_thread = GetWindowThreadProcessId(hwnd_foreground, None);

        AttachThreadInput(current_thread, fg_thread, true);

        SetForegroundWindow(hwnd_target);
        SetFocus(Some(hwnd_target));
        BringWindowToTop(hwnd_target);
        AttachThreadInput(current_thread, fg_thread, false);
    }
}

pub fn is_minimized(hwnd: HWND) -> bool {
    { (unsafe { IsIconic(hwnd) }) == TRUE }
}
pub fn is_visible(hwnd: HWND) -> bool {
    { (unsafe { IsWindowVisible(hwnd) }) == TRUE }
}
