use crate::overlay::app_info::{AppPosition, AppSize};
use crate::overlay::monitor_info::StatusbarMonitorInfo;
use crate::overlay::win_event::WinEvent;
use crate::overlay::{app_info::AppInfo, app_window::AppWindow};
use anyhow::Context;
use flume::{Receiver, Sender};
use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::str::FromStr;
use std::time::Duration;
use std::{collections::HashMap, sync::OnceLock};
use std::{
    ffi::{OsString, c_void},
    os::windows::ffi::OsStringExt,
};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        System::Threading::*,
        UI::{Input::KeyboardAndMouse::SetFocus, WindowsAndMessaging::*},
    },
    core::{BOOL, PWSTR},
};

pub static APP_WINDOW_PADDING: i32 = 0;
// ---------------------------------------------------------------------------
// Process / window info helpers
// ---------------------------------------------------------------------------

pub(crate) fn get_process_path(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut process_id: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        if process_id == 0 {
            return None;
        }

        let process_handle = OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
            false,
            process_id,
        )
        .ok()?;

        let mut path_buffer: Vec<u16> = vec![0; 1024];
        let mut size: u32 = path_buffer.len() as u32;

        let result = QueryFullProcessImageNameW(
            process_handle,
            PROCESS_NAME_FORMAT(0),
            PWSTR(path_buffer.as_mut_ptr()),
            &mut size,
        )
        .ok();

        let _ = CloseHandle(process_handle);

        if result.is_some() && size > 0 {
            path_buffer.truncate(size as usize);
            Some(
                OsString::from_wide(&path_buffer)
                    .to_string_lossy()
                    .into_owned(),
            )
        } else {
            None
        }
    }
}

pub(crate) fn get_app_class(hwnd: HWND) -> Option<String> {
    let mut buffer: [u16; 256] = [0; 256];
    let copied = unsafe { GetClassNameW(hwnd, &mut buffer) };
    if copied > 0 {
        Some(
            OsString::from_wide(&buffer[..copied as usize])
                .to_string_lossy()
                .into_owned(),
        )
    } else {
        None
    }
}
pub fn is_window_maximized(hwnd: HWND) -> anyhow::Result<bool> {
    unsafe {
        let mut placement = WINDOWPLACEMENT::default();
        placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;

        GetWindowPlacement(hwnd, &mut placement)?;
        let result = placement.showCmd == SW_SHOWMAXIMIZED.0 as u32;
        Ok(result)
    }
}
pub(crate) fn is_foreground(hwnd: HWND) -> bool {
    let result = unsafe { GetForegroundWindow() } == hwnd;
    result
}

pub(crate) fn get_rect(hwnd: HWND) -> (AppSize, AppPosition) {
    let rect = unsafe {
        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);
        rect
    };
    (
        AppSize {
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        },
        AppPosition {
            x: rect.left,
            y: rect.top,
        },
    )
}

#[derive(Debug)]
pub struct Rect {
    pub l: i32,
    pub t: i32,
    pub r: i32,
    pub b: i32,
    pub w: i32,
    pub h: i32,
}

pub(crate) fn get_dwm_rect(hwnd: HWND, thickness: i32) -> Rect {
    let mut rect = RECT::default();
    unsafe {
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut _ as *mut _,
            std::mem::size_of::<RECT>() as u32,
        );
    }

    rect.left -= thickness;
    rect.top -= thickness;
    rect.right += thickness;
    rect.bottom += thickness;

    Rect {
        l: rect.left,
        t: rect.top,
        r: rect.right,
        b: rect.bottom,
        w: rect.right - rect.left,
        h: rect.bottom - rect.top,
    }
}
pub fn is_maximized(hwnd: isize) -> bool {
    unsafe { IsZoomed(HWND(hwnd as *mut c_void)).as_bool() }
}
pub fn get_rect_padding(hwnd: isize) -> (i32, i32) {
    let dwm_rect = get_dwm_rect(crate::hwnd!(hwnd), 0);
    let rect = get_rect(crate::hwnd!(hwnd));
    let x = rect.0.width - dwm_rect.w;
    let y = rect.0.height - dwm_rect.h;
    (x, y)
}

