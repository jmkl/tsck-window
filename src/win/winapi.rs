use flume::{Receiver, Sender};
use ntek_derive::{NtekDes, NtekSer};
use std::{
    collections::HashSet,
    ffi::{OsString, c_void},
    os::windows::ffi::OsStringExt,
    str::FromStr,
    sync::OnceLock,
    time::Duration,
};

use windows::Win32::UI::{
    Accessibility::{HWINEVENTHOOK, SetWinEventHook},
    Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT,
        SendInput,
    },
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        System::Threading::*,
        UI::WindowsAndMessaging::*,
    },
    core::{BOOL, PWSTR},
};

use crate::{h, log_error, win::event::WindowsEvent};

pub static WINEVENT_CHANNEL: OnceLock<(
    Sender<(WindowsEvent, WinApp)>,
    Receiver<(WindowsEvent, WinApp)>,
)> = OnceLock::new();

pub const STATUSBAR_HEIGHT: f32 = 30.0;

pub fn get_statusbar_height(monitor: usize) -> f32 {
    if monitor == 0 { STATUSBAR_HEIGHT } else { 0.0 }
}

fn channel() -> &'static (
    Sender<(WindowsEvent, WinApp)>,
    Receiver<(WindowsEvent, WinApp)>,
) {
    WINEVENT_CHANNEL.get_or_init(|| flume::unbounded())
}

pub fn channel_receiver() -> Receiver<(WindowsEvent, WinApp)> {
    channel().1.clone()
}

pub fn channel_send(event: WindowsEvent, win_app: WinApp) {
    if let Err(err) = channel().0.send((event, win_app)) {
        eprintln!("failed to send event {err} {event:?}")
    }
}

#[derive(Debug, Clone, NtekSer, NtekDes)]
pub struct AppData {
    pub hwnd: isize,
    pub name: String,
    pub title: String,
    pub class: String,
    pub rect: AppRect,
}
const MIN: i32 = 300;
const MAX: i32 = 3000;
#[derive(Debug, Clone, NtekSer, NtekDes)]
pub struct AppRect {
    pub l: i32,
    pub t: i32,
    pub r: i32,
    pub b: i32,
    pub width: i32,
    pub height: i32,
}
impl AppRect {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            l: x,
            t: y,
            r: x + width,
            b: y + height,
            width,
            height,
        }
    }
    pub fn move_x(r: &AppRect, val: i32) -> Self {
        Self {
            l: r.l + val,
            r: r.r + val,
            t: r.t,
            b: r.b,
            width: r.width,
            height: r.height,
        }
    }
    pub fn move_y(r: &AppRect, val: i32) -> Self {
        Self {
            l: r.l,
            r: r.r,
            t: r.t + val,
            b: r.b + val,
            width: r.width,
            height: r.height,
        }
    }
    pub fn set_width(r: &AppRect, width: i32) -> Self {
        Self {
            l: r.l,
            t: r.t,
            r: r.r + width,
            b: r.b + r.height,
            width: width,
            height: r.height,
        }
    }
    pub fn add_to_width(r: &AppRect, inc: i32) -> Self {
        let new_width = (r.width + inc).clamp(MIN, MAX);
        let delta = new_width - r.width;

        Self {
            l: r.l,
            t: r.t,
            r: r.r + delta,
            b: r.b,
            width: new_width,
            height: r.height,
        }
    }
    pub fn add_to_height(r: &AppRect, inc: i32) -> Self {
        let new_height = (r.height + inc).clamp(MIN, MAX);
        let delta = new_height - r.height;

        Self {
            l: r.l,
            t: r.t,
            r: r.r,
            b: r.b + delta,
            width: r.width,
            height: new_height,
        }
    }
    pub fn xy(rect: &AppRect, x: i32, y: i32) -> Self {
        Self {
            l: x,
            t: y,
            r: x + rect.width,
            b: y + rect.height,
            width: rect.width,
            height: rect.height,
        }
    }
}

