use crate::{
    hook::{
        self, animation,
        app_info::{AppInfo, AppPosition, AppSize, Column, SizeRatio},
        app_window::AppWindow,
        border::{HwndItem, SlotText, StatusBar, StatusBarFont, Visibility, Workspace},
        color,
        win_api::{self, BORDER_MANAGER, MonitorInfo},
        win_event::WinEvent,
    },
    hwnd, slot_text,
};

use parking_lot::Mutex;
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use windows::Win32::UI::WindowsAndMessaging::{GW_HWNDNEXT, GetForegroundWindow, GetWindow};

pub type MonitorWidth = i32;
pub type MonitorLeft = i32;
pub type MonitorHeight = i32;
pub type PadX = i32;
pub type PadY = i32;
pub type Hwnd = isize;

const _BLACKLIST: &[&str] = &[
    "TextInputHost.exe",
    "msedgewebview2.exe",
    "Microsoft.CmdPal.UI.exe",
    "StartMenuExperienceHost.exe",
    "SearchHost.exe",
    "ShellExperienceHost.exe",
];
pub enum WorkspaceIndicatorPosition {
    Left,
    Center,
    Right,
    None, // hide it
}

pub struct WidgetSlots {
    pub left: BTreeMap<String, Vec<SlotText>>,
    pub center: BTreeMap<String, Vec<SlotText>>,
    pub right: BTreeMap<String, Vec<SlotText>>,
    pub workspace_indicator: WorkspaceIndicatorPosition,
}

impl Default for WidgetSlots {
    fn default() -> Self {
        Self {
            left: BTreeMap::new(),
            center: BTreeMap::new(),
            right: BTreeMap::new(),
            workspace_indicator: WorkspaceIndicatorPosition::Center,
        }
    }
}
pub struct WindowHookHandler {
    current_active_app_hwnd: Hwnd,
    active_workspace_per_monitor: Vec<usize>,
    height_selector_index: usize,
    width_selector_index: usize,
    app_position: usize,
    apps: HashMap<isize, AppInfo>,
    monitors: Vec<MonitorInfo>,
    active_app_index: usize,
    blacklist: Vec<String>,
    workspaces: Vec<Workspace>,
    border_hwnds: Vec<isize>,
    size_factor: Vec<f32>,
    statusbar_left: Vec<SlotText>,
    statusbar_center: Vec<SlotText>,
    statusbar_right: Vec<SlotText>,
    pub user_widgets: WidgetSlots,
}

impl WindowHookHandler {
    fn new(blacklist: Vec<String>, workspaces: Vec<String>, size_factor: Vec<f32>) -> Self {
        Self {
            apps: HashMap::new(),
            monitors: Vec::new(),
            blacklist,
            current_active_app_hwnd: -1,
            height_selector_index: 0,
            width_selector_index: 0,
            app_position: 0,
            active_app_index: 0,
            border_hwnds: Vec::new(),
            size_factor,
            statusbar_left: vec![],
            statusbar_center: vec![],
            statusbar_right: vec![],
            workspaces: workspaces
                .iter()
                .enumerate()
                .map(|(i, ws)| Workspace {
                    text: ws.to_string(),
                    active: i == 0,
                    hwnds: Vec::new(),
                })
                .collect(),
            active_workspace_per_monitor: vec![0; 2],
            user_widgets: WidgetSlots {
                workspace_indicator: WorkspaceIndicatorPosition::Center,
                ..Default::default()
            },
        }
    }

    pub fn init_active_appinfo(&mut self) -> anyhow::Result<AppInfo> {
        let mut current = unsafe { GetForegroundWindow() };
        if current.0.is_null() {
            anyhow::bail!("No current window");
        }
        while !current.0.is_null() {
            if self.apps.contains_key(&(current.0 as isize)) {
                self.current_active_app_hwnd = current.0 as isize;
                let app_info = AppWindow::from(current)
                    .get_app_info()
                    .ok_or(anyhow::anyhow!("AppInfo Not Found"))?;
                return Ok(app_info);
            }
            current = unsafe { GetWindow(current, GW_HWNDNEXT) }?;
        }
        anyhow::bail!("App info not found");
    }

