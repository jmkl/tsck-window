use anyhow::Result;
use flume::{Receiver, Sender};
use ntek_derive::{NtekDes, NtekSer};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ffi::{OsString, c_void};
use std::os::windows::ffi::OsStringExt;
use std::str::FromStr;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Dwm::{
    DWM_WINDOW_CORNER_PREFERENCE, DWMWA_EXTENDED_FRAME_BOUNDS, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_DONOTROUND, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::Input::KeyboardAndMouse::{INPUT, INPUT_MOUSE, SendInput};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EVENT_MAX, EVENT_MIN, EVENT_OBJECT_DESTROY, GW_HWNDNEXT, GW_HWNDPREV,
    GetClassNameW, GetMessageW, GetTopWindow, GetWindow, GetWindowPlacement, GetWindowRect,
    GetWindowTextW, HWND_BOTTOM, HWND_DESKTOP, HWND_NOTOPMOST, HWND_TOPMOST, IsZoomed, MSG,
    OBJID_WINDOW, SW_HIDE, SW_RESTORE, SW_SHOWMAXIMIZED, SW_SHOWNOACTIVATE, SWP_ASYNCWINDOWPOS,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOREDRAW, SWP_NOSENDCHANGING, SWP_NOSIZE, SWP_NOZORDER,
    SWP_SHOWWINDOW, SetCursorPos, SetForegroundWindow, SetWindowPos, ShowWindow, TranslateMessage,
    WINDOW_EX_STYLE, WINDOW_STYLE, WINDOWPLACEMENT, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
    WS_EX_TOPMOST,
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, FALSE, HANDLE, HWND, LPARAM, TRUE},
        Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute},
        System::Threading::{
            OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
            QueryFullProcessImageNameW,
        },
        UI::WindowsAndMessaging::{
            EnumWindows, GA_ROOTOWNER, GWL_EXSTYLE, GWL_STYLE, GetAncestor, GetWindowLongW,
            GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible, WS_EX_TOOLWINDOW,
            WS_OVERLAPPEDWINDOW,
        },
    },
    core::{BOOL, PWSTR},
};

use crate::{MonitorInfo, MonitorManager, h};

// Add this to track recent events
thread_local! {
    static EVENT_THROTTLE: RefCell<HashMap<(u32, isize), Instant>> =
        RefCell::new(HashMap::new());
}

pub static WINEVENT_CHANNEL: OnceLock<(
    Sender<(WindowsEvent, WindowsApp)>,
    Receiver<(WindowsEvent, WindowsApp)>,
)> = OnceLock::new();

fn channel() -> &'static (
    Sender<(WindowsEvent, WindowsApp)>,
    Receiver<(WindowsEvent, WindowsApp)>,
) {
    WINEVENT_CHANNEL.get_or_init(|| flume::unbounded())
}

pub fn channel_receiver() -> Receiver<(WindowsEvent, WindowsApp)> {
    channel().1.clone()
}

pub fn channel_send(event: WindowsEvent, win_app: WindowsApp) {
    if let Err(err) = channel().0.send((event, win_app)) {
        eprintln!("failed to send event {err} {event:?}")
    }
}

#[derive(Debug, Clone, NtekSer, NtekDes)]
pub struct RectPadding {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, NtekSer, NtekDes)]
pub struct WinRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[allow(unused)]
#[derive(Debug, Clone, NtekSer, NtekDes)]
pub struct WindowsAppData {
    pub hwnd: isize,
    pub name: String,
    pub title: String,
    pub class: String,
    pub padding: RectPadding,
    pub rect: WinRect,
    pub is_maximised: bool,
}

pub struct WindowsApp {
    pub hwnd: isize,
}

impl From<HWND> for WindowsApp {
    fn from(hwnd: HWND) -> Self {
        Self {
            hwnd: hwnd.0 as isize,
        }
    }
}

