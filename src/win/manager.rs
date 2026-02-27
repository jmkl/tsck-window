use crate::log_error;
use crate::win::border::BorderOverlay;
use crate::win::config::{WinNtek, spawn_commandline, spawn_hotkee};
use crate::win::event::WindowsEvent as E;
use crate::win::statusbar::StatusbarWindow;
use crate::win::widget::Workspace;
use crate::win::winapi::{self, WindowsAPI};
use ntek;
use std::sync::Arc;
use std::time::Duration;

use crate::win::context::{AppContext, Shared};
use anyhow::Result;
use parking_lot::Mutex;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, TranslateMessage,
};

pub struct WinManager {
    _context: Shared<AppContext>,
}
impl WinManager {
    pub fn new() -> Self {
        let config = ntek::from_str::<WinNtek>(include_str!("../../win.ntek"))
            .expect("Failed to parse win.ntek");
        let statusbar_hwnds = Arc::new(Mutex::new(vec![]));
        WindowsAPI::spawn_app_listener_service();
        Self::spawn_statusbar_service(statusbar_hwnds.clone());

        let ntek = Arc::new(config);

        let mut ctx = AppContext::new(ntek.clone());
        ctx.statusbar = statusbar_hwnds.clone();
        ctx.user_widgets.lock().workspaces = ntek
            .workspaces
            .iter()
            .enumerate()
            .map(|(i, ws)| Workspace {
                text: ws.to_string(),
                active: i == 0,
                hwnds: vec![],
            })
            .collect();
        Self::spawn_border_service(ctx.border_overlay.clone());
        // Self::spawn_topmost_border_service(ctx.top_most_overlay.clone());

        ctx.spawn_widget();

        let context = Arc::new(Mutex::new(ctx));
        Self::spawn_event_listener_service(context.clone());
        spawn_hotkee(ntek, context.clone());
        spawn_commandline(context.clone());

        Self { _context: context }
    }
    pub fn event_loop(&self) {
        let ctx = self._context.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(2));
            ctx.lock().initialized = true;
        });
        loop {
            std::thread::park();
        }
    }

    fn spawn_statusbar_service(hwnds: Arc<Mutex<Vec<isize>>>) {
        std::thread::spawn(move || {
            unsafe {
                let mut msg = MSG::default();
                _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            }
            let monitors = WindowsAPI::get_all_monitors();
            for monitor in monitors.iter() {
                match StatusbarWindow::new(monitor) {
                    Ok(window) => {
                        hwnds.lock().push(window.hwnd().0 as isize);
                        std::mem::forget(window);
                    }
                    Err(e) => {
                        log_error!("Statusbar error ", e);
                    }
                }
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

    fn spawn_border_service(border_overlay: Arc<Mutex<Option<BorderOverlay>>>) {
        std::thread::spawn(move || {
            unsafe {
                let mut msg = MSG::default();
                _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            }

            match BorderOverlay::new("F0Cu5-80RD3R") {
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

    fn spawn_event_listener_service(ctx: Shared<AppContext>) {
        std::thread::spawn(move || -> Result<()> {
            while let Ok((ev, win)) = winapi::channel_receiver().recv() {
                if let Some(app) = win.get_app_info() {
                    if ctx.lock().is_blacklist(&app.name) {
                        continue;
                    }
                    if app.title.contains("Program Manager") {
                        continue;
                    }
                }
                match ev {
                    E::Init => {
                        ctx.lock().add_app(true, &win)?;
                    }
                    E::Done => {
                        ctx.lock().initialized();
                        // ctx.lock().apply_layout_in_workspace(false);
                    }
                    /*
                    This fire initialy when we start the app
                    it will list currently active app
                    */
                    E::ObjectCreate => {
                        ctx.lock().add_app(false, &win)?;
                    }
                    E::ObjectLocationchange => {
                        if let Some(app) = win.get_app_info() {
                            ctx.lock().on_location_change(&app)?;
                        }
                    }
                    E::SystemCapturestart => {}
                    E::SystemCaptureend => {}
                    E::SystemMovesizestart => {}

                    /*
                    this event fired when done moving/resizing
                    app.
                    - update monitor
                    - update size and position
                    -
                    */
                    E::SystemMovesizeend => {
                        if let Some(app) = win.get_app_info() {
                            ctx.lock().on_move_size_end(&app);
                        }
                    }
                    E::ObjectReorder => {}
                    E::SystemMinimizestart => {}
                    E::SystemForeground => {
                        if let Some(app) = win.get_app_info() {
                            {
                                let mut guard = ctx.lock();
                                guard.active_app = Some(app.hwnd);
                                guard.sync_widget_and_border("SystemForeground")?;
                            }
                            {}
                        }
                    }
                    E::SystemMinimizeend => {}
                    E::ObjectDestroy => {
                        if let Some(app) = win.get_app_info() {
                            ctx.lock().remove_app(app.hwnd)?;
                        }
                    }
                    /*
                    This event fire when we start new app

                    */
                    E::ObjectShow => {
                        ctx.lock().add_app(false, &win)?;
                    }

                    E::ObjectNamechange => {
                        if let Some(app) = win.get_app_info() {
                            ctx.lock().widget_update_title(app.hwnd)?;
                        }
                    }
                    _ => {}
                }
            }
            Ok(())
        });
    }
}
