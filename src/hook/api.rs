use crate::{
    hook::{
        self, animation,
        app_info::{AppInfo, AppPosition, AppSize, Column, SizeRatio},
        border::{HwndItem, Workspace},
        win_api::{self, BORDER_MANAGER, MonitorInfo},
        win_event::WindowEvent,
    },
    hwnd,
};

use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};

const SIZE_FACTOR: &[f32] = &[1.0, 0.75, 0.66, 0.5, 0.33, 0.25];
const MONITOR_INDEX: usize = 0;

pub type MonitorWidth = i32;
pub type MonitorHeight = i32;
pub type PadX = i32;
pub type PadY = i32;
pub type Hwnd = isize;

// TODO: remove this
const _BLACKLIST: &[&str] = &[
    "TextInputHost.exe",
    "msedgewebview2.exe",
    "Microsoft.CmdPal.UI.exe",
    "StartMenuExperienceHost.exe",
    "SearchHost.exe",
    "ShellExperienceHost.exe", // "ApplicationFrameHost.exe",
];

pub struct WindowHookHandler {
    current_active_app_hwnd: Hwnd,
    current_active_workspace: usize,
    height_selector_index: usize,
    width_selector_index: usize,
    app_position: usize,
    apps: HashMap<isize, AppInfo>,
    monitors: Vec<MonitorInfo>,
    active_app_index: usize,
    blacklist: Vec<String>,
    workspaces: Vec<Workspace>,
    border_hwnd: Option<isize>,
}

impl WindowHookHandler {
    fn new(blacklist: Vec<String>, workspaces: Vec<String>) -> Self {
        Self {
            apps: HashMap::new(),
            monitors: Vec::new(),
            blacklist,
            current_active_app_hwnd: -1,
            height_selector_index: 0,
            width_selector_index: 0,
            app_position: 0,
            active_app_index: 0,
            border_hwnd: None,
            workspaces: workspaces
                .iter()
                .enumerate()
                .map(|(i, ws)| {
                    let active = i == 0;
                    Workspace {
                        text: ws.to_string(),
                        active,
                        hwnds: Vec::new(),
                    }
                })
                .collect::<Vec<_>>(),
            current_active_workspace: 0,
        }
    }
    fn update_border(&mut self, hwnd: Hwnd) {
        let rect = win_api::get_dwm_rect(hwnd!(hwnd), 0);
        let border = BORDER_MANAGER.lock();
        if self.border_hwnd.is_none() {
            self.border_hwnd = Some(border.hwnd().0 as isize);
        }
        for (idx, ws) in self.workspaces.iter_mut().enumerate() {
            if self.current_active_workspace == idx {
                ws.active = true;
            } else {
                ws.active = false
            }
        }
        _ = border.update_workspaces(self.workspaces.clone());
        if let Some((w, h)) = win_api::get_dwm_props(hwnd!(hwnd), rect.w, rect.h) {
            _ = border.update_rect_position(rect.l, rect.t);
            _ = border.update_rect_size(w, h);
        }
    }

    fn filter_app(&mut self, app: &AppInfo) -> bool {
        let is_blacklist = self.blacklist.contains(&app.exe);
        let mut total_widht = 0;
        for monitor in self.monitors.iter() {
            total_widht += monitor.width;
        }
        let windows = app.exe.contains("explorer.exe") && app.size.width == total_widht;
        is_blacklist || windows
    }

