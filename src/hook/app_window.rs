use crate::{
    hook::{
        app_info::{AppInfo, SizeRatio},
        win_api,
    },
    hwnd,
};
use windows::Win32::Foundation::HWND;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct AppWindow {
    pub hwnd: isize,
}

impl From<HWND> for AppWindow {
    fn from(value: HWND) -> Self {
        Self {
            hwnd: value.0 as isize,
        }
    }
}

impl AppWindow {
    pub fn get_app_info(&self) -> Option<AppInfo> {
        let hwnd = hwnd!(self.hwnd);
        let exe_path = win_api::get_process_path(hwnd)?;
        let exe = exe_path.split('\\').next_back()?.to_string();
        // let rect = win_api::get_dwm_rect(hwnd, 2);
        let rect = win_api::get_rect(hwnd);
        let size = rect.0;
        let position = rect.1;
        let title = win_api::get_app_title(hwnd)?;
        let class = win_api::get_app_class(hwnd)?;
        Some(AppInfo {
            hwnd: self.hwnd,
            exe: exe,
            exe_path,
            size,
            position,
            title,
            class,
            column: crate::hook::app_info::Column::Left,
            size_ratio: SizeRatio {
                width: 1.0,
                height: 1.0,
            },
        })
    }
}
