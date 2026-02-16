use crate::{
    hook::{
        self, animation,
        app_info::{AppInfo, AppPosition, AppSize, Column, SizeRatio},
        win_api::{self, BORDER_MANAGER, MonitorInfo},
        win_event::WindowEvent,
    },
    hwnd,
};

use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};

const SIZE_FACTOR: &[f32] = &[1.0, 0.75, 0.66, 0.5, 0.33, 0.25];

pub type MonitorWidth = i32;
pub type MonitorHeight = i32;
pub type PadX = i32;
pub type PadY = i32;
pub type Hwnd = isize;

const BLACKLIST: &[&str] = &[
    "TextInputHost.exe",
    "msedgewebview2.exe",
    "Microsoft.CmdPal.UI.exe",
    "StartMenuExperienceHost.exe",
    "SearchHost.exe",
    // "ApplicationFrameHost.exe",
];

struct AppIndex {
    hwnd: isize,
    monitor: usize,
}

pub struct WindowHookHandler {
    active_app: isize,
    height_selector_index: usize,
    width_selector_index: usize,
    app_position: usize,
    apps: HashMap<isize, AppInfo>,
    monitors: Vec<MonitorInfo>,
    apps_index: Vec<AppIndex>,
}

impl WindowHookHandler {
    fn new() -> Self {
        Self {
            apps: HashMap::new(),
            monitors: Vec::new(),
            active_app: -1,
            height_selector_index: 0,
            width_selector_index: 0,
            app_position: 0,
            apps_index: Vec::new(),
        }
    }
    fn update_border(&self, app: &AppInfo) {
        let rect = win_api::get_dwm_rect(hwnd!(app.hwnd), 0);
        let hwnd = BORDER_MANAGER.lock().hwnd();
        win_api::set_badge_position(hwnd, hwnd!(app.hwnd), rect, 50, 5);
        // win_api::set_border_size_position(hwnd, hwnd!(app.hwnd), rect.l, rect.t, rect.w, rect.h);
    }

    pub fn update_apps(&mut self, app: AppInfo) {
        if BLACKLIST.contains(&app.exe.as_str()) {
            return;
        }
        if !self.apps_index.iter().any(|f| f.hwnd == app.hwnd) {
            if let Some(idx) = win_api::get_monitor_index(hwnd!(app.hwnd), &self.monitors) {
                println!("UPDATE \x1b[32m{}\x1b[0m x:pos{}", &app.exe, idx);
                self.apps_index.push(AppIndex {
                    hwnd: app.hwnd,
                    monitor: idx,
                });
            }
        }
        self.update_border(&app);
        if let Some(old_app) = self.apps.get_mut(&app.hwnd) {
            let old_ratio = old_app.size_ratio.clone();
            let old_column = old_app.column.clone();
            *old_app = AppInfo {
                size_ratio: old_ratio,
                column: old_column,
                ..app
            };
        } else {
            self.apps.insert(app.hwnd, app);
        }
    }
    pub fn delete_app(&mut self, app: AppInfo) {
        self.apps.remove(&app.hwnd);
    }
    pub fn update_monitors(&mut self, monitors: Vec<MonitorInfo>) {
        self.monitors.clear();
        self.monitors = monitors;
        println!("{:#?}", self.monitors);
    }

    pub fn get_active_app_position(&self) -> Option<(i32, i32)> {
        let position = &self.apps.get(&self.active_app)?.position;
        Some((position.x, position.y))
    }
    fn get_active_app(&mut self) -> Option<&mut AppInfo> {
        self.apps.get_mut(&self.active_app)
    }
    fn get_rect_padding(&self, hwnd: isize) -> (i32, i32) {
        let dwm_rect = win_api::get_dwm_rect(hwnd!(hwnd), 0);
        let rect = win_api::get_rect(hwnd!(hwnd));
        let x = rect.0.width - dwm_rect.w;
        let y = rect.0.height - dwm_rect.h;
        (x, y)
    }
    pub fn set_position(&mut self, x: i32, y: i32) {
        if let Some(app) = self.get_active_app() {
            win_api::set_app_position(hwnd!(app.hwnd), x, y);
        }
    }
    pub fn set_size(&mut self, w: i32, h: i32) {
        if let Some(app) = self.get_active_app() {
            win_api::set_app_size(hwnd!(app.hwnd), app.size.width + w, app.size.height + h);
        }
    }

    fn get_app_props(
        &mut self,
    ) -> Option<(
        MonitorWidth,
        MonitorHeight,
        Hwnd,
        AppPosition,
        AppSize,
        PadX,
        PadY,
        SizeRatio,
        Column,
    )> {
        let (moni_w, moni_h) = {
            let monitor = self.monitors.get(0)?;
            (monitor.width, monitor.height)
        };
        let (active_hwnd, pos, size, ratio, column) = {
            let app = self.get_active_app()?;
            (
                app.hwnd,
                app.position.clone(),
                app.size.clone(),
                app.size_ratio.clone(),
                app.column.clone(),
            )
        };
        let (px, py) = { self.get_rect_padding(active_hwnd) };
        Some((
            moni_w,
            moni_h,
            active_hwnd,
            pos,
            size,
            px,
            py,
            ratio,
            column,
        ))
    }