impl WindowsApp {
    pub fn hwnd_ptr(&self) -> HWND {
        HWND(self.hwnd as *mut c_void)
    }
    pub fn get_app_data(&self) -> Result<WindowsAppData> {
        self.get_app_data_impl()
            .ok_or(anyhow::anyhow!("Cant got the app data"))
    }
    fn get_app_data_impl(&self) -> Option<WindowsAppData> {
        let hwnd_ptr = self.hwnd_ptr();
        let proc_path = WinAPI::get_proc_path(hwnd_ptr)?;
        let name = proc_path
            .split('\\')
            .next_back()?
            .strip_suffix(".exe")?
            .to_string();
        let is_maximised = WinAPI::is_maximized(hwnd_ptr).unwrap_or(false);
        let padding = WinAPI::get_rect_padding(hwnd_ptr);
        let title = WinAPI::get_title(hwnd_ptr)?;
        let class = WinAPI::get_class(hwnd_ptr)?;
        let rect = WinAPI::get_rect(hwnd_ptr);
        Some(WindowsAppData {
            hwnd: self.hwnd,
            name,
            padding,
            is_maximised,
            title,
            class,
            rect,
        })
    }
}

struct HandleGuard(HANDLE);
impl Drop for HandleGuard {
    fn drop(&mut self) {
        _ = unsafe { CloseHandle(self.0) };
    }
}

