use crate::hook::{
    app_window::AppWindow,
    overlay::{
        app_border::{BorderInfo, BorderOverlay},
        monitor_info::get_monitors,
        statusbar::{StatusBar, StatusbarWindow},
    },
    win_api,
    win_event::WinEvent,
};
use flume::{Receiver, Sender};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, OnceLock},
    time::Duration,
};
use windows::Win32::{
    Foundation::*,
    UI::{
        Accessibility::{HWINEVENTHOOK, SetWinEventHook},
        WindowsAndMessaging::*,
    },
};
pub const STATUSBAR_HEIGHT: f32 = 30.0;
pub const WM_UPDATE_STATUSBAR: u32 = WM_USER + 1;
pub const WM_UPDATE_BORDER: u32 = WM_USER + 2;

static WINEVENT_CHANNEL: OnceLock<(
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

pub struct OverlayManager {
    statusbar: Arc<Mutex<Vec<isize>>>,
    borders: Arc<Mutex<Vec<isize>>>,
}

impl OverlayManager {
    pub fn new() -> Self {
        let statusbar_hwnds = Arc::new(Mutex::new(vec![]));
        let border_hwnds = Arc::new(Mutex::new(vec![]));
        let border_overlay = Arc::new(Mutex::new(None::<BorderOverlay>));

        Self::init_winhook();
        Self::spawn_border_overlay_service(border_overlay.clone());
        Self::spawn_statusbar_service(statusbar_hwnds.clone());

        std::thread::spawn(move || {
            while let Ok((ev, app_window)) = channel_receiver().recv() {
                match ev {
                    WinEvent::ObjectLocationchange | WinEvent::SystemForeground => {
                        if let Some(app) = app_window.get_app_info() {
                            if let Some(ref overlay) = *border_overlay.lock() {
                                let (px, py) = win_api::get_rect_padding(app.hwnd);
                                overlay.set_focus(BorderInfo {
                                    x: app.position.x + (px / 2),
                                    y: app.position.y + (py / 2),
                                    width: app.size.width - (px),
                                    height: app.size.height - (py),
                                    color: 0xFFdd00,
                                    thickness: 1.0,
                                    radius: 5.0,
                                });
                            }
                        }
                        eprintln!("{:?}", ev);
                    }
                    _ => {}
                }
            }
        });
        // store tx
        Self {
            statusbar: statusbar_hwnds,
            borders: border_hwnds,
        }
    }
    fn spawn_statusbar_service(hwnds: Arc<Mutex<Vec<isize>>>) {
        std::thread::spawn(move || {
            unsafe {
                let mut msg = MSG::default();
                _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            }
            let monitors = get_monitors();
            for monitor in monitors.iter() {
                match StatusbarWindow::new(monitor) {
                    Ok(window) => {
                        hwnds.lock().push(window.hwnd().0 as isize);
                        std::mem::forget(window);
                    }
                    Err(e) => eprintln!("Statusbar error: {e}"),
                }
            }
            // message loop for statusbar windows
            unsafe {
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        });
    }
    fn spawn_border_overlay_service(border_overlay_init: Arc<Mutex<Option<BorderOverlay>>>) {
        std::thread::spawn(move || {
            unsafe {
                let mut msg = MSG::default();
                _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            }

            match BorderOverlay::new() {
                Ok(overlay) => {
                    *border_overlay_init.lock() = Some(overlay);
                }
                Err(e) => eprintln!("BorderOverlay error: {e}"),
            }

            unsafe {
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        });
    }
    fn init_winhook() {
        std::thread::spawn(|| {
            // Attach window hook
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
            if GetAncestor(hwnd, GA_ROOTOWNER) != hwnd
                || GetWindowTextLengthW(hwnd) == 0
                || hwnd.is_invalid()
            {
                return;
            }
            let app_window = AppWindow::from(hwnd);

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

    pub fn update_statusbar(&self, monitor_index: usize, bar: StatusBar) -> anyhow::Result<()> {
        // small delay to let the window thread initialize
        let raw = loop {
            if let Some(&r) = self.statusbar.lock().get(monitor_index) {
                break r;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        let hwnd = HWND(raw as *mut std::ffi::c_void);
        unsafe {
            PostMessageW(
                Some(hwnd),
                WM_UPDATE_STATUSBAR,
                WPARAM(Box::into_raw(Box::new(bar)) as usize),
                LPARAM(0),
            )?;
        }
        Ok(())
    }
}