    fn go_animate(&mut self) -> Option<()> {
        let (moni_w, moni_h, active_hwnd, pos, size, px, py, ratio, column) =
            self.get_app_props()?;
        let width = (SIZE_FACTOR[self.width_selector_index] * (moni_w as f32)) as i32;
        let height = (SIZE_FACTOR[self.height_selector_index] * (moni_h as f32)) as i32;
        // let x = pos.x - (px / 2);
        // let y = pos.y - (py / 2);
        let w = width + (px);
        let h = height + (py);
        let to_pos = {
            match column {
                Column::Left => AppPosition { x: 0, y: 0 },
                Column::Right => AppPosition {
                    x: moni_w - width,
                    y: 0,
                },
            }
        };

        animation::animate_window(
            active_hwnd,
            pos,
            to_pos,
            size,
            AppSize::new(w, h),
            animation::AnimationEasing::EaseOutQuart,
        );
        Some(())
    }

    pub fn reset_size_selector(&mut self) {
        // self.width_selector_index = 0;
        // self.height_selector_index = 0;
    }
    pub fn cycle_column(&mut self) -> Option<()> {
        let app = self.get_active_app()?;
        let column = match app.column {
            hook::app_info::Column::Left => Column::Right,
            hook::app_info::Column::Right => Column::Left,
        };
        app.column = column;
        self.go_animate();
        Some(())
    }
    pub fn cycle_window_width(&mut self, direction: &str) -> Option<()> {
        self.width_selector_index = {
            let app = self.get_active_app()?;
            SIZE_FACTOR
                .iter()
                .position(|c| c == &app.size_ratio.width)?
        };
        println!("C-Width before {}", &self.width_selector_index);
        match direction {
            "Prev" => {
                self.width_selector_index =
                    ((self.width_selector_index + SIZE_FACTOR.len()) - 1) % SIZE_FACTOR.len();
            }
            "Next" => {
                self.width_selector_index = (self.width_selector_index + 1) % SIZE_FACTOR.len();
            }
            _ => {}
        }
        self.go_animate();
        {
            let idx = self.width_selector_index;
            let app = self.get_active_app()?;
            app.size_ratio.width = SIZE_FACTOR[idx];
            println!("C-Width after {}", &self.width_selector_index);
        }
        Some(())
    }
    pub fn cycle_window_height(&mut self, direction: &str) -> Option<()> {
        self.height_selector_index = {
            let app = self.get_active_app()?;
            SIZE_FACTOR
                .iter()
                .position(|c| c == &app.size_ratio.height)?
        };
        match direction {
            "Prev" => {
                self.height_selector_index =
                    ((self.height_selector_index + SIZE_FACTOR.len()) - 1) % SIZE_FACTOR.len();
            }
            "Next" => {
                self.height_selector_index = (self.height_selector_index + 1) % SIZE_FACTOR.len();
            }
            _ => {}
        }
        self.go_animate();
        {
            let idx = self.height_selector_index;
            let app = self.get_active_app()?;
            app.size_ratio.height = SIZE_FACTOR[idx];
        }
        Some(())
    }

    pub fn cycle_position(&mut self, grid: Vec<(f32, f32, f32, f32)>) -> Option<()> {
        let (moni_w, moni_h, active_hwnd, pos, size, px, py, ratio, column) =
            self.get_app_props()?;
        self.app_position = (self.app_position + 1) % grid.len();
        if let Some((x, y, w, h)) = grid.get(self.app_position) {
            let x = (moni_w as f32 * x) as i32 - (px / 2);
            let y = (moni_h as f32 * y) as i32 - (py / 2);
            let w = (moni_w as f32 * w) as i32 + (px);
            let h = (moni_h as f32 * h) as i32 + (py);
            animation::animate_window(
                active_hwnd,
                pos,
                AppPosition::new(x, y),
                size,
                AppSize::new(w, h),
                animation::AnimationEasing::EaseOutQuart,
            );
            // win_api::set_app_size_position(hwnd!(hwnd), x, y, w, h, true);
        }
        Some(())
    }
}

pub type ArcMutWHookHandler = Arc<Mutex<WindowHookHandler>>;
pub struct WindowHook {
    handler: ArcMutWHookHandler,
}

impl WindowHook {
    pub fn new() -> Self {
        Self {
            handler: Arc::new(Mutex::new(WindowHookHandler::new())),
        }
    }
    pub fn bind<F>(self, f: F) -> Self
    where
        F: FnOnce(ArcMutWHookHandler),
    {
        f(self.handler.clone());
        self
    }
    pub fn run(&self) {
        hook::win_api::init_winhook();
        let handler = self.handler.clone();
        {
            handler
                .lock()
                .update_monitors(hook::win_api::get_all_monitors());
        }
        std::thread::spawn(move || {
            while let Ok((ev, app_window)) = crate::hook::win_api::channel_receiver().recv() {
                match ev {
                    WindowEvent::ObjectCreate => {
                        if let Some(app_info) = app_window.get_app_info() {
                            handler.lock().update_apps(app_info);
                        }
                    }
                    // this event occure when window is moving
                    WindowEvent::ObjectLocationchange => {
                        if let Some(app_info) = app_window.get_app_info() {
                            handler.lock().update_apps(app_info);
                        }
                    }
                    // this event hit when window receive focus
                    WindowEvent::SystemForeground => {
                        if let Some(app_info) = app_window.get_app_info() {
                            let mut handler = handler.lock();
                            {
                                handler.active_app = app_info.hwnd;
                                handler.update_apps(app_info);
                                handler.reset_size_selector();
                            }
                        }
                    }
                    WindowEvent::ObjectDestroy => {
                        if let Some(app_info) = app_window.get_app_info() {
                            handler.lock().delete_app(app_info);
                        }
                    }
                    WindowEvent::SystemMovesizeend => {}
                    WindowEvent::SystemMinimizeend => {}
                    _ => {}
                }
            }
        });
        loop {
            std::thread::park();
        }
    }
}
