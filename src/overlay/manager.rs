use crate::{
    hwnd,
    overlay::{
        app_border::{BorderInfo, BorderOverlay},
        app_info::AppInfo,
        app_window::AppWindow,
        color::{self},
        config::NtekConfig,
        monitor_info::{self, get_monitors},
        overlay_handler::OverlayHandler,
        statusbar::StatusbarWindow,
        win_api,
        win_event::WinEvent,
        workspaces::Workspace,
    },
};
use flume::{Receiver, Sender};
use parking_lot::Mutex;
use std::{
    str::FromStr,
    sync::{Arc, OnceLock},
    time::Duration,
};
use windows::{
    Win32::{
        Foundation::*,
        UI::{
            Accessibility::{HWINEVENTHOOK, SetWinEventHook},
            WindowsAndMessaging::*,
        },
    },
    core::BOOL,
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

pub type OptBorderOverlay = Arc<Mutex<Option<BorderOverlay>>>;
pub type Shared<T> = Arc<Mutex<T>>;

pub struct OverlayManager {
    //apps
    // statusbar: Shared<Vec<isize>>,
    // borders: Shared<Vec<isize>>,
    app_handler: Shared<OverlayHandler>,
}

impl OverlayManager {
    pub fn new(config: Arc<NtekConfig>) -> Self {
        let statusbar_hwnds = Arc::new(Mutex::new(vec![]));
        // let border_hwnds = Arc::new(Mutex::new(vec![]));
        let border_overlay = Arc::new(Mutex::new(None::<BorderOverlay>));

        Self::init_winhook();
        Self::spawn_border_overlay_service(border_overlay.clone());
        Self::spawn_statusbar_service(statusbar_hwnds.clone());

        let mut handler = OverlayHandler::new();
        handler.monitors = monitor_info::get_monitors();
        handler.blacklist = config.blacklist.clone();
        handler.size_factor = config.size_factor.clone();
        handler.statusbar = statusbar_hwnds.clone();
        handler.user_widgets.lock().workspaces = config
            .workspaces
            .iter()
            .enumerate()
            .map(|(i, ws)| Workspace {
                text: ws.to_string(),
                active: i == 0,
                hwnds: Vec::new(),
            })
            .collect();
        handler.spawn_widget();
        let app_handler = Arc::new(Mutex::new(handler));

        Self::spawn_winevent_listener_service(border_overlay.clone(), app_handler.clone());

        Self {
            // statusbar: statusbar_hwnds,
            // borders: border_hwnds,
            app_handler,
        }
    }

    pub fn with_handler<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut OverlayHandler) -> R,
    {
        let mut handler = self.app_handler.lock();
        f(&mut handler)
    }

    //==============================================================================//
    // tag         : INTERNAL FUNCTION
    // description : -
    //==============================================================================//

    fn update_border(app: &AppInfo, border_overlay: OptBorderOverlay) {
        if let Some(ref overlay) = *border_overlay.lock() {
            const PADDING: i32 = 2;
            let is_maximized = win_api::is_maximized(app.hwnd);
            let (px, py) = win_api::get_rect_padding(app.hwnd);
            let y = if is_maximized {
                app.position.y + (py / 2)
            } else {
                app.position.y
            };
            overlay.set_focus(BorderInfo {
                x: app.position.x + (px / 2) + PADDING / 2,
                y: y + PADDING / 2,
                width: app.size.width - (px) - PADDING,
                height: app.size.height - (py) - PADDING,
                color: color::Theme::DANGER,
                thickness: 2.0,
                radius: 5.0,
            });
        }
    }

    fn spawn_winevent_listener_service(
        border_overlay: OptBorderOverlay,
        handler: Shared<OverlayHandler>,
    ) {
        std::thread::spawn(move || {
            while let Ok((ev, app_window)) = channel_receiver().recv() {
                if let Some(app) = app_window.get_app_info() {
                    eprintln!("{:?} {}{}", ev, app.exe, app.title);
                }
                match ev {
                    WinEvent::ObjectNamechange => {
                        if let Some(app) = app_window.get_app_info() {
                            let mut handler = handler.lock();
                            handler.update_app_title(&app);
                        }
                    }
                    WinEvent::ObjectDestroy => {
                        //delete app from app_list
                        if let Some(app) = app_window.get_app_info() {
                            handler.lock().delete_app(&app);
                            //Self::update_border(&app, border_overlay.clone());
                        }
                    }
                    WinEvent::ObjectCreate => {
                        //insert app into app_list
                        // this execute once every start app
                        if let Some(app) = app_window.get_app_info() {
                            handler.lock().update_apps(app, ev);
                        }
                    }
                    WinEvent::ObjectShow => {
                        //this is new spawn app
                        // add them to app_list
                        if let Some(app) = app_window.get_app_info() {
                            handler.lock().update_apps(app, ev);
                        }
                    }
                    WinEvent::Done => {
                        //this is called once per out start up
                        // when the active app listing are done
                        if let Ok(app) = Self::init_active_appinfo(&handler) {
                            handler.lock().update_apps(app, ev);
                        }
                    }
                    WinEvent::SystemCaptureend
                    | WinEvent::SystemMovesizeend
                    | WinEvent::SystemMinimizeend => {
                        if let Some(app) = app_window.get_app_info() {
                            Self::update_border(&app, border_overlay.clone());
                            handler.lock().update_apps(app, ev);
                        }
                    }
                    WinEvent::ObjectLocationchange => {
                        if let Some(app) = app_window.get_app_info() {
                            Self::update_border(&app, border_overlay.clone());
                            if let Ok(maximized) = win_api::is_window_maximized(hwnd!(app.hwnd)) {
                                if maximized {
                                    handler.lock().fake_maximize();
                                }
                            }
                            {
                                handler.lock().update_apps(app, ev);
                            }
                        }
                    }
                    WinEvent::SystemForeground => {
                        if let Some(app) = app_window.get_app_info() {
                            Self::update_border(&app, border_overlay.clone());
                            let mut handler = handler.lock();
                            handler.update_active_app(app.hwnd);
                            handler.update_apps(app, ev);
                            handler.reset_size_selector();
                        }
                    }
                    _ => {}
                }
            }
        });
    }
    pub fn init_active_appinfo(handler: &Shared<OverlayHandler>) -> anyhow::Result<AppInfo> {
        let mut current = unsafe { GetForegroundWindow() };
        if current.0.is_null() {
            anyhow::bail!("No current window");
        }
        while !current.0.is_null() {
            let contain = {
                let guard = handler.lock();
                let contain = guard.apps.contains_key(&(current.0 as isize));
                contain
            };
            if contain {
                {
                    handler.lock().current_active_app = Some(current.0 as isize);
                }
                let app_info = AppWindow::from(current)
                    .get_app_info()
                    .ok_or(anyhow::anyhow!("AppInfo Not Found"))?;
                return Ok(app_info);
            }
            current = unsafe { GetWindow(current, GW_HWNDNEXT) }?;
        }
        anyhow::bail!("App info not found");
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
    fn spawn_border_overlay_service(border_overlay: Arc<Mutex<Option<BorderOverlay>>>) {
        std::thread::spawn(move || {
            unsafe {
                let mut msg = MSG::default();
                _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            }

            match BorderOverlay::new() {
                Ok(overlay) => {
                    *border_overlay.lock() = Some(overlay);
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
            if let Err(err) = unsafe { EnumWindows(Some(Self::init_applist), LPARAM(0)) } {
                eprintln!("Error Listing {err}")
            }
            channel_send(WinEvent::Done, AppWindow::default());
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
            if GetAncestor(hwnd, GA_ROOTOWNER) != hwnd
                || GetWindowTextLengthW(hwnd) == 0
                || hwnd.is_invalid()
            {
                return;
            }
            let app_window = AppWindow::from(hwnd);

            if matches!(event, EVENT_OBJECT_DESTROY) {
                channel_send(WinEvent::ObjectDestroy, app_window);
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
            if let Ok(ev) = crate::overlay::win_event::WinEvent::from_str(
                crate::overlay::win_event::WinEvent::parse_event(event),
            ) {
                channel_send(ev, app_window);
            }
        }
    }
}
