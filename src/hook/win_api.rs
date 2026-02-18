#![allow(unused)]
use crate::hook::api::Hwnd;
use crate::hook::app_info::{AppPosition, AppSize};
use crate::hook::border::BorderManager;
use crate::hook::win_event::WindowEvent;
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

pub static TOOLBAR_HEIGHT: i32 = 0;
pub static APP_WINDOW_PADDING: i32 = 0;

pub static WINEVENT_CHANNEL: OnceLock<(
    Sender<(WindowEvent, AppWindow)>,
    Receiver<(WindowEvent, AppWindow)>,
)> = OnceLock::new();

fn hook_channel() -> &'static (
    Sender<(WindowEvent, AppWindow)>,
    Receiver<(WindowEvent, AppWindow)>,
) {
    WINEVENT_CHANNEL.get_or_init(|| flume::unbounded())
}
fn channel_sender() -> Sender<(WindowEvent, AppWindow)> {
    hook_channel().0.clone()
}
pub fn channel_receiver() -> Receiver<(WindowEvent, AppWindow)> {
    hook_channel().1.clone()
}
pub fn channel_send(event: WindowEvent, app_window: AppWindow) {
    if let Err(err) = channel_sender().send((event, app_window)) {
        eprintln!("failed to send event {err} {event:?}")
    }
}
fn init_border_manager() {
    std::thread::spawn(move || {
        let manager = { BORDER_MANAGER.lock().clone() };
        manager.run_message_loop();
    });
}
pub fn init_winhook() {
    _ = hook_channel();
    init_border_manager();
    std::thread::spawn(|| {
        // Initiate all active app into the applists
        if let Err(err) = unsafe { EnumWindows(Some(init_applist), LPARAM(0)) } {
            eprintln!("Error Listing {err}")
        }

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
                // WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
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
extern "system" fn init_applist(hwnd: HWND, lparam: LPARAM) -> BOOL {
    match unsafe { IsWindowVisible(hwnd) } == FALSE {
        true => return TRUE,
        false => (),
    }
    let app_window = AppWindow::from(hwnd);
    channel_send(WindowEvent::ObjectCreate, app_window);
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
        };
        if matches!(event, EVENT_OBJECT_DESTROY) {
            channel_send(WindowEvent::ObjectDestroy, app_window);
        }
        if IsWindowVisible(hwnd).as_bool() == false {
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
        if let Ok(event) = crate::hook::win_event::WindowEvent::from_str(
            crate::hook::win_event::parse_event(event),
        ) {
            channel_send(event, app_window);
        }
    }
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
            handle: hmonitor.0 as isize,
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
        .ok();

        if process_handle.is_none() {
            return None;
        }

        // Query executable path
        let mut path_buffer: Vec<u16> = vec![0; 1024];
        let mut size: u32 = path_buffer.len() as u32;

        let result = QueryFullProcessImageNameW(
            process_handle?,
            PROCESS_NAME_FORMAT(0),
            PWSTR(path_buffer.as_mut_ptr()),
            &mut size,
        )
        .ok();

        _ = CloseHandle(process_handle?);

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
        let class_name = OsString::from_wide(&buffer[..copied as usize]);
        Some(class_name.to_string_lossy().into_owned())
    } else {
        None
    }
}

pub(crate) fn is_foreground(hwnd: HWND) -> bool {
    let fg = unsafe { GetForegroundWindow() };
    fg == hwnd
}
pub(crate) fn get_rect(hwnd: HWND) -> (AppSize, AppPosition) {
    let rect = unsafe {
        let mut rect = RECT::default();
        if let Err(err) = GetWindowRect(hwnd, &mut rect) {}
        rect
    };
    let x = rect.left;
    let y = rect.top;
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    (AppSize { width, height }, AppPosition { x, y })
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
    (unsafe {
        _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut _ as *mut _,
            std::mem::size_of::<RECT>() as u32,
        );
    });

    rect.left -= thickness;
    rect.top -= thickness;
    rect.right += thickness;
    rect.bottom += thickness;

    let x = rect.left;
    let y = rect.top;
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    Rect {
        l: rect.left,
        t: rect.top,
        r: rect.right,
        b: rect.bottom,
        w: width,
        h: height,
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
pub fn get_monitor_index(hwnd: HWND, monitors: &[MonitorInfo]) -> Option<usize> {
    let current = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    monitors
        .iter()
        .position(|m| HMONITOR(m.handle as *mut c_void) == current)
}

fn disable_rounded_corner(hwnd: HWND) {
    unsafe {
        let pref = DWMWCP_DONOTROUND;

        DwmSetWindowAttribute(
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
    //Disable Rounded Corner
    if disable_rounded {
        disable_rounded_corner(hwnd)
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
    let rect = get_dwm_rect(hwnd, 0);
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
    let rect = get_dwm_rect(hwnd, 0);
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

pub(crate) fn bring_to_front(hwnd: HWND) {
    unsafe {
        if IsWindow(Some(hwnd)) == FALSE {
            eprintln!("This is not a window");
            return;
        }
        if IsIconic(hwnd) == TRUE {
            eprintln!("ICONIC, RESTORE");
            ShowWindow(hwnd, SW_RESTORE);
        }
        let res = SetForegroundWindow(hwnd);
        eprintln!("SET FORGORUND {:?} {}", hwnd.0, res.as_bool());
        force_bring_to_front(hwnd);
    }
}
pub(crate) fn get_app_position(hwnd: HWND) -> AppPosition {
    get_rect(hwnd).1
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