pub struct WinAPI;
impl WinAPI {
    pub fn get_windows_app_list() -> Vec<WindowsAppData> {
        let mut windows: Vec<WindowsAppData> = Vec::new();
        unsafe {
            _ = EnumWindows(
                Some(Self::get_active_list),
                LPARAM(&mut windows as *mut _ as isize),
            );
        };
        windows
    }
    pub fn resolve_monitor_rect(hwnd: HWND, monitor_index: Option<usize>) -> RECT {
        if let Some(index) = monitor_index {
            let monitors = MonitorManager::fetch_all_monitors();
            if let Some(m) = monitors.get(index) {
                return RECT {
                    left: m.left,
                    top: m.top,
                    right: m.right,
                    bottom: m.bottom,
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
    pub fn walk_z_order() -> anyhow::Result<Vec<String>> {
        let mut hwnds = Vec::new();
        unsafe {
            let mut current = GetTopWindow(Some(HWND(0 as *mut c_void)))?;
            while !current.is_invalid() {
                if let Some(p) = Self::get_proc_path(current) {
                    let name = p.split('\\').next_back().unwrap().to_string();
                    hwnds.push(name);
                }
                current = GetWindow(current, GW_HWNDNEXT)?;
            }
        }
        Ok(hwnds)
    }

    pub fn get_bottom_zorder_of(hwnds: HashSet<isize>) -> anyhow::Result<HWND> {
        unsafe {
            let mut current = GetTopWindow(None)?;

            if current.is_invalid() {
                anyhow::bail!("No windows in Z-order");
            }

            loop {
                let next = GetWindow(current, GW_HWNDNEXT)?;
                if next.is_invalid() {
                    break;
                }
                current = next;
            }

            while !current.is_invalid() {
                if hwnds.contains(&(current.0 as isize)) {
                    return Ok(current);
                }

                current = GetWindow(current, GW_HWNDPREV)?;
            }
        }

        anyhow::bail!("Cant find bottom most")
    }
    pub fn get_top_zorder_of(hwnds: HashSet<isize>) -> Result<HWND> {
        unsafe {
            let mut current = GetTopWindow(None)?;
            while !current.is_invalid() {
                if hwnds.contains(&(current.0 as isize)) {
                    return Ok(current);
                }
                current = GetWindow(current, GW_HWNDNEXT)?;
            }
        }
        anyhow::bail!("Cant find top most")
    }
    pub fn order_z_order(border_hwnd: HWND, app_hwnd: HWND) {
        unsafe {
            _ = SetWindowPos(
                app_hwnd,
                Some(border_hwnd),
                0,
                0,
                0,
                0,
                SWP_NOSIZE | SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOSENDCHANGING,
            );
        }
    }
    pub fn is_top_most(hwnd: isize) -> bool {
        let ex_style = unsafe { GetWindowLongW(h!(hwnd), GWL_EXSTYLE) };
        let is_topmost = (ex_style & WS_EX_TOPMOST.0 as i32) != 0;
        is_topmost
    }

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
    pub fn center_scale(hwnd: HWND, m: &MonitorInfo) -> WinRect {
        let w = m.width / 2;
        let h = m.height / 2;
        let target = WinRect {
            x: m.left + w / 2,
            y: m.top + h / 2,
            width: w,
            height: h,
        };
        _ = Self::set_position(hwnd.0 as isize, &target, false);
        target
    }
    pub fn toggle_top_most(top_most: bool, hwnd: HWND) {
        let after = if top_most { HWND_TOPMOST } else { HWND_BOTTOM };
        _ = unsafe {
            SetWindowPos(
                hwnd,
                Some(after),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOSENDCHANGING | SWP_NOREDRAW,
            )
        };
    }
    pub fn set_position(hwnd: isize, rect: &WinRect, to_down: bool) {
        let hwnd = h!(hwnd);
        Self::disable_rounded_corner(hwnd);
        _ = unsafe { ShowWindow(hwnd, SW_RESTORE) };
        let insert_after = if to_down { Some(HWND_BOTTOM) } else { None };
        unsafe {
            _ = SetWindowPos(
                hwnd,
                insert_after,
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                SWP_NOACTIVATE | SWP_NOSENDCHANGING,
            );
        }
    }
    pub fn focus_app(hwnd: isize) -> anyhow::Result<()> {
        let event = [INPUT {
            r#type: INPUT_MOUSE,
            ..Default::default()
        }];

        unsafe {
            SendInput(&event, size_of::<INPUT>() as i32);
            let _ = SetWindowPos(
                h!(hwnd),
                None,
                0,
                0,
                0,
                0,
                SWP_NOZORDER
                    | SWP_NOMOVE
                    | SWP_NOSIZE
                    | SWP_NOREDRAW
                    | SWP_SHOWWINDOW
                    | SWP_NOSENDCHANGING
                    | SWP_ASYNCWINDOWPOS,
            );
            _ = SetForegroundWindow(h!(hwnd));
        }
        Ok(())
    }
    pub fn hide_window(hwnd: isize) {
        unsafe {
            let _ = ShowWindow(h!((hwnd)), SW_HIDE);
        };
    }
    pub fn show_window(hwnd: isize) {
        unsafe {
            let _ = ShowWindow(h!((hwnd)), SW_SHOWNOACTIVATE);
        }
    }
    pub fn windows_app_listener() {
        unsafe {
            SetWinEventHook(
                EVENT_MIN,
                EVENT_MAX,
                None,
                Some(Self::windows_event_hook),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
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
    }
    pub fn is_maximized(hwnd: HWND) -> anyhow::Result<bool> {
        unsafe {
            let mut placement = WINDOWPLACEMENT::default();
            placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;

            GetWindowPlacement(hwnd, &mut placement)?;
            let result = placement.showCmd == SW_SHOWMAXIMIZED.0 as u32;
            Ok(result)
        }
    }

    fn get_proc_path(hwnd: HWND) -> Option<String> {
        let mut proc_id = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut proc_id)) };
        if proc_id == 0 {
            return None;
        }
        let proc_handle = unsafe {
            OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, proc_id).ok()
        }?;
        let mut path_buff: Vec<u16> = vec![0; 1024];
        let mut size = path_buff.len() as u32;
        let result = unsafe {
            QueryFullProcessImageNameW(
                proc_handle,
                PROCESS_NAME_FORMAT(0),
                PWSTR(path_buff.as_mut_ptr()),
                &mut size,
            )
            .ok()
        };
        let _guard = HandleGuard(proc_handle);
        if result.is_some() && size > 0 {
            path_buff.truncate(size as usize);
            Some(
                OsString::from_wide(&path_buff)
                    .to_string_lossy()
                    .into_owned(),
            )
        } else {
            None
        }
    }
    fn get_class(hwnd: HWND) -> Option<String> {
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
    fn get_title(hwnd: HWND) -> Option<String> {
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
    pub fn get_rect_padding(hwnd: HWND) -> RectPadding {
        let dwm_rect = Self::get_dwm_rect(hwnd, 0);
        let rect = Self::get_rect(hwnd);
        let x = rect.width - dwm_rect.width;
        let y = rect.height - dwm_rect.height;
        RectPadding { x, y }
    }
    pub fn get_rect(hwnd: HWND) -> WinRect {
        let rect = unsafe {
            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);
            rect
        };
        WinRect {
            x: rect.left,
            y: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        }
    }
    fn get_dwm_rect(hwnd: HWND, thickness: i32) -> WinRect {
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

        WinRect {
            x: rect.left,
            y: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        }
    }
    pub fn set_cursor_position(x: i32, y: i32) -> anyhow::Result<()> {
        unsafe { Ok(SetCursorPos(x, y)?) }
    }

    extern "system" fn get_active_list(hwnd: HWND, lparam: LPARAM) -> BOOL {
        if unsafe { IsWindowVisible(hwnd) } == FALSE {
            return TRUE;
        }
        if unsafe { GetAncestor(hwnd, GA_ROOTOWNER) } != hwnd
            || unsafe { GetWindowTextLengthW(hwnd) } == 0
            || hwnd.is_invalid()
        {
            return TRUE;
        }

        let ex_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
        let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;

        if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
            return TRUE;
        }
        if style & WS_OVERLAPPEDWINDOW.0 == 0 {
            return TRUE;
        }

        let mut cloaked = 0u32;
        let _ = unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                &mut cloaked as *mut _ as _,
                size_of::<u32>() as u32,
            )
        };
        if cloaked != 0 {
            return TRUE;
        }
        let windows = unsafe { &mut *(lparam.0 as *mut Vec<WindowsAppData>) };
        if let Ok(data) = WindowsApp::from(hwnd).get_app_data() {
            windows.push(data);
        }
        TRUE
    }
    extern "system" fn windows_event_hook(
        _win_event_hook: HWINEVENTHOOK,
        event: u32,
        hwnd: HWND,
        id_object: i32,
        id_child: i32,
        _id_event_thread: u32,
        _dwms_event_time: u32,
    ) {
        if id_object != OBJID_WINDOW.0 || id_child != 0 {
            return;
        }

        let hwnd_val = hwnd.0 as isize;

        // // Throttle rapid events from same window
        // let should_process = EVENT_THROTTLE.with(|throttle| {
        //     let mut map = throttle.borrow_mut();
        //     let key = (event, hwnd_val);
        //     let now = Instant::now();

        //     if let Some(&last_time) = map.get(&key) {
        //         if now.duration_since(last_time) < Duration::from_millis(5) {
        //             return false;
        //         }
        //     }

        //     map.insert(key, now);

        //     // Cleanup old entries (keep map size manageable)
        //     if map.len() > 1000 {
        //         map.retain(|_, &mut v| now.duration_since(v) < Duration::from_secs(5));
        //     }

        //     true
        // });
        // if !should_process {
        //     return;
        // }

        if unsafe { GetAncestor(hwnd, GA_ROOTOWNER) } != hwnd
            || unsafe { GetWindowTextLengthW(hwnd) } == 0
            || hwnd.is_invalid()
        {
            return;
        }

        if matches!(event, EVENT_OBJECT_DESTROY) {
            channel_send(WindowsEvent::Destroy, WindowsApp::from(hwnd));
            //destroy
        }

        if !unsafe { IsWindowVisible(hwnd).as_bool() } {
            return;
        }
        let style = WINDOW_STYLE(unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32);
        if !style.contains(WS_OVERLAPPEDWINDOW) {
            return;
        }

        let ex_style = WINDOW_EX_STYLE(unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32);
        if ex_style.contains(WS_EX_TOOLWINDOW) {
            return;
        }
        // Check cloaked (Windows 10+ feature for UWP apps)
        // let mut cloaked = 0u32;
        // unsafe {
        //     let _ = DwmGetWindowAttribute(
        //         hwnd,
        //         DWMWA_CLOAKED,
        //         &mut cloaked as *mut _ as _,
        //         size_of::<u32>() as u32,
        //     );
        // }
        // if cloaked != 0 {
        //     return;
        // }

        if let Ok(ev) = WindowsEvent::from_str(WindowsEvent::parse_event(event)) {
            channel_send(ev, WindowsApp::from(hwnd));
        }
    }
}