    // -----------------------------------------------------------------------
    // Border helpers
    // -----------------------------------------------------------------------

    /// Resolve which monitor index a given hwnd lives on.
    fn monitor_index_for(&self, hwnd: Hwnd) -> usize {
        win_api::get_monitor_index(hwnd!(hwnd), &self.monitors).unwrap_or(0)
    }

    /// Cache the border HWNDs from BorderManager (called once monitors are known).
    fn sync_border_hwnds(&mut self) {
        let border = BORDER_MANAGER.lock();
        self.border_hwnds = (0..self.monitors.len())
            .filter_map(|i| border.hwnd_for(i).map(|h| h.0 as isize))
            .collect();
    }

    /// Return the border HWND for a specific monitor index, if one exists.
    fn border_hwnd_for(&self, monitor_index: usize) -> Option<isize> {
        self.border_hwnds.get(monitor_index).copied()
    }
    // fn build_statusbar(&self, always_show: Visibility) -> StatusBar {
    //     StatusBar {
    //         font: StatusBarFont {
    //             family: "JetBrainsMono Nerd Font".into(),
    //             size: 10.0,
    //         },
    //         always_show,
    //         height: 20.0,
    //         padding: 10.0,
    //         left: self.statusbar_left.clone(),
    //         center: self.statusbar_center.clone(),
    //         right: self.statusbar_right.clone(),
    //     }
    // }
    fn flatten_slots(map: &BTreeMap<String, Vec<SlotText>>) -> Vec<SlotText> {
        map.values().flatten().cloned().collect()
    }
    fn build_statusbar(&mut self, monitor_index: usize, always_show: Visibility) -> StatusBar {
        let ws = self.get_workspace_indicator(monitor_index);

        let mut left = Self::flatten_slots(&self.user_widgets.left);
        let mut center = Self::flatten_slots(&self.user_widgets.center);
        let mut right = Self::flatten_slots(&self.user_widgets.right);

        match self.user_widgets.workspace_indicator {
            WorkspaceIndicatorPosition::Left => {
                let mut m = ws;
                m.extend(left);
                left = m;
            }
            WorkspaceIndicatorPosition::Center => {
                let mut m = ws;
                m.extend(center);
                center = m;
            }
            WorkspaceIndicatorPosition::Right => {
                let mut m = ws;
                m.extend(right);
                right = m;
            }
            WorkspaceIndicatorPosition::None => {}
        }

        StatusBar {
            left,
            center,
            right,
            font: StatusBarFont {
                family: "JetBrainsMono Nerd Font".into(),
                size: 10.0,
            },
            always_show,
            height: win_api::get_toolbar_height(monitor_index) as f32,
            padding: 10.0,
        }
    }

    fn get_workspace_indicator(&mut self, monitor_index: usize) -> Vec<SlotText> {
        let active = self.active_workspace_for(monitor_index);
        self.workspaces
            .iter()
            .enumerate()
            .map(|(idx, ws)| {
                let has_apps = ws.hwnds.iter().any(|h| h.monitor == monitor_index);
                SlotText {
                    text: ws.text.clone(),
                    foreground: if has_apps { 0xFFFFFF } else { 0x666666 },
                    background: if active == idx { 0xAC3E31 } else { 0x80000000 },
                }
            })
            .collect()
    }

    pub fn set_slot_left(&mut self, key: impl Into<String>, slots: Vec<SlotText>) {
        self.user_widgets.left.insert(key.into(), slots);
        self.refresh_all_statusbars();
    }

    pub fn set_slot_center(&mut self, key: impl Into<String>, slots: Vec<SlotText>) {
        self.user_widgets.center.insert(key.into(), slots);
        self.refresh_all_statusbars();
    }

