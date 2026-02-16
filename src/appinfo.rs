use std::{ffi::OsString, os::windows::ffi::OsStringExt};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HWND, RECT},
        Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
        System::Threading::{
            OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
            QueryFullProcessImageNameW,
        },
        UI::WindowsAndMessaging::{
            GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowTextLengthW,
            GetWindowTextW, GetWindowThreadProcessId,
        },
    },
    core::PWSTR,
};

use crate::{api::BORDER_MANAGER, hwnd, win_api};
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
    pub fn get_appinfo(self) -> Option<AppInfo> {
        let hwnd = hwnd!(self.hwnd);
        let exe_path = Self::get_process_path(hwnd)?;
        let exe = exe_path.split('\\').next_back()?.to_string();
        // let rect = Self::get_rect(hwnd);
        let rect = Self::get_dwm_rect(hwnd, 0);
        let size = rect.0;
        let position = rect.1;
        let title = Self::get_app_title(hwnd)?;
        let class = Self::get_app_class(hwnd)?;
        let app_info = AppInfo {
            hwnd: self.hwnd,
            exe: exe,
            exe_path,
            size,
            position,
            title,
            class,
        };
        Some(app_info)
    }
    fn get_dwm_rect(hwnd: HWND, thickness: i32) -> (AppSize, AppPosition) {
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
        (AppSize { width, height }, AppPosition { x, y })
    }
    fn is_foreground(hwnd: HWND) -> bool {
        let fg = unsafe { GetForegroundWindow() };
        fg == hwnd
    }
    fn get_rect(hwnd: HWND) -> (AppSize, AppPosition) {
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
    pub fn get_app_title(hwnd: HWND) -> Option<String> {
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
    fn get_app_class(hwnd: HWND) -> Option<String> {
        let mut buffer: [u16; 256] = [0; 256];
        let copied = unsafe { GetClassNameW(hwnd, &mut buffer) };

        if copied > 0 {
            let class_name = OsString::from_wide(&buffer[..copied as usize]);
            Some(class_name.to_string_lossy().into_owned())
        } else {
            None
        }
    }
    fn get_process_path(hwnd: HWND) -> Option<String> {
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
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct AppPosition {
    pub x: i32,
    pub y: i32,
}
impl AppPosition {
    pub fn from_tuple(tuple: (i32, i32)) -> Self {
        Self {
            x: tuple.0,
            y: tuple.1,
        }
    }
    pub fn to_tuple(&self) -> (i32, i32) {
        (self.x, self.y)
    }
}
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct AppSize {
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct AppInfo {
    pub hwnd: isize,
    pub exe: String,
    pub exe_path: String,
    pub size: AppSize,
    pub position: AppPosition,
    pub title: String,
    pub class: String,
}

impl AppInfo {
    pub fn bring_to_front(&self) {
        println!("Bring To Front");
        win_api::bring_to_front(hwnd!(self.hwnd));
    }
    pub fn resize_app(&mut self, size: AppSize) {
        win_api::resize_window(hwnd!(self.hwnd), size.width, size.height);
        self.size = AppSize {
            width: size.width,
            height: size.height,
        }
    }
    fn get_rect(&self) -> RECT {
        let rect = unsafe {
            let mut rect = RECT::default();
            _ = GetWindowRect(hwnd!(self.hwnd), &mut rect);
            rect
        };
        rect
    }
    fn get_dwm_rect(&self) -> RECT {
        let rect = {
            let mut rect = RECT::default();
            unsafe {
                _ = DwmGetWindowAttribute(
                    hwnd!(self.hwnd),
                    DWMWA_EXTENDED_FRAME_BOUNDS,
                    &mut rect as *mut _ as _,
                    size_of::<RECT>() as u32,
                );
            }
            rect
        };
        rect
    }
    pub fn move_resize(&mut self, size: AppSize, pos: AppPosition) {
        let window_rect = self.get_rect();
        let extended = self.get_dwm_rect();

        let border_left = extended.left - window_rect.left;
        let border_top = extended.top - window_rect.top;
        let target_size = (
            size.width + (border_left * 2),
            size.height + (border_top * 2),
        );
        let target_pos = (pos.x - border_left, pos.y - border_top);
        win_api::resize_and_move_window(
            hwnd!(self.hwnd),
            target_size.0,
            target_size.1,
            target_pos.0,
            target_pos.1,
        );
        self.position = AppPosition {
            x: target_pos.0,
            y: target_pos.1,
        };
        self.size = AppSize {
            width: target_size.0,
            height: target_size.1,
        };
    }
    pub fn move_app(&mut self, target: AppPosition) {
        win_api::move_window(hwnd!(self.hwnd), target.x, target.y);
        self.position = target;
    }
    pub fn update_border(&self) {
        let (w, h) = (self.size.width, self.size.height);
        let (x, y) = (self.position.x, self.position.y);
        let hwnd = BORDER_MANAGER.lock().hwnd();
        win_api::transform(hwnd, x, y, w, h);
    }
}
