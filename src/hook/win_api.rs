#![allow(unused)]
use crate::hook::api::Hwnd;
use crate::hook::app_info::{AppPosition, AppSize};
use crate::hook::border::BorderManager;
use crate::hook::win_event::WinEvent;
use crate::hook::{app_info::AppInfo, app_window::AppWindow};
use anyhow::{Context, Result};
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
        UI::{
            Input::KeyboardAndMouse::{SetActiveWindow, SetFocus},
            WindowsAndMessaging::*,
        },
    },
    core::{BOOL, PWSTR},
};

lazy_static! {
    pub static ref BORDER_MANAGER: Mutex<BorderManager> = Mutex::new(BorderManager::new());
    static ref APP_INFO_LIST: Mutex<HashMap<isize, AppInfo>> = Mutex::new(HashMap::new());
}

// static TOOLBAR_HEIGHT: i32 = 25;
pub static APP_WINDOW_PADDING: i32 = 0;

pub fn get_toolbar_height(monitor: usize) -> i32 {
    if monitor == 0 { 25 } else { 0 }
}

pub static WINEVENT_CHANNEL: OnceLock<(
    Sender<(WinEvent, AppWindow)>,
    Receiver<(WinEvent, AppWindow)>,
)> = OnceLock::new();

fn hook_channel() -> &'static (
    Sender<(WinEvent, AppWindow)>,
    Receiver<(WinEvent, AppWindow)>,
) {
    WINEVENT_CHANNEL.get_or_init(|| flume::unbounded())
}

fn channel_sender() -> Sender<(WinEvent, AppWindow)> {
    hook_channel().0.clone()
}

pub fn channel_receiver() -> Receiver<(WinEvent, AppWindow)> {
    hook_channel().1.clone()
}

pub fn channel_send(event: WinEvent, app_window: AppWindow) {
    if let Err(err) = channel_sender().send((event, app_window)) {
        eprintln!("failed to send event {err} {event:?}")
    }
}

fn init_border_manager() {
    std::thread::spawn(move || {
        // Clone gives us a shared Arc — the message loop runs here while
        // the original stays in the lazy_static for the rest of the app.
        let manager = { BORDER_MANAGER.lock().clone() };
        manager.run_message_loop();
    });
}

pub fn init_winhook() {
    _ = hook_channel();
    init_border_manager();
    std::thread::spawn(|| {
        // Enumerate all active windows into the app list
        if let Err(err) = unsafe { EnumWindows(Some(init_applist), LPARAM(0)) } {
            eprintln!("Error Listing {err}")
        }
        channel_send(WinEvent::Done, AppWindow::default());

        // Attach window hook
        unsafe {
            SetWinEventHook(
                EVENT_MIN,
                EVENT_MAX,
                None,
                Some(win_event_hook),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };

        let mut msg: MSG = MSG::default();
        loop {
            unsafe {
                if !GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            std::thread::sleep(Duration::ZERO);
        }
    });
}

// ---------------------------------------------------------------------------
// Monitor info
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct MonitorInfo {
    pub handle: isize,
    pub top: i32,
    pub left: i32,
    pub bottom: i32,
    pub right: i32,
    pub width: i32,
    pub height: i32,
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
        let len = monitors.len();
        monitors.push(MonitorInfo {
            handle: hmonitor.0 as isize,
            top: monitor.top + get_toolbar_height(len),
            left: monitor.left,
            bottom: monitor.bottom,
            right: monitor.right,
            width: monitor.right - monitor.left,
            height: monitor.bottom - monitor.top - get_toolbar_height(len),
        })
    }

    true.into()
}

// ---------------------------------------------------------------------------
// Window enumeration callbacks
// ---------------------------------------------------------------------------

extern "system" fn init_applist(hwnd: HWND, _lparam: LPARAM) -> BOOL {
    if unsafe { IsWindowVisible(hwnd) } == FALSE {
        return TRUE;
    }
    let app_window = AppWindow::from(hwnd);
    channel_send(WinEvent::ObjectCreate, app_window);
    TRUE
}

extern "system" fn win_event_hook(
    _win_event_hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _id_event_thread: u32,
    _dwms_event_time: u32,
) {
    unsafe {
        if id_object != OBJID_WINDOW.0 || id_child != 0 {
            return;
        }

        let app_window = AppWindow::from(hwnd);

        if GetAncestor(hwnd, GA_ROOTOWNER) != hwnd
            || GetWindowTextLengthW(hwnd) == 0
            || hwnd.is_invalid()
        {
            return;
        }

        if matches!(event, EVENT_OBJECT_DESTROY) {
            channel_send(WinEvent::ObjectDestroy, app_window);
            // return;
        }

        if !IsWindowVisible(hwnd).as_bool() {
            return;
        }

        let style = WINDOW_STYLE(GetWindowLongW(hwnd, GWL_STYLE) as u32);
        if !style.contains(WS_OVERLAPPEDWINDOW) {
            return;
        }

        let ex_style = WINDOW_EX_STYLE(GetWindowLongW(hwnd, GWL_EXSTYLE) as u32);
        if ex_style.contains(WS_EX_TOOLWINDOW) {
            return;
        }

        if let Ok(ev) = crate::hook::win_event::WinEvent::from_str(
            crate::hook::win_event::WinEvent::parse_event(event),
        ) {
            channel_send(ev, app_window);
        }
    }
}

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

/// Returns which index in `monitors` the given hwnd is currently on.
pub fn get_monitor_index(hwnd: HWND, monitors: &[MonitorInfo]) -> Option<usize> {
    let current = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    monitors
        .iter()
        .position(|m| HMONITOR(m.handle as *mut c_void) == current)
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
    unsafe { SetWindowPos(hwnd, None, 0, 0, width, height, SWP_NOMOVE | SWP_NOZORDER) };
}

pub(crate) fn set_app_position(hwnd: HWND, x: i32, y: i32) {
    if unsafe { IsWindow(Some(hwnd)) } == FALSE {
        return;
    }
    unsafe { SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER) };
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
    unsafe { ShowWindow(hwnd, SW_RESTORE) };
    let w = (width - APP_WINDOW_PADDING * 2).max(0);
    let h = (height - APP_WINDOW_PADDING * 2).max(0);
    unsafe {
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
        SetWindowPos(
            hwnd,
            Some(parent_hwnd),
            x + APP_WINDOW_PADDING,
            y + APP_WINDOW_PADDING,
            w,
            h,
            SWP_NOZORDER,
        )
    };
}