    pub fn set_slot_right(&mut self, key: impl Into<String>, slots: Vec<SlotText>) {
        self.user_widgets.right.insert(key.into(), slots);
        self.refresh_all_statusbars();
    }

    pub fn remove_slot_left(&mut self, key: &str) {
        self.user_widgets.left.remove(key);
        self.refresh_all_statusbars();
    }

    pub fn remove_slot_center(&mut self, key: &str) {
        self.user_widgets.center.remove(key);
        self.refresh_all_statusbars();
    }

    pub fn remove_slot_right(&mut self, key: &str) {
        self.user_widgets.right.remove(key);
        self.refresh_all_statusbars();
    }

    fn refresh_all_statusbars(&mut self) {
        let border = BORDER_MANAGER.lock();
        for i in 0..self.monitors.len() {
            _ = border.update_statusbar(i, self.build_statusbar(i, self.show_statusbar(i)));
        }
    }
    fn show_statusbar(&self, monitor: usize) -> Visibility {
        if monitor == 0 {
            Visibility::Always
        } else {
            Visibility::Disable
        }
    }

    fn update_border(&mut self, hwnd: Hwnd) {
        let app_monitor = self.monitor_index_for(hwnd);
        let rect = win_api::get_dwm_rect(hwnd!(hwnd), 0);

        for (idx, ws) in self.workspaces.iter_mut().enumerate() {
            // mark active per monitor — just use monitor 0's active as the global flag
            // since ws.active is only used for legacy checks, drive it from app_monitor
            ws.active = self
                .active_workspace_per_monitor
                .get(0)
                .copied()
                .unwrap_or(0)
                == idx;
        }

        let border = BORDER_MANAGER.lock();

        if self.border_hwnds.is_empty() {
            self.border_hwnds = (0..self.monitors.len())
                .filter_map(|i| border.hwnd_for(i).map(|h| h.0 as isize))
                .collect();
        }

        // always rebuild ALL monitors from user_widgets so clock etc never get wiped
        for i in 0..self.monitors.len() {
            _ = border.update_statusbar(i, self.build_statusbar(i, self.show_statusbar(i)));
        }

        _ = border.set_active_monitor(app_monitor);

        if let Some((w, h)) = win_api::get_dwm_props(hwnd!(hwnd), rect.w, rect.h) {
            let (mon_left, mon_top) = self
                .monitors
                .get(app_monitor)
                .map(|m| (m.left, m.top))
                .unwrap_or((0, 0));
            _ = border.update_rect_position_on(app_monitor, rect.l - mon_left, rect.t - mon_top);
            _ = border.update_rect_size_on(app_monitor, w, h);
        }
    }
    fn filter_app(&mut self, app: &AppInfo) -> bool {
        let is_blacklist = self.blacklist.contains(&app.exe);
        let total_width: i32 = self.monitors.iter().map(|m| m.width).sum();
        let is_fullscreen_explorer =
            app.exe.contains("explorer.exe") && app.size.width == total_width;
        is_blacklist || is_fullscreen_explorer
    }

    pub fn force_border_to_top(&self, hwnd: isize) {
        let target_monitor = self.monitor_index_for(hwnd);
        let border_hwnd = self.border_hwnds.get(target_monitor).copied().unwrap_or(0);
        win_api::force_border_to_front(hwnd!(border_hwnd));
    }