macro_rules! windows_event_builder {
  ($event_name:ident , $( ($int_val:expr,  $str_val:expr, $enum_val:ident) ),* $(,)?) => {
        #[derive(Debug, Clone, Copy)]
        pub enum $event_name {
          $($enum_val),*
        }
        impl From<u32> for $event_name{
          fn from(s:u32)->Self{
            match s{
              $( $int_val =>Self::$enum_val, )*
              _=>Self::Unknown
            }
          }
        }
        impl std::str::FromStr for $event_name{
          type Err = ();
          fn from_str(s:&str)->Result<Self,Self::Err>{
            match s{
              $( $str_val =>Ok(Self::$enum_val), )*
              _=>Err(())
            }
          }
        }
        impl $event_name{
          pub fn to_str(&self)->String{
            return match self {
              $($event_name::$enum_val => $str_val.to_string(), )*
            }
          }
          pub fn parse_event<'a>(id:u32)->&'a str{
            return match id {
              $($int_val => $str_val, )*
              _=>"Unknown"
            }
          }
        }

    };
}

windows_event_builder! {
  WindowsEvent,
    (3, "EVENT_SYSTEM_FOREGROUND", Foreground),
    (8, "EVENT_SYSTEM_CAPTURESTART", CaptureStart),
    (9, "EVENT_SYSTEM_CAPTUREEND", CaptureEnd),
    (10, "EVENT_SYSTEM_MOVESIZESTART", MoveSizeStart),
    (11, "EVENT_SYSTEM_MOVESIZEEND", MoveSizeEnd),
    (22, "EVENT_SYSTEM_MINIMIZESTART", MinimizeStart),
    (23, "EVENT_SYSTEM_MINIMIZEEND", MinimizeEnd),
    (32772, "EVENT_OBJECT_REORDER", Reorder),
    (32773, "EVENT_OBJECT_FOCUS", Focus),
    (32768, "EVENT_OBJECT_CREATE", Create),
    (32769, "EVENT_OBJECT_DESTROY", Destroy),
    (32770, "EVENT_OBJECT_SHOW", Show),
    (32778, "EVENT_OBJECT_STATECHANGE", StateChange),
    (32779, "EVENT_OBJECT_LOCATIONCHANGE", LocationChange),
    (32780, "EVENT_OBJECT_NAMECHANGE", NameChange),
    (99990, "EVENT_UNKNOWN", Unknown),
    (99991, "EVENT_INIT", Init),
    (99992, "EVENT_DONE", Done),
}

// enum WindowsEvent{
//   EVENT_OBJECT_CREATE
//   EVENT_OBJECT_LOCATIONCHANGE
//   EVENT_SYSTEM_CAPTURESTART
//   EVENT_SYSTEM_CAPTUREEND
//   EVENT_SYSTEM_MOVESIZESTART
//   EVENT_SYSTEM_MOVESIZEEND
//   EVENT_OBJECT_REORDER
//   EVENT_SYSTEM_MINIMIZESTART
//   EVENT_SYSTEM_FOREGROUND
//   EVENT_SYSTEM_MINIMIZEEND
//   EVENT_OBJECT_DESTROY
//   EVENT_OBJECT_SHOW
//   EVENT_OBJECT_NAMECHANGE
// }