pub(crate) fn get_app_title(hwnd: HWND) -> Option<String> {
    unsafe {
        let length = GetWindowTextLengthW(hwnd);
        if length == 0 {
            return None;
        }
        let mut buffer: Vec<u16> = vec![0; (length + 1) as usize];
        let copied = GetWindowTextW(hwnd, &mut buffer);

        if copied > 0 {
            buffer.truncate(copied as usize);
            Some(OsString::from_wide(&buffer).to_string_lossy().into_owned())
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Window manipulation
// ---------------------------------------------------------------------------

fn disable_rounded_corner(hwnd: HWND) {
    unsafe {
        let pref = DWMWCP_DONOTROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as _,
            std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
    }
}

pub(crate) fn set_app_size(hwnd: HWND, width: i32, height: i32) {
    if unsafe { IsWindow(Some(hwnd)) } == FALSE {
        return;
    }
    _ = unsafe { SetWindowPos(hwnd, None, 0, 0, width, height, SWP_NOMOVE | SWP_NOZORDER) };
}

pub(crate) fn set_app_position(hwnd: HWND, x: i32, y: i32) {
    if unsafe { IsWindow(Some(hwnd)) } == FALSE {
        return;
    }
    _ = unsafe { SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER) };
}

pub(crate) fn set_app_size_position(
    hwnd: HWND,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    disable_rounded: bool,
) {
    if unsafe { IsWindow(Some(hwnd)) } == FALSE {
        return;
    }
    if disable_rounded {
        disable_rounded_corner(hwnd);
    }
    _ = unsafe { ShowWindow(hwnd, SW_RESTORE) };
    let w = (width - APP_WINDOW_PADDING * 2).max(0);
    let h = (height - APP_WINDOW_PADDING * 2).max(0);
    println!("APP_SIZE {:?}", (x, y, w, h));
    _ = unsafe {
        SetWindowPos(
            hwnd,
            None,
            x + APP_WINDOW_PADDING,
            y + APP_WINDOW_PADDING,
            w,
            h,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
        )
    };
}

pub(crate) fn get_dwm_props(hwnd: HWND, width: i32, height: i32) -> Option<(i32, i32)> {
    if unsafe { IsWindow(Some(hwnd)) } == FALSE {
        return None;
    }
    let w = (width - APP_WINDOW_PADDING * 2).max(0);
    let h = (height - APP_WINDOW_PADDING * 2).max(0);
    Some((w, h))
}

pub(crate) fn set_border_size_position(
    hwnd: HWND,
    parent_hwnd: HWND,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) {
    if unsafe { IsWindow(Some(hwnd)) } == FALSE {
        return;
    }
    let w = (width - APP_WINDOW_PADDING * 2).max(0);
    let h = (height - APP_WINDOW_PADDING * 2).max(0);
    unsafe {
        _ = SetWindowPos(
            hwnd,
            Some(parent_hwnd),
            x + APP_WINDOW_PADDING,
            y + APP_WINDOW_PADDING,
            w,
            h,
            SWP_NOZORDER,
        );
    };
}

pub fn close_app(hwnd: HWND) -> anyhow::Result<()> {
    unsafe {
        PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0))
            .context("Failed to close an app")?
    };
    Ok(())
}

pub(crate) fn get_app_position(hwnd: HWND) -> AppPosition {
    get_rect(hwnd).1
}

pub(crate) fn bring_to_front(hwnd: HWND, border_hwnd: HWND) {
    force_to_front(hwnd, border_hwnd);
}

fn force_to_front(hwnd: HWND, border_hwnd: HWND) {
    unsafe {
        if IsWindow(Some(hwnd)) == FALSE {
            return;
        }
        if IsIconic(hwnd) == TRUE {
            _ = ShowWindow(hwnd, SW_RESTORE);
        }
        _ = ShowWindow(hwnd, SW_SHOW);

        // Temporarily make topmost so SetForegroundWindow reliably fires
        _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE,
        );
        _ = SetForegroundWindow(hwnd);
        // Remove topmost — stack just above the border overlay for this monitor
        _ = SetWindowPos(
            hwnd,
            Some(HWND_NOTOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE,
        );
        // SetWindowPos(hwnd, Some(border_hwnd), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
    }
}
pub fn force_border_to_front(hwnd: HWND) {
    _ = unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE,
        )
    };
}

fn force_bring_to_front(hwnd: HWND) {
    unsafe {
        let hwnd_foreground = GetForegroundWindow();
        let current_thread = GetCurrentThreadId();
        let fg_thread = GetWindowThreadProcessId(hwnd_foreground, None);

        _ = AttachThreadInput(current_thread, fg_thread, true);
        _ = SetForegroundWindow(hwnd);
        _ = SetFocus(Some(hwnd));
        _ = BringWindowToTop(hwnd);
        _ = AttachThreadInput(current_thread, fg_thread, false);
    }
}

pub fn get_monitor_index(hwnd: HWND, monitors: &[StatusbarMonitorInfo]) -> Option<usize> {
    let current = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    monitors
        .iter()
        .position(|m| HMONITOR(m.handle as *mut c_void) == current)
}

pub fn get_monitor_index_from_cursor(monitors: &[StatusbarMonitorInfo]) -> usize {
    let mut point = POINT { x: 0, y: 0 };
    _ = unsafe { GetCursorPos(&mut point) };
    let hmonitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    monitors
        .iter()
        .position(|m| HMONITOR(m.handle as *mut c_void) == hmonitor)
        .unwrap_or(0)
}

pub fn is_top_most(hwnd: HWND) -> bool {
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        (ex_style & WS_EX_TOPMOST.0) != 0
    }
}

pub fn toggle_top_most(hwnd: HWND, parent_hwnd: HWND) -> bool {
    let top_most = is_top_most(hwnd);
    let flag = if top_most {
        HWND_NOTOPMOST
    } else {
        HWND_TOPMOST
    };
    unsafe {
        _ = SetWindowPos(hwnd, Some(flag), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);

        _ = SetWindowPos(
            parent_hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
    !top_most
}

pub fn app_exist(hwnd: HWND) {
    let exits = unsafe { IsWindow(Some(hwnd)).as_bool() };
}
