use crate::{
    MonitorManager,
    border_manager::BorderPainter,
    config::{WinNtek, spawn_hotkee},
    d, log_error, log_warn,
    statusbar_manager::StatusbarWindow,
    windows_api::{self, WindowsApp},
    windows_handler::{UpdateKind, WindowsHandler},
};
use parking_lot::Mutex;
use std::sync::Arc;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, TranslateMessage,
};
use windows_api::WindowsEvent as E;

pub type Shared<T> = Arc<Mutex<T>>;

pub struct WindowsManager {
    handler: Shared<WindowsHandler>,
}
impl WindowsManager {
    pub fn new() -> Self {
        let config = Arc::new(
            ntek::from_str::<WinNtek>(include_str!("../wm.ntek"))
                .expect("Failed to parse win.ntek"),
        );
        let blacklist = &config.blacklist;
        let apps = windows_api::WinAPI::get_windows_app_list()
            .iter()
            .filter(|ap| !blacklist.contains(&ap.name))
            .cloned()
            .collect::<Vec<_>>();
        let statusbar_hwnds = Arc::new(Mutex::new(vec![]));
        let mut window_handler = WindowsHandler::new(apps, config.clone(), statusbar_hwnds.clone());
        window_handler.init();

        Self::spawn_border_window(window_handler.border.clone());
        Self::spawn_statusbar(statusbar_hwnds.clone());
        window_handler.spawn_widget();
        let handler = Arc::new(Mutex::new(window_handler));
        spawn_hotkee(config.clone(), handler.clone());
        Self::spawn_listener(handler.clone());
        Self { handler }
    }
    fn spawn_statusbar(hwnds: Arc<Mutex<Vec<isize>>>) {
        std::thread::spawn(move || {
            unsafe {
                let mut msg = MSG::default();
                _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            }
            let monitors = MonitorManager::fetch_all_monitors();
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

    fn spawn_border_window(border: Arc<Mutex<Option<BorderPainter>>>) {
        std::thread::spawn(move || {
            unsafe {
                let mut msg = MSG::default();
                _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            }

            match BorderPainter::new("Focus-Border") {
                Ok(painter) => {
                    *border.lock() = Some(painter);
                }
                Err(err) => {
                    println!("Border creation failed {}", err);
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
    fn debug_event(ev: &E, winapp: &WindowsApp) {
        if let Ok(data) = winapp.get_app_data() {
            log_error!("::", d!(ev), data.name, data.hwnd);
        }
    }
    fn spawn_listener(handler: Shared<WindowsHandler>) {
        let handler = handler.clone();
        std::thread::spawn(move || -> anyhow::Result<()> {
            while let Ok((event, winapp)) = windows_api::channel_receiver().recv() {
                match event {
                    E::Foreground => {
                        if let Ok(data) = winapp.get_app_data() {
                            let mut hd = handler.lock();
                            hd.update_app(&data, UpdateKind::Foreground)?;
                        }
                    }
                    E::CaptureStart => {}
                    E::CaptureEnd => {}
                    E::MoveSizeStart => {}
                    E::MoveSizeEnd => {
                        if let Ok(data) = winapp.get_app_data() {
                            let mut hd = handler.lock();
                            hd.update_app(&data, UpdateKind::MoveSize)?;
                        }
                    }
                    E::MinimizeStart => {}
                    E::MinimizeEnd => {
                        log_warn!("MinimizeEnd");
                    }
                    E::Reorder => {
                        log_warn!("Reorder");
                    }
                    E::Create => {
                        log_warn!("Create Event");
                        if let Ok(data) = winapp.get_app_data() {
                            let mut hd = handler.lock();
                            log_warn!("Create", &data.name);
                            hd.add_app(&data)?;
                        }
                    }
                    E::Destroy => {
                        log_warn!("Destroy Event");

                        let mut hd = handler.lock();
                        hd.delete_app(winapp.hwnd)?;
                    }
                    E::Show => {
                        log_warn!("Create Event");
                        if let Ok(data) = winapp.get_app_data() {
                            let mut hd = handler.lock();
                            hd.add_app(&data)?;
                        }
                    }
                    E::StateChange => {
                        log_warn!("StateChange");
                    }
                    E::LocationChange => {
                        if let Ok(data) = winapp.get_app_data() {
                            let mut hd = handler.lock();
                            hd.update_app(&data, UpdateKind::Location)?;
                        }
                    }
                    E::NameChange => {
                        if let Ok(data) = winapp.get_app_data() {
                            let mut hd = handler.lock();
                            hd.update_app(&data, UpdateKind::Title)?;
                        }
                    }
                    E::Unknown => {
                        log_warn!("Unknown");
                    }
                    E::Init => {}
                    E::Done => {}
                    E::Focus => {
                        log_warn!("Focus Change");
                    }
                }
            }
            Ok(())
        });
        std::thread::spawn(|| {
            windows_api::WinAPI::windows_app_listener();
        });
    }
    pub fn event_loop(&self) {
        loop {
            std::thread::park();
        }
    }
}