    pub fn update_apps(&mut self, app: AppInfo, event: WindowEvent) {
        if self.filter_app(&app) {
            return;
        }
        match event {
            WindowEvent::ObjectCreate => {
                if let Some(idx) = win_api::get_monitor_index(hwnd!(app.hwnd), &self.monitors) {
                    self.assign_app_to_workspace(self.current_active_workspace, app.hwnd, idx);
                }
            }
            WindowEvent::ObjectLocationchange | WindowEvent::SystemForeground => {
                if app.hwnd == self.current_active_app_hwnd {
                    self.update_border(app.hwnd);
                    self.update_app_parking_position(app.hwnd, app.position.y);
                }
            }
            _ => {}
        }

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
    }
    pub fn get_all_apps(&self) -> &HashMap<isize, AppInfo> {
        &self.apps
    }
    pub fn get_active_app_position(&self) -> Option<(i32, i32)> {
        let position = &self.apps.get(&self.current_active_app_hwnd)?.position;
        Some((position.x, position.y))
    }
    fn get_active_app(&mut self) -> Option<&mut AppInfo> {
        self.apps.get_mut(&self.current_active_app_hwnd)
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
    pub fn get_next(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
    pub fn reset_size_selector(&mut self) {
        // self.width_selector_index = 0;
        // self.height_selector_index = 0;
    }

    pub fn cycle_active_app(&mut self, direction: &str) -> anyhow::Result<()> {
        let mut hwnd_item = {
            let ws = self
                .workspaces
                .get(self.current_active_workspace)
                .ok_or(anyhow::anyhow!("Cant Get WorkSpace"))?;
            ws.hwnds
                .iter()
                .filter(|hwnd| hwnd.monitor == MONITOR_INDEX)
                .collect::<Vec<_>>()
        };
        if hwnd_item.len() == 0 {
            anyhow::bail!("App count is zeroo");
        }

        match direction {
            "Prev" => {
                self.active_app_index =
                    (self.active_app_index + hwnd_item.len() - 1) % hwnd_item.len();
            }
            "Next" => {
                self.active_app_index = (self.active_app_index + 1) % hwnd_item.len();
            }
            _ => {}
        };

        if let Some(hwnd) = hwnd_item.get(self.active_app_index) {
            if let Some(app) = self.apps.get(&hwnd.hwnd) {
                win_api::bring_to_front(hwnd!(app.hwnd));
                self.current_active_app_hwnd = app.hwnd;
                self.update_border(app.hwnd);
            } else {
                hwnd_item.remove(self.active_app_index);
                if self.active_app_index >= hwnd_item.len() && !hwnd_item.is_empty() {
                    self.active_app_index = hwnd_item.len() - 1;
                }
            }
        }

        Ok(())
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
                Column::Left => AppPosition { x: -(px / 2), y: 0 },
                Column::Right => AppPosition {
                    x: (moni_w - (px / 2)) - width,
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

//==============================================================================//
// tag         : WORKSPACE Handler
// description : this is where worskpace stupid lays
//==============================================================================//

impl WindowHookHandler {
    pub fn get_all_workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }

    pub fn create_workspace(&mut self, title: &str, monitor: usize) -> Option<()> {
        {
            self.workspaces.push(Workspace {
                text: title.into(),
                active: false,
                hwnds: Vec::new(),
            });
        }
        let app = {
            let app = self.get_active_app()?;
            app.clone()
        };
        self.update_border(app.hwnd);
        Some(())
    }
    fn assign_app_to_workspace(&mut self, workspace_index: usize, hwnd: isize, monitor: usize) {
        if self.workspaces.is_empty() {
            let hwnds = vec![HwndItem {
                hwnd,
                monitor,
                parked_position: None,
            }];
            self.workspaces.push(Workspace {
                text: "Main".to_string(),
                active: true,
                hwnds,
            });
        } else {
            for workspace in &mut self.workspaces {
                if let Some(index) = workspace.hwnds.iter().position(|f| f.hwnd == hwnd) {
                    workspace.hwnds.remove(index);
                }
            }
            if let Some(ws) = self.workspaces.get_mut(workspace_index) {
                ws.hwnds.push(HwndItem {
                    hwnd,
                    monitor,
                    parked_position: None,
                });
            }
        }
        if self.apps.contains_key(&hwnd) {
            _ = self.reorder_app_pos_in_workspace();
        }
    }
    // this is convenien resetter if moving y position going to shit
    // in development stage
    // basically it will set all y position to 0
    // and call it a day
    pub fn reset_y_position(&mut self) -> anyhow::Result<()> {
        for (_, app) in self.workspaces.iter_mut().enumerate() {
            for hwnd in app.hwnds.iter_mut() {
                let (_, ai) = self
                    .apps
                    .iter_mut()
                    .find(|(a, _)| *a == &hwnd.hwnd)
                    .ok_or(anyhow::anyhow!("cant find app"))?;
                win_api::set_app_position(hwnd!(ai.hwnd), ai.position.x, 0);
            }
        }
        Ok(())
    }
    fn update_app_parking_position(&mut self, target: Hwnd, position: i32) {
        if let Some(h) = self
            .workspaces
            .iter_mut()
            .filter(|w| w.active)
            .flat_map(|ws| ws.hwnds.iter_mut())
            .find(|h| h.hwnd == target)
        {
            h.parked_position = Some(position);
        }
    }
    fn reorder_app_pos_in_workspace(&mut self) -> anyhow::Result<()> {
        for (workspace_index, workspace) in self.workspaces.iter_mut().enumerate() {
            let is_active = self.current_active_workspace == workspace_index;
            for hitem in workspace.hwnds.iter_mut() {
                if hitem.monitor == MONITOR_INDEX {
                    if let Some((_, appinfo)) = self.apps.iter().find(|(hw, _)| *hw == &hitem.hwnd)
                    {
                        if is_active {
                            if let Some(parked_pos) = hitem.parked_position {
                                win_api::set_app_position(
                                    hwnd!(hitem.hwnd),
                                    appinfo.position.x,
                                    parked_pos,
                                );
                            } else {
                                hitem.parked_position = Some(appinfo.position.y);
                            }
                        } else {
                            if hitem.parked_position.is_some() {
                                win_api::set_app_position(
                                    hwnd!(hitem.hwnd),
                                    appinfo.position.x,
                                    -2000,
                                );
                            }
                        }
                    }
                }
            }
        }

        if self.current_active_app_hwnd > 0 {
            self.update_border(self.current_active_app_hwnd);
        }
        Ok(())
    }

    pub fn activate_workspace(&mut self, workspace: &str) {
        let ws_count = self.workspaces.len();
        match workspace {
            "Prev" => {
                self.current_active_workspace =
                    (self.current_active_workspace + ws_count - 1) % ws_count;
            }
            "Next" => {
                self.current_active_workspace = (self.current_active_workspace + 1) % ws_count;
            }
            _ => {}
        }

        if let Err(err) = self.reorder_app_pos_in_workspace() {
            eprintln!("update_app_position_in_workspace =>{err}");
        }
    }
    pub fn move_active_app_to_workspace(&mut self, workspace: &str) -> anyhow::Result<()> {
        let (active_hwnd, app_name) = {
            let app = self
                .get_active_app()
                .ok_or(anyhow::anyhow!("active app not found"))?;
            (app.hwnd, app.exe.clone())
        };
        let count = self.workspaces.len();
        let workspace_index = {
            match workspace {
                "Prev" => (self.current_active_workspace + count - 1) % count,
                "Next" => (self.current_active_workspace + 1) % count,
                _ => anyhow::bail!("unknown workspace direction: {}", workspace),
            }
        };
        self.assign_app_to_workspace(workspace_index, active_hwnd, MONITOR_INDEX);
        Ok(())
    }
}

pub type ArcMutWHookHandler = Arc<Mutex<WindowHookHandler>>;
pub struct WindowHook {
    handler: ArcMutWHookHandler,
}

impl WindowHook {
    pub fn new(blacklist: Vec<String>, workspaces: Vec<String>) -> Self {
        Self {
            handler: Arc::new(Mutex::new(WindowHookHandler::new(blacklist, workspaces))),
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
                    WindowEvent::ObjectShow => {
                        // App is being shown/unminimized - update info but don't reassign workspace
                        if let Some(app_info) = app_window.get_app_info() {
                            handler
                                .lock()
                                .update_apps(app_info, WindowEvent::ObjectShow);
                        }
                    }
                    WindowEvent::ObjectCreate => {
                        if let Some(app_info) = app_window.get_app_info() {
                            handler
                                .lock()
                                .update_apps(app_info, WindowEvent::ObjectCreate);
                        }
                    }
                    // this event occure when window is moving
                    WindowEvent::ObjectLocationchange => {
                        if let Some(app_info) = app_window.get_app_info() {
                            handler
                                .lock()
                                .update_apps(app_info, WindowEvent::ObjectLocationchange);
                        }
                    }
                    // this event hit when window receive focus
                    WindowEvent::SystemForeground => {
                        if let Some(app_info) = app_window.get_app_info() {
                            let mut handler = handler.lock();
                            {
                                handler.current_active_app_hwnd = app_info.hwnd;
                                handler.update_apps(app_info, WindowEvent::SystemForeground);
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
                    _ => {
                        // println!("[#] {:?}", ev);
                    }
                }
            }
        });
        loop {
            std::thread::park();
        }
    }
}
