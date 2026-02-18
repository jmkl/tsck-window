use crate::{
    appinfo::AppInfo, wh_handler::WinHookHandler, window_border::BorderManager,
    winhook::wm_event_channel_receiver,
};
use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};

lazy_static! {
    pub static ref APPINFO_LIST: Mutex<HashMap<isize, AppInfo>> = Mutex::new(HashMap::new());
    pub static ref BORDER_MANAGER: Mutex<BorderManager> = Mutex::new(BorderManager::new());
}

enum AppInfoUpdateStatus {
    Create,
    Delete,
    Update,
    Resized,
    Focused,
    Unknown,
}

const BLACKLIST: &[&str] = &[
    "Microsoft.CmdPal.UI.exe",
    "msedgewebview2.exe",
    "TextInputHost.exe",
    "ApplicationFrameHost.exe",
    "explorer.exe",
    "RtkUWP.exe",
];

pub type MutexWinHookHandler = Arc<Mutex<WinHookHandler>>;
pub struct WinHook {
    handler: MutexWinHookHandler,
}
impl WinHook {
    pub fn new() -> Self {
        Self {
            handler: Arc::new(Mutex::new(WinHookHandler::new())),
        }
    }
    pub fn bind<F>(self, f: F) -> Self
    where
        F: FnOnce(Arc<Mutex<WinHookHandler>>),
    {
        f(self.handler.clone());
        self
    }

    pub fn blacklist(info: &AppInfo) -> bool {
        let found = BLACKLIST.iter().find(|f| &&info.exe == f).is_some();
        found
    }

    fn update_applist(info: AppInfo) {
        let mut list = APPINFO_LIST.lock();
        if let Some(old_info) = list.get_mut(&info.hwnd) {
            *old_info = info;
        } else {
            list.insert(info.hwnd, info);
        }
        drop(list);
    }
    fn update_appinfo_list(info: AppInfo, status: AppInfoUpdateStatus) {
        match status {
            AppInfoUpdateStatus::Create | AppInfoUpdateStatus::Update => {
                Self::update_applist(info);
            }
            AppInfoUpdateStatus::Resized => {
                Self::update_applist(info);
            }
            AppInfoUpdateStatus::Focused => {
                Self::update_applist(info);
            }
            AppInfoUpdateStatus::Delete => {
                let mut list = APPINFO_LIST.lock();
                list.remove(&info.hwnd);
                drop(list);
            }
            AppInfoUpdateStatus::Unknown => {}
        }
    }

    pub fn run(&self) {
        crate::winhook::init();
        let manager = BORDER_MANAGER.lock().clone();
        std::thread::spawn(move || manager.run_message_loop());
        let handler = self.handler.clone();
        std::thread::spawn(move || {
            while let Ok((ev, app_window)) = wm_event_channel_receiver().recv() {
                match ev.as_str() {
                    "EVENT_OBJECT_CREATE" => {
                        if let Some(info) = app_window.get_appinfo() {
                            handler.lock().init();
                            Self::update_appinfo_list(info, AppInfoUpdateStatus::Create);
                        }
                    }
                    "EVENT_OBJECT_SHOW" => {
                        // debug_info!(app_window, "SHOW", exe);
                    }
                    "EVENT_OBJECT_LOCATIONCHANGE" => {
                        // debug_info!(app_window, "EVENT_OBJECT_LOCATIONCHANGE", exe, size);

                        //minimize position -32000 -32000
                        if let Some(info) = app_window.get_appinfo() {
                            info.update_border();
                            // Self::update_appinfo_list(info, AppInfoUpdateStatus::Update);
                        }
                    }
                    // "EVENT_OBJECT_NAMECHANGE" => {}
                    "EVENT_SYSTEM_FOREGROUND" => {
                        // debug_info!(app_window, "EVENT_SYSTEM_FOREGROUND", exe, size);
                        if let Some(info) = app_window.get_appinfo() {
                            info.update_border();
                            if let Some(info) = app_window.get_appinfo() {
                                handler.lock().focus_app(info.hwnd);
                                Self::update_appinfo_list(info, AppInfoUpdateStatus::Focused);
                            }
                        }
                    }
                    // "EVENT_OBJECT_FOCUS" => {}
                    // "EVENT_SYSTEM_CAPTURESTART" => {}
                    // "EVENT_OBJECT_STATECHANGE" => {}
                    // "EVENT_SYSTEM_CAPTUREEND" => {}
                    "EVENT_OBJECT_DESTROY" => {
                        // debug_info!(app_window, "DESTROY", exe);
                        if let Some(info) = app_window.get_appinfo() {
                            Self::update_appinfo_list(info, AppInfoUpdateStatus::Delete);
                        }
                    }
                    // "EVENT_SYSTEM_MOVESIZESTART" => {}
                    "EVENT_SYSTEM_MOVESIZEEND" => {
                        if let Some(info) = app_window.get_appinfo() {
                            info.update_border();
                            let w = info.size.width;
                            let h = info.size.height;
                            let hwnd = info.hwnd;
                            Self::update_appinfo_list(info, AppInfoUpdateStatus::Resized);
                            handler.lock().handle_resize_by_hwnd(0, hwnd, w, h);
                        }
                    }
                    "EVENT_OBJECT_REORDER" => {
                        // debug_info!(app_window, "MIN?MAX", exe);
                    }
                    // "EVENT_OBJECT_HIDE" => {}
                    "EVENT_SYSTEM_MINIMIZEEND" => {
                        if let Some(info) = app_window.get_appinfo() {
                            info.update_border();
                            Self::update_appinfo_list(info, AppInfoUpdateStatus::Update);
                        }
                    }
                    "EVENT_SYSTEM_MINIMIZESTART" => {
                        // debug_info!(app_window, "MINIMIZED", exe);
                    }
                    // "EVENT_OBJECT_VALUECHANGE" => {}
                    // "EVENT_OBJECT_SELECTIONWITHIN" => {}
                    // "EVENT_OBJECT_HELPCHANGE" => {}
                    // "EVENT_OBJECT_PARENTCHANGE" => {}
                    // "EVENT_OBJECT_SELECTIONREMOVE" => {}
                    // "EVENT_OBJECT_CLOAKED" => {}
                    // "EVENT_OBJECT_UNCLOAKED" => {}
                    // "EVENT_OBJECT_DESCRIPTIONCHANGE" => {}
                    // "EVENT_OBJECT_SELECTION" => {}
                    _ => {
                        //debug_info!("?", app_window, exe, ev);
                    }
                }
            }
        });

        crate::winhook::hook_win_event();
    }
}

#[cfg(test)]
mod apitest {
    #[test]
    fn test1() {
        println!("TEST !");
    }
}