pub(crate) fn set_badge_position(
    hwnd: HWND,
    parent_hwnd: HWND,
    rect: Rect,
    width: i32,
    height: i32,
) {
    if unsafe { IsWindow(Some(hwnd)) } == FALSE {
        return;
    }
    let x = (rect.l - (width / 2)) + (rect.w / 2);
    let y = rect.t;
    unsafe {
        SetWindowPos(
            hwnd,
            Some(parent_hwnd),
            x + APP_WINDOW_PADDING,
            y + APP_WINDOW_PADDING + 5,
            width,
            height,
            SWP_NOZORDER,
        )
    };
}
pub fn get_monitor_index_from_cursor(monitors: &[MonitorInfo]) -> usize {
    let mut point = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut point) };
    let hmonitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    monitors
        .iter()
        .position(|m| HMONITOR(m.handle as *mut c_void) == hmonitor)
        .unwrap_or(0)
}

pub(crate) fn get_app_position(hwnd: HWND) -> AppPosition {
    get_rect(hwnd).1
}

/// Bring `hwnd` to the foreground, then stack it just above `border_hwnd`.
/// `border_hwnd` should be the border window that lives on the same monitor
/// as `hwnd` — obtain it via `BORDER_MANAGER.lock().hwnd_for(monitor_index)`.
pub(crate) fn bring_to_front(hwnd: HWND, border_hwnd: HWND) {
    force_to_front(hwnd, border_hwnd);
}

fn force_to_front(hwnd: HWND, border_hwnd: HWND) {
    unsafe {
        if IsWindow(Some(hwnd)) == FALSE {
            return;
        }
        if IsIconic(hwnd) == TRUE {
            ShowWindow(hwnd, SW_RESTORE);
        }
        ShowWindow(hwnd, SW_SHOW);

        // Temporarily make topmost so SetForegroundWindow reliably fires
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE,
        );
        SetForegroundWindow(hwnd);
        // Remove topmost — stack just above the border overlay for this monitor
        SetWindowPos(
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
    unsafe {
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

        AttachThreadInput(current_thread, fg_thread, true);
        SetForegroundWindow(hwnd);
        SetFocus(Some(hwnd));
        BringWindowToTop(hwnd);
        AttachThreadInput(current_thread, fg_thread, false);
    }
}