#[derive(Debug)]
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

#[derive(Debug, Clone, Copy)]
pub struct WinApp {
    pub hwnd: isize,
}
impl From<HWND> for WinApp {
    fn from(value: HWND) -> Self {
        Self {
            hwnd: value.0 as isize,
        }
    }
}

impl WinApp {
    pub fn get_app_info(&self) -> Option<AppData> {
        let hwnd = HWND(self.hwnd as *mut c_void);
        let exe_path = WindowsAPI::get_process_path(hwnd)?;
        let exe = exe_path
            .split('\\')
            .next_back()?
            .strip_suffix(".exe")?
            .to_string();
        let rect = WindowsAPI::get_rect(hwnd);
        let title = WindowsAPI::get_app_title(hwnd)?;
        let class = WindowsAPI::get_app_class(hwnd)?;
        Some(AppData {
            hwnd: self.hwnd,
            name: exe,
            title,
            class,
            rect,
        })
    }
}

pub struct WindowsAPI;
impl WindowsAPI {
    pub fn get_all_monitors() -> Vec<MonitorInfo> {
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
    pub fn center_scale(hwnd: HWND, monitor: Option<usize>) -> Option<AppRect> {
        if let Some(idx) = monitor {
            let monitors = Self::get_all_monitors();
            let m = &monitors[idx];
            let w = m.width / 2;
            let h = m.height / 2;
            let target = AppRect {
                l: m.left + w / 2,
                t: m.top + h / 2,
                r: m.left + w,
                b: m.right + h,
                width: w,
                height: h,
            };
            _ = Self::transform_to(hwnd.0 as isize, &target);
            return Some(target);
        }
        None
    }
    pub fn get_app_monitor(hwnd: HWND, monitors: &[MonitorInfo]) -> Option<usize> {
        let current = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
        monitors
            .iter()
            .position(|m| HMONITOR(m.handle as *mut c_void) == current)
    }

    pub fn get_monitor_in_cursor(monitors: &[MonitorInfo]) -> usize {
        let mut point = POINT { x: 0, y: 0 };
        _ = unsafe { GetCursorPos(&mut point) };
        let hmonitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
        monitors
            .iter()
            .position(|m| HMONITOR(m.handle as *mut c_void) == hmonitor)
            .unwrap_or(0)
    }
    pub fn resolve_monitor_rect(hwnd: HWND, monitor_index: Option<usize>) -> RECT {
        if let Some(index) = monitor_index {
            let monitors = Self::get_all_monitors();
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
    pub fn window_above(hwnd: HWND) -> Option<isize> {
        unsafe {
            let above = GetWindow(hwnd, GW_HWNDPREV).ok()?;
            Some(above.0 as isize)
        }
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

    pub fn get_rect_padding(hwnd: isize) -> (i32, i32) {
        let dwm_rect = Self::get_dwm_rect(crate::h!(hwnd), 0);
        let rect = Self::get_rect(crate::h!(hwnd));
        let x = rect.width - dwm_rect.width;
        let y = rect.height - dwm_rect.height;
        (x, y)
    }
    pub(crate) fn get_rect(hwnd: HWND) -> AppRect {
        let rect = unsafe {
            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);
            rect
        };
        AppRect {
            l: rect.left,
            t: rect.top,
            r: rect.right,
            b: rect.bottom,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        }
    }
    pub fn list_z_orders() -> anyhow::Result<()> {
        let mut result = Vec::new();
        unsafe {
            let mut hwnd = GetTopWindow(Some(HWND(0 as *mut c_void)))?;
            while !hwnd.is_invalid() {
                if IsWindowVisible(hwnd).as_bool() {
                    result.push(hwnd.0 as isize);
                }
                hwnd = match GetWindow(hwnd, GW_HWNDNEXT) {
                    Ok(next) => next,
                    Err(_) => break,
                };
            }
        }
        result.iter().for_each(|hwnd| {
            println!("{:?}", Self::get_process_path(HWND(*hwnd as *mut c_void)));
        });
        Ok(())
    }
    pub fn get_window_z_order(hwnd: isize) -> anyhow::Result<i32> {
        unsafe {
            let mut z_order = 0;
            let mut current_hwnd = GetTopWindow(Some(h!(0)))?;

            while !current_hwnd.is_invalid() {
                if current_hwnd == h!(hwnd) {
                    return Ok(z_order);
                }
                z_order += 1;
                current_hwnd = GetWindow(current_hwnd, GW_HWNDNEXT)?;
            }

            Ok(i32::MAX)
        }
    }

    pub fn top_visible_window(
        blacklist: &Vec<String>,
        floating: Vec<isize>,
    ) -> anyhow::Result<HWND> {
        let hwnd = unsafe { GetTopWindow(None)? };
        let mut next_hwnd = hwnd;

        while !next_hwnd.is_invalid() {
            if unsafe { IsWindowVisible(next_hwnd) } == TRUE
                && !Self::is_blacklist(blacklist, next_hwnd)
                && !floating.contains(&(next_hwnd.0 as isize))
            {
                return Ok(next_hwnd);
            }

            next_hwnd = unsafe { GetWindow(next_hwnd, GW_HWNDNEXT) }?;
        }

        anyhow::bail!("could not find next window")
    }
    fn is_blacklist(blacklist: &Vec<String>, hwnd: HWND) -> bool {
        let class_b = &["SystemTray_Main", "StatusbarWindowYoo"];
        if let Some(app) = WinApp::from(hwnd).get_app_info() {
            if !blacklist.contains(&app.name)
                && app.rect.width != 0
                && !class_b.contains(&app.class.as_str())
            {
                return false;
            }
        }
        true
    }
    pub fn top_zorder_from_app(apps: &[isize]) -> isize {
        if apps.is_empty() {
            return 0;
        }

        unsafe {
            let set: HashSet<isize> = apps.iter().copied().collect();

            // Start from the topmost window
            let mut hwnd = match GetTopWindow(None) {
                Ok(h) => h,
                Err(_) => return 0,
            };

            loop {
                if hwnd.is_invalid() {
                    break;
                }

                let handle = hwnd.0 as isize;

                // Check if this window is in our list
                if set.contains(&handle) {
                    return handle;
                }

                // Move to next window in Z-order
                hwnd = match GetWindow(hwnd, GW_HWNDNEXT) {
                    Ok(next) => next,
                    Err(_) => break,
                };
            }

            0
        }
    }
    pub fn below_floating_app(apps: &[isize]) -> isize {
        if apps.is_empty() {
            return 0;
        }

        unsafe {
            let mut remaining: HashSet<isize> = apps.iter().copied().collect();

            // Start from the topmost window
            let mut hwnd = match GetTopWindow(None) {
                Ok(h) => h,
                Err(_) => return 0,
            };

            loop {
                if hwnd.is_invalid() {
                    break;
                }

                let handle = hwnd.0 as isize;

                // Check if this window is in our list
                if remaining.contains(&handle) {
                    remaining.remove(&handle);

                    // Found all apps? Return the NEXT window
                    if remaining.is_empty() {
                        // Get the next window in Z-order
                        return match GetWindow(hwnd, GW_HWNDNEXT) {
                            Ok(next) if !next.is_invalid() => next.0 as isize,
                            _ => 0,
                        };
                    }
                }

                // Move to next window in Z-order
                hwnd = match GetWindow(hwnd, GW_HWNDNEXT) {
                    Ok(next) => next,
                    Err(_) => break,
                };
            }

            0 // Didn't find all apps
        }
    }
    pub fn top_window_in_workspace(apps: &[&AppData]) -> Option<isize> {
        unsafe {
            let mut hwnd = GetTopWindow(Some(HWND(0 as *mut c_void))).ok()?;

            while hwnd.0 as isize != 0 {
                // Only consider visible windows
                if IsWindowVisible(hwnd).as_bool() {
                    if apps.iter().any(|a| a.hwnd == hwnd.0 as isize) {
                        return Some(hwnd.0 as isize);
                    }
                }

                hwnd = GetWindow(hwnd, GW_HWNDNEXT).ok()?;
            }

            None
        }
    }
    pub fn top_window(blacklist: &Vec<String>) -> Option<HWND> {
        let class_b = &["SystemTray_Main", "Shell_TrayWnd", "Tsck-Statusbar"];
        unsafe {
            let mut hwnd = GetTopWindow(Some(HWND(0 as *mut c_void))).ok()?;
            while !hwnd.is_invalid() {
                if IsWindowVisible(hwnd).as_bool() {
                    if let Some(path) = Self::get_process_path(hwnd) {
                        let name = path.to_lowercase();
                        let stem = name.strip_suffix(".exe").unwrap_or(&name);
                        let is_blacklisted = blacklist.iter().any(|b| {
                            let b = b.to_lowercase();
                            let b_stem = b.strip_suffix(".exe").unwrap_or(&b);
                            stem.ends_with(b_stem)
                        });
                        let rect = Self::get_rect(hwnd);
                        let class = Self::get_app_class(hwnd)?;

                        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                        let is_topmost = (ex_style & WS_EX_TOPMOST.0 as i32) != 0;

                        if !is_blacklisted
                            && rect.width > 50
                            && !is_topmost
                            && !class_b.contains(&class.as_str())
                        {
                            if let Ok(hw) = GetWindow(hwnd, GW_HWNDPREV) {
                                return Some(hw);
                            }
                            return Some(hwnd);
                        }
                    }
                }
                hwnd = match GetWindow(hwnd, GW_HWNDNEXT) {
                    Ok(next) => next,
                    Err(_) => break,
                };
            }
        }
        None
    }

    pub fn spawn_app_listener_service() {
        std::thread::spawn(|| {
            // Enumerate all active windows into the app list
            if let Err(err) = unsafe { EnumWindows(Some(Self::get_active_app_list), LPARAM(0)) } {
                eprintln!("Error Listing {err}")
            }
            channel_send(WindowsEvent::Done, WinApp::from(h!(0)));

            unsafe {
                SetWinEventHook(
                    EVENT_MIN,
                    EVENT_MAX,
                    None,
                    Some(Self::win_event_hook),
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

    extern "system" fn get_active_app_list(hwnd: HWND, _lparam: LPARAM) -> BOOL {
        if unsafe { IsWindowVisible(hwnd) } == FALSE {
            return TRUE;
        }
        // Skip owned windows
        unsafe {
            if GetAncestor(hwnd, GA_ROOTOWNER) != hwnd
                || GetWindowTextLengthW(hwnd) == 0
                || hwnd.is_invalid()
            {
                return TRUE;
            }

            // Skip tool windows
            let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
                return TRUE;
            }

            // Must have normal window style
            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            if style & WS_OVERLAPPEDWINDOW.0 == 0 {
                return TRUE;
            }

            // Skip cloaked (virtual desktop/UWP hidden)
            let mut cloaked: u32 = 0;
            let _ = DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                &mut cloaked as *mut _ as _,
                std::mem::size_of::<u32>() as u32,
            );

            if cloaked != 0 {
                return TRUE;
            }

            // Must have title
            let len = GetWindowTextLengthW(hwnd);
            if len == 0 {
                return TRUE;
            }
        }
        let app_window = WinApp::from(hwnd);
        if let Some(app) = app_window.get_app_info() {
            log_error!(app.name, app.title);
        };
        channel_send(WindowsEvent::Init, app_window);
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
            let app_window = WinApp::from(hwnd);

            if GetAncestor(hwnd, GA_ROOTOWNER) != hwnd
                || GetWindowTextLengthW(hwnd) == 0
                || hwnd.is_invalid()
            {
                return;
            }

            if matches!(event, EVENT_OBJECT_DESTROY) {
                channel_send(WindowsEvent::ObjectDestroy, app_window);
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

            if let Ok(ev) = WindowsEvent::from_str(WindowsEvent::parse_event(event)) {
                channel_send(ev, app_window);
            }
        }
    }

    pub(crate) fn get_dwm_rect(hwnd: HWND, thickness: i32) -> AppRect {
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

        AppRect {
            l: rect.left,
            t: rect.top,
            r: rect.right,
            b: rect.bottom,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        }
    }
    pub fn is_maximized(hwnd: isize) -> bool {
        unsafe { IsZoomed(HWND(hwnd as *mut c_void)).as_bool() }
    }

    pub fn transform(hwnd: isize, _from: &AppRect, to_rect: &AppRect) -> anyhow::Result<()> {
        // log_debug!("Transforming", hwnd);
        Self::transform_to(hwnd, to_rect)?;
        // crate::win::animation::animate_window(hwnd, from, to_rect);
        Ok(())
    }
    pub fn to_bottom_order(hwnd: isize) {
        let hwnd = h!(hwnd);
        unsafe {
            _ = SetWindowPos(hwnd, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOSIZE | SWP_NOMOVE);
        };
    }

    pub fn transform_to(hwnd: isize, rect: &AppRect) -> anyhow::Result<()> {
        let hwnd = h!(hwnd);
        Self::disable_rounded_corner(hwnd);
        _ = unsafe { ShowWindow(hwnd, SW_RESTORE) };
        _ = unsafe {
            SetWindowPos(
                hwnd,
                None,
                rect.l,
                rect.t,
                rect.width,
                rect.height,
                SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_ASYNCWINDOWPOS | SWP_NOZORDER,
            )
        };

        // unsafe { MoveWindow(hwnd, rect.l, rect.t, rect.width, rect.height, true)? };

        Ok(())
    }

    pub fn set_positionx(hwnd: isize, position: (i32, i32)) {
        let hwnd = h!(hwnd);
        if unsafe { IsWindow(Some(hwnd)) } == FALSE {
            return;
        }

        _ = unsafe {
            SetWindowPos(
                hwnd,
                None,
                position.0,
                position.1,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
            )
        };
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

    pub fn is_window_maximized(hwnd: HWND) -> anyhow::Result<bool> {
        unsafe {
            let mut placement = WINDOWPLACEMENT::default();
            placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;

            GetWindowPlacement(hwnd, &mut placement)?;
            let result = placement.showCmd == SW_SHOWMAXIMIZED.0 as u32;
            Ok(result)
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
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW | SWP_ASYNCWINDOWPOS,
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
        };
    }

    pub fn is_top_most(hwnd: HWND) -> bool {
        unsafe {
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            (ex_style & WS_EX_TOPMOST.0) != 0
        }
    }
    pub fn set_top_most(hwnd: HWND) {
        _ = unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        };
    }
    pub fn set_top_most_after(hwnd: HWND, after: HWND) {
        _ = unsafe {
            SetWindowPos(
                hwnd,
                Some(after),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        };
    }
    pub fn toggle_top_most(top_most: bool, hwnd: HWND) {
        let after = if top_most {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        };
        _ = unsafe {
            SetWindowPos(
                hwnd,
                Some(after),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        };
    }

    pub fn left_click() -> u32 {
        let inputs = [
            INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: 0,
                        dwFlags: MOUSEEVENTF_LEFTDOWN,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: 0,
                        dwFlags: MOUSEEVENTF_LEFTUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];

        unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) }
    }
    pub fn set_cursor_pos(x: i32, y: i32) -> anyhow::Result<()> {
        unsafe { Ok(SetCursorPos(x, y)?) }
    }
    pub fn is_window(hwnd: HWND) -> bool {
        unsafe { IsWindow(Some(hwnd)).as_bool() }
    }
}
