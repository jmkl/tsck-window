use crate::appinfo::{AppInfo, AppWindow};
use flume::{Receiver, Sender};
use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::{collections::HashMap, sync::OnceLock, time::Duration};
use windows::{
    Win32::{
        Foundation::{FALSE, HWND, LPARAM, TRUE},
        UI::{
            Accessibility::{HWINEVENTHOOK, SetWinEventHook},
            WindowsAndMessaging::{
                DispatchMessageW, EVENT_MAX, EVENT_MIN, EVENT_OBJECT_DESTROY, EnumWindows,
                GA_ROOTOWNER, GWL_EXSTYLE, GWL_STYLE, GetAncestor, GetMessageW, GetWindowLongW,
                GetWindowTextLengthW, IsWindowVisible, MSG, OBJID_WINDOW, TranslateMessage,
                WINDOW_EX_STYLE, WINDOW_STYLE, WINEVENT_OUTOFCONTEXT, WS_EX_TOOLWINDOW,
                WS_OVERLAPPEDWINDOW,
            },
        },
    },
    core::BOOL,
};

lazy_static! {
    static ref APP_INFO_LIST: Mutex<HashMap<isize, AppInfo>> = Mutex::new(HashMap::new());
}
use crate::event;

pub static WINEVENT_CHANNEL: OnceLock<(
    Sender<(String, AppWindow)>,
    Receiver<(String, AppWindow)>,
)> = OnceLock::new();

extern "system" fn wc_init_applist(hwnd: HWND, lparam: LPARAM) -> BOOL {
    match unsafe { IsWindowVisible(hwnd) } == FALSE {
        true => return TRUE,
        false => (),
    }

    let app_window = AppWindow::from(hwnd);
    channel_send("EVENT_OBJECT_CREATE", app_window);

    TRUE
}
pub fn wm_event_channel() -> &'static (Sender<(String, AppWindow)>, Receiver<(String, AppWindow)>) {
    WINEVENT_CHANNEL.get_or_init(|| flume::unbounded())
}
pub fn wm_event_tx() -> Sender<(String, AppWindow)> {
    wm_event_channel().0.clone()
}

pub fn wm_event_channel_receiver() -> Receiver<(String, AppWindow)> {
    wm_event_channel().1.clone()
}
pub fn init() {
    let (_, _) = wm_event_channel();
    if let Err(err) = unsafe { EnumWindows(Some(wc_init_applist), LPARAM(0)) } {
        eprintln!("Error Listing {err}")
    }
}

pub fn hook_win_event() {
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
}
fn channel_send(event: &str, app_window: AppWindow) {
    if let Err(err) = wm_event_tx().send((event.to_string(), app_window)) {
        eprintln!("failed to send event {err} {event}")
    }
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
            channel_send("EVENT_OBJECT_DESTROY", app_window);
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

        channel_send(event::parse_event(event), app_window);
    }
}