    pub fn update_apps(&mut self, app: AppInfo, event: WinEvent) {
        if self.filter_app(&app) {
            return;
        }
        match event {
            WinEvent::ObjectCreate => {
                if let Some(idx) = win_api::get_monitor_index(hwnd!(app.hwnd), &self.monitors) {
                    self.assign_app_to_workspace(0, app.hwnd, app.exe.clone(), idx);
                }
            }
            WinEvent::ObjectShow => {
                if let Some(monitor_index) =
                    win_api::get_monitor_index(hwnd!(app.hwnd), &self.monitors)
                {
                    let active_workspace = self
                        .active_workspace_per_monitor
                        .get(monitor_index)
                        .copied()
                        .unwrap_or(0);
                    self.assign_app_to_workspace(
                        active_workspace,
                        app.hwnd,
                        app.exe.clone(),
                        monitor_index,
                    );
                    println!(
                        "Monitor Index {} Active Monitor {}",
                        monitor_index, active_workspace
                    );
                }
            }
            WinEvent::Done => {
                self.update_border(app.hwnd);
                self.update_app_parking_position(app.hwnd, app.position.y);
            }
            WinEvent::ObjectLocationchange | WinEvent::SystemForeground => {
                if app.hwnd == self.current_active_app_hwnd {
                    self.update_border(app.hwnd);
                    self.update_app_parking_position(app.hwnd, app.position.y);
                    self.force_border_to_top(app.hwnd);
                    self.update_widget_active_app(&app.exe, &app.title);
                    self.refresh_all_statusbars();
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
        if let Some(ws) = self
            .workspaces
            .iter_mut()
            .find(|ws| ws.hwnds.iter().any(|h| h.hwnd == app.hwnd))
        {
            ws.hwnds.retain(|h| h.hwnd != app.hwnd);
            self.current_active_app_hwnd = -1;
            self.update_border(self.current_active_app_hwnd);
        }
    }

    pub fn update_monitors(&mut self, monitors: Vec<MonitorInfo>) {
        self.monitors = monitors;
        // Don't call sync_border_hwnds here — it would lock BORDER_MANAGER
        // which may already be locked upstream. Let update_border handle it lazily.
        self.border_hwnds.clear(); // just invalidate the cache
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

    // fn get_rect_padding(&self, hwnd: isize) -> (i32, i32) {
    //     let dwm_rect = win_api::get_dwm_rect(hwnd!(hwnd), 0);
    //     let rect = win_api::get_rect(hwnd!(hwnd));
    //     let x = rect.0.width - dwm_rect.w;
    //     let y = rect.0.height - dwm_rect.h;
    //     (x, y)
    // }

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
        MonitorLeft,
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
        let (moni_left, moni_w, moni_h, monitor_index) = {
            let active_hwnd = self.current_active_app_hwnd;
            let monitor_idx = self.monitor_index_for(active_hwnd);
            let monitor = self.monitors.get(monitor_idx)?;

            (monitor.left, monitor.width, monitor.height, monitor_idx)
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
        let (px, py) = win_api::get_rect_padding(active_hwnd);
        Some((
            moni_left,
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

    pub fn reset_size_selector(&mut self) {}

    /// Cycle the active app on whichever monitor the current app is on.
    pub fn cycle_active_app(&mut self, direction: &str) -> anyhow::Result<()> {
        let target_monitor = self.monitor_index_for(self.current_active_app_hwnd);
        let active_ws = self.active_workspace_for(target_monitor);
        let border_hwnd = self.border_hwnds.get(target_monitor).copied().unwrap_or(0);
        let border_hwnd = hwnd!(border_hwnd);

        let hwnd_items: Vec<isize> = {
            let ws = self
                .workspaces
                .get(active_ws)
                .ok_or(anyhow::anyhow!("Can't get workspace"))?;
            ws.hwnds
                .iter()
                .filter(|h| h.monitor == target_monitor)
                .map(|h| h.hwnd)
                .collect()
        };

        if hwnd_items.is_empty() {
            anyhow::bail!("No apps on monitor {}", target_monitor);
        }

        match direction {
            "Prev" => {
                self.active_app_index =
                    (self.active_app_index + hwnd_items.len() - 1) % hwnd_items.len();
            }
            "Next" => {
                self.active_app_index = (self.active_app_index + 1) % hwnd_items.len();
            }
            _ => {}
        }

        if let Some(&target_hwnd) = hwnd_items.get(self.active_app_index) {
            let (hwnd, exe, title) = {
                let app = self
                    .apps
                    .get(&target_hwnd)
                    .ok_or(anyhow::anyhow!("Cant find the app"))?;
                (app.hwnd, app.exe.clone(), app.title.clone())
            };
            win_api::bring_to_front(hwnd!(hwnd), border_hwnd);
            self.current_active_app_hwnd = hwnd;
            self.update_border(hwnd);
            self.update_widget_active_app(&exe, &title);
        }

        Ok(())
    }
    fn update_widget_active_app(&mut self, app_name: &str, title: &str) {
        self.set_slot_left(
            "active-app",
            vec![
                slot_text!("{}", app_name, color::FOREGROUND, color::PRIMARY),
                slot_text!("{}", title, color::FOREGROUND, color::BACKGROUND),
            ],
        );
    }
    pub fn cycle_column(&mut self) -> Option<()> {
        let app = self.get_active_app()?;
        app.column = match app.column {
            hook::app_info::Column::Left => Column::Right,
            hook::app_info::Column::Right => Column::Left,
        };
        self.go_animate();
        Some(())
    }

    pub fn reposition_app_on_monitor(&mut self, app: &AppInfo) {
        let toolbar_height = win_api::get_toolbar_height(self.monitor_index_for(app.hwnd));
        if app.position.y < toolbar_height {
            win_api::set_app_position(hwnd!(app.hwnd), app.position.x, toolbar_height);
            self.update_app_parking_position(app.hwnd, app.position.y.max(toolbar_height));
            self.update_border(app.hwnd);
        }
    }

    pub fn cycle_app_width(&mut self, direction: &str) -> Option<()> {
        let size_factor = self.size_factor.clone();
        self.width_selector_index = {
            let app = self.get_active_app()?;
            size_factor
                .iter()
                .position(|c| c == &app.size_ratio.width)?
        };
        match direction {
            "Prev" => {
                self.width_selector_index = (self.width_selector_index + self.size_factor.len()
                    - 1)
                    % self.size_factor.len();
            }
            "Next" => {
                self.width_selector_index =
                    (self.width_selector_index + 1) % self.size_factor.len();
            }
            _ => {}
        }
        self.go_animate();
        {
            let idx = self.width_selector_index;
            let app = self.get_active_app()?;
            app.size_ratio.width = size_factor[idx];
        }
        Some(())
    }

    pub fn cycle_app_height(&mut self, direction: &str) -> Option<()> {
        let size_factor = self.size_factor.clone();
        self.height_selector_index = {
            let app = self.get_active_app()?;
            size_factor
                .iter()
                .position(|c| c == &app.size_ratio.height)?
        };
        match direction {
            "Prev" => {
                self.height_selector_index = (self.height_selector_index + self.size_factor.len()
                    - 1)
                    % self.size_factor.len();
            }
            "Next" => {
                self.height_selector_index =
                    (self.height_selector_index + 1) % self.size_factor.len();
            }
            _ => {}
        }
        self.go_animate();
        {
            let idx = self.height_selector_index;
            let app = self.get_active_app()?;
            app.size_ratio.height = size_factor[idx];
        }
        Some(())
    }

    fn go_animate(&mut self) -> Option<()> {
        let (moni_left, moni_w, moni_h, active_hwnd, pos, size, px, py, _ratio, column) =
            self.get_app_props()?;
        let width = (self.size_factor[self.width_selector_index] * moni_w as f32) as i32;
        let height = (self.size_factor[self.height_selector_index] * moni_h as f32) as i32;
        let w = width + px;
        let h = height + py;
        let to_pos = match column {
            Column::Left => AppPosition {
                x: moni_left + (-(px / 2)),
                y: 0,
            },
            Column::Right => AppPosition {
                x: moni_left + ((moni_w - (px / 2)) - width),
                y: 0,
            },
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
    pub fn fake_maximize(&mut self) -> Option<()> {
        let (moni_left, moni_w, moni_h, active_hwnd, pos, size, px, py, _ratio, column) =
            self.get_app_props()?;
        let width = (self.size_factor[self.width_selector_index] * moni_w as f32) as i32;
        let height = (self.size_factor[self.height_selector_index] * moni_h as f32) as i32;
        let toolbar_height = win_api::get_toolbar_height(self.monitor_index_for(active_hwnd));

        let w = width + px;
        let h = height + (py / 2);
        let x = moni_left + (-(px / 2));
        let y = toolbar_height;
        win_api::set_app_size_position(hwnd!(active_hwnd), x, y, w, h, true);
        Some(())
    }

    pub fn cycle_app_on_grid(&mut self, grid: Vec<(f32, f32, f32, f32)>) -> Option<()> {
        let (moni_left, moni_w, moni_h, active_hwnd, pos, size, px, py, _ratio, _column) =
            self.get_app_props()?;
        let toolbar_height = win_api::get_toolbar_height(self.monitor_index_for(active_hwnd));
        self.app_position = (self.app_position + 1) % grid.len();
        if let Some((x, y, w, h)) = grid.get(self.app_position) {
            let x = moni_left + ((moni_w as f32 * x) as i32 - (px / 2));
            let y = (moni_h as f32 * y) as i32 - (py / 2) + toolbar_height;
            let w = (moni_w as f32 * w) as i32 + px;
            let h = (moni_h as f32 * h) as i32 + py;
            animation::animate_window(
                active_hwnd,
                pos,
                AppPosition::new(x, y),
                size,
                AppSize::new(w, h),
                animation::AnimationEasing::EaseOutQuart,
            );
        }
        Some(())
    }
}

//==============================================================================//
// WORKSPACE Handler
//==============================================================================//

impl WindowHookHandler {
    pub fn get_all_workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }
    fn active_workspace_for(&self, monitor: usize) -> usize {
        self.active_workspace_per_monitor
            .get(monitor)
            .copied()
            .unwrap_or(0)
    }
    pub fn create_workspace(&mut self, title: &str, _monitor: usize) -> Option<()> {
        self.workspaces.push(Workspace {
            text: title.into(),
            active: false,
            hwnds: Vec::new(),
        });
        let app = self.get_active_app()?.clone();
        self.update_border(app.hwnd);
        Some(())
    }

    fn get_active_workspace_monitor(&self) -> (usize, usize) {
        let active_monitor = win_api::get_monitor_index_from_cursor(&self.monitors);
        let active_workspace = self
            .active_workspace_per_monitor
            .get(active_monitor)
            .copied()
            .unwrap_or(0);
        (active_monitor, active_workspace)
    }

    fn assign_app_to_workspace(
        &mut self,
        workspace_index: usize,
        hwnd: isize,
        app_name: String,
        monitor: usize,
    ) {
        if self.workspaces.is_empty() {
            self.workspaces.push(Workspace {
                text: "Main".to_string(),
                active: true,
                hwnds: vec![HwndItem {
                    hwnd,
                    app_name,
                    monitor,
                    parked_position: None,
                }],
            });
        } else {
            // Remove from whichever workspace currently holds it
            for workspace in &mut self.workspaces {
                workspace.hwnds.retain(|h| h.hwnd != hwnd);
            }
            if let Some(ws) = self.workspaces.get_mut(workspace_index) {
                ws.hwnds.push(HwndItem {
                    hwnd,
                    app_name,
                    monitor,
                    parked_position: None,
                });
            }
        }
        if self.apps.contains_key(&hwnd) {
            _ = self.reorder_app_pos_in_workspace();
        }
    }

    pub fn reset_y_position(&mut self) -> anyhow::Result<()> {
        for ws in self.workspaces.iter_mut() {
            for hwnd_item in ws.hwnds.iter_mut() {
                let ai = self
                    .apps
                    .get(&hwnd_item.hwnd)
                    .ok_or(anyhow::anyhow!("can't find app"))?;
                win_api::set_app_position(hwnd!(ai.hwnd), ai.position.x, 0);
            }
        }
        Ok(())
    }

    fn update_app_parking_position(&mut self, target: Hwnd, position: i32) {
        for ws in self.workspaces.iter_mut().filter(|w| w.active) {
            if let Some(h) = ws.hwnds.iter_mut().find(|h| h.hwnd == target) {
                h.parked_position = Some(position);
            }
        }
    }
    fn reorder_app_pos_in_workspace(&mut self) -> anyhow::Result<()> {
        for (workspace_index, workspace) in self.workspaces.iter_mut().enumerate() {
            for hitem in workspace.hwnds.iter_mut() {
                let is_active = self
                    .active_workspace_per_monitor
                    .get(hitem.monitor)
                    .copied()
                    .unwrap_or(0)
                    == workspace_index;
                if let Some(appinfo) = self.apps.get(&hitem.hwnd) {
                    if is_active {
                        let toolbar_height = {
                            let idx =
                                win_api::get_monitor_index(hwnd!(appinfo.hwnd), &self.monitors)
                                    .unwrap_or(0);
                            win_api::get_toolbar_height(idx)
                        };
                        if let Some(parked_pos) = hitem.parked_position {
                            win_api::set_app_position(
                                hwnd!(hitem.hwnd),
                                appinfo.position.x,
                                parked_pos.max(toolbar_height),
                            );
                        } else {
                            hitem.parked_position = Some(appinfo.position.y.max(toolbar_height));
                        }
                    } else if hitem.parked_position.is_some() {
                        win_api::set_app_position(hwnd!(hitem.hwnd), appinfo.position.x, -2000);
                    }
                }
            }
        }
        if self.current_active_app_hwnd > 0 {
            let h = self.current_active_app_hwnd;
            self.update_border(h);
        }
        Ok(())
    }

    fn reorder_app_pos_in_workspace_for_monitor(&mut self, monitor: usize) -> anyhow::Result<()> {
        let active_ws = self.active_workspace_for(monitor);
        for (workspace_index, workspace) in self.workspaces.iter_mut().enumerate() {
            let is_active = active_ws == workspace_index;
            for hitem in workspace.hwnds.iter_mut() {
                if hitem.monitor != monitor {
                    continue;
                }
                if let Some(appinfo) = self.apps.get(&hitem.hwnd) {
                    if is_active {
                        if let Some(parked_pos) = hitem.parked_position {
                            win_api::set_app_position(
                                hwnd!(hitem.hwnd),
                                appinfo.position.x,
                                parked_pos.max(win_api::get_toolbar_height(monitor)),
                            );
                        } else {
                            hitem.parked_position =
                                Some(appinfo.position.y.max(win_api::get_toolbar_height(monitor)));
                        }
                    } else if hitem.parked_position.is_some() {
                        win_api::set_app_position(hwnd!(hitem.hwnd), appinfo.position.x, -2000);
                    }
                }
            }
        }

        let border = BORDER_MANAGER.lock();
        for i in 0..self.monitors.len() {
            _ = border.update_statusbar(i, self.build_statusbar(i, self.show_statusbar(i)));
        }
        if self.current_active_app_hwnd > 0 {
            let h = self.current_active_app_hwnd;
            drop(border);
            self.update_border(h);
        }
        Ok(())
    }
    pub fn activate_workspace(&mut self, workspace: &str) {
        let cursor_monitor = win_api::get_monitor_index_from_cursor(&self.monitors);
        let ws_count = self.workspaces.len();
        let current = self.active_workspace_for(cursor_monitor);

        let next = match workspace {
            "Prev" => (current + ws_count - 1) % ws_count,
            "Next" => (current + 1) % ws_count,
            _ => return,
        };

        if let Some(slot) = self.active_workspace_per_monitor.get_mut(cursor_monitor) {
            *slot = next;
        }

        if let Err(err) = self.reorder_app_pos_in_workspace_for_monitor(cursor_monitor) {
            eprintln!("activate_workspace => {err}");
        }
    }
    pub fn close_active_app(&mut self) -> anyhow::Result<()> {
        let (hwnd, exe) = {
            let app = self
                .get_active_app()
                .ok_or(anyhow::anyhow!("Cant find active app"))?;
            (app.hwnd, app.exe.clone())
        };
        if let Err(err) = win_api::close_app(hwnd!(hwnd)) {
            println!("Failed to close the app {exe} cause :{err}");
        }
        Ok(())
    }
    pub fn move_active_app_to_workspace(&mut self, workspace: &str) -> anyhow::Result<()> {
        let (hwnd, exe) = {
            let app = self
                .get_active_app()
                .ok_or(anyhow::anyhow!("Cant find active app"))?;
            (app.hwnd, app.exe.clone())
        };
        let (monitor_index, current) = {
            let monitor_index = self.monitor_index_for(hwnd);
            let current = self.active_workspace_for(monitor_index);
            (monitor_index, current)
        };
        let count = self.workspaces.len();
        let workspace_index = match workspace {
            "Prev" => (current + count - 1) % count,
            "Next" => (current + 1) % count,
            _ => anyhow::bail!("unknown workspace direction: {}", workspace),
        };
        self.assign_app_to_workspace(workspace_index, hwnd, exe, monitor_index);
        self.activate_workspace(workspace);
        Ok(())
    }
}

pub type ArcMutWHookHandler = Arc<Mutex<WindowHookHandler>>;

pub struct WindowHook {
    handler: ArcMutWHookHandler,
}

impl WindowHook {
    pub fn new(blacklist: Vec<String>, workspaces: Vec<String>, size_factor: Vec<f32>) -> Self {
        Self {
            handler: Arc::new(Mutex::new(WindowHookHandler::new(
                blacklist,
                workspaces,
                size_factor,
            ))),
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
            let mut h = handler.lock();
            let monitors = hook::win_api::get_all_monitors();
            h.monitors = monitors;
        }
        std::thread::spawn(move || {
            while let Ok((ev, app_window)) = crate::hook::win_api::channel_receiver().recv() {
                match ev {
                    WinEvent::ObjectShow => {
                        if let Some(app_info) = app_window.get_app_info() {
                            handler.lock().update_apps(app_info, WinEvent::ObjectShow);
                        }
                    }
                    WinEvent::Done => {
                        let mut handler = handler.lock();
                        match handler.init_active_appinfo() {
                            Ok(app_info) => handler.update_apps(app_info, WinEvent::Done),
                            Err(err) => {
                                eprintln!("Error initializing active hwnd: `{err}`");
                            }
                        }
                    }
                    WinEvent::ObjectCreate => {
                        if let Some(app_info) = app_window.get_app_info() {
                            handler.lock().update_apps(app_info, WinEvent::ObjectCreate);
                        }
                    }
                    WinEvent::ObjectLocationchange => {
                        if let Some(app_info) = app_window.get_app_info() {
                            if let Ok(res) = win_api::is_window_maximized(hwnd!(app_info.hwnd)) {
                                if res {
                                    let mut handler = handler.lock();
                                    handler.fake_maximize();
                                }
                            }
                            {
                                let mut handler = handler.lock();
                                handler.update_apps(app_info, WinEvent::ObjectLocationchange);
                            }
                        }
                    }
                    WinEvent::SystemForeground => {
                        if let Some(app_info) = app_window.get_app_info() {
                            let mut handler = handler.lock();
                            handler.current_active_app_hwnd = app_info.hwnd;
                            handler.update_apps(app_info, WinEvent::SystemForeground);
                            handler.reset_size_selector();
                        }
                    }
                    WinEvent::ObjectDestroy => {
                        if let Some(app_info) = app_window.get_app_info() {
                            handler.lock().delete_app(app_info);
                        }
                    }
                    WinEvent::SystemMovesizeend | WinEvent::SystemMinimizeend => {
                        if let Some(app_info) = app_window.get_app_info() {
                            let mut handler = handler.lock();
                            handler.reposition_app_on_monitor(&app_info);
                        }
                    }
                    _ => {
                        // println!("EVENT {:?}", ev);
                    }
                }
            }
        });
        loop {
            std::thread::park();
        }
    }
}
