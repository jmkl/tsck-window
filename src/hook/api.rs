use crate::{
    hook::{
        self, animation,
        app_info::{AppInfo, AppPosition, AppSize, Column, SizeRatio},
        app_window::AppWindow,
        border::{
            BorderManager, HwndItem, SlotText, StatusBar, StatusBarFont, Visibility, Workspace,
        },
        win_api::{self, BORDER_MANAGER, MonitorInfo},
        win_event::WinEvent,
    },
    hwnd,
};

use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};
use windows::Win32::UI::WindowsAndMessaging::{GW_HWNDNEXT, GetForegroundWindow, GetWindow};

pub type MonitorWidth = i32;
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
    pub left: Vec<SlotText>,
    pub center: Vec<SlotText>,
    pub right: Vec<SlotText>,
    pub workspace_indicator: WorkspaceIndicatorPosition,
}

impl Default for WidgetSlots {
    fn default() -> Self {
        Self {
            left: vec![],
            center: vec![],
            right: vec![],
            workspace_indicator: WorkspaceIndicatorPosition::Center, // default behavior
        }
    }
}
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
            current_active_workspace: 0,
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
    fn build_statusbar(&mut self, monitor_index: usize, always_show: Visibility) -> StatusBar {
        let ws = self.get_workspace_indicator(monitor_index);

        let mut left = self.user_widgets.left.clone();
        let mut center = self.user_widgets.center.clone();
        let mut right = self.user_widgets.right.clone();

        match self.user_widgets.workspace_indicator {
            WorkspaceIndicatorPosition::Left => {
                let mut merged = ws;
                merged.extend(left);
                left = merged;
            }
            WorkspaceIndicatorPosition::Center => {
                let mut merged = ws;
                merged.extend(center);
                center = merged;
            }
            WorkspaceIndicatorPosition::Right => {
                let mut merged = ws;
                merged.extend(right);
                right = merged;
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
            height: 20.0,
            padding: 10.0,
        }
    }

    fn get_workspace_indicator(&mut self, monitor_index: usize) -> Vec<SlotText> {
        self.workspaces
            .iter()
            .map(|ws| {
                let has_apps = ws.hwnds.iter().any(|h| h.monitor == monitor_index);
                SlotText {
                    text: ws.text.clone(),
                    foreground: if has_apps { 0xFFFFFF } else { 0x666666 },
                    background: if ws.active { 0xAC3E31 } else { 0x80000000 },
                }
            })
            .collect()
    }

    pub fn set_statusbar_center(&mut self, slots: Vec<SlotText>) {
        self.statusbar_center = slots;
    }
    pub fn set_statusbar_left(&mut self, slots: Vec<SlotText>) {
        self.statusbar_left = slots;
    }
    pub fn set_statusbar_right(&mut self, slots: Vec<SlotText>) {
        self.statusbar_right = slots;
    }

    pub fn set_widget(&mut self, slots: WidgetSlots) {
        self.user_widgets = slots;
        self.refresh_all_statusbars();
    }

    pub fn refresh_all_statusbars(&mut self) {
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

        // update active workspace flags
        for (idx, ws) in self.workspaces.iter_mut().enumerate() {
            ws.active = self.current_active_workspace == idx;
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
                    // always assign new apps to workspace 0, not current active
                    self.assign_app_to_workspace(0, app.hwnd, idx);
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
        // Use the monitor that owns the active app, not always monitor 0
        let (moni_w, moni_h) = {
            let active_hwnd = self.current_active_app_hwnd;
            let monitor_idx = self.monitor_index_for(active_hwnd);
            let monitor = self.monitors.get(monitor_idx)?;
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
        let (px, py) = self.get_rect_padding(active_hwnd);
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

    pub fn reset_size_selector(&mut self) {}

    /// Cycle the active app on whichever monitor the current app is on.
    pub fn cycle_active_app(&mut self, direction: &str) -> anyhow::Result<()> {
        let target_monitor = self.monitor_index_for(self.current_active_app_hwnd);

        // Use cached hwnd instead of locking BORDER_MANAGER again
        let border_hwnd = self.border_hwnds.get(target_monitor).copied().unwrap_or(0);
        let border_hwnd = hwnd!(border_hwnd);

        let hwnd_items: Vec<isize> = {
            let ws = self
                .workspaces
                .get(self.current_active_workspace)
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
            if let Some(app) = self.apps.get(&target_hwnd) {
                win_api::bring_to_front(hwnd!(app.hwnd), border_hwnd);
                self.current_active_app_hwnd = app.hwnd;
                self.update_border(app.hwnd);
            }
        }

        Ok(())
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

    pub fn cycle_window_width(&mut self, direction: &str) -> Option<()> {
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

    pub fn cycle_window_height(&mut self, direction: &str) -> Option<()> {
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
        let (moni_w, moni_h, active_hwnd, pos, size, px, py, _ratio, column) =
            self.get_app_props()?;
        let width = (self.size_factor[self.width_selector_index] * moni_w as f32) as i32;
        let height = (self.size_factor[self.height_selector_index] * moni_h as f32) as i32;
        let w = width + px;
        let h = height + py;
        let to_pos = match column {
            Column::Left => AppPosition { x: -(px / 2), y: 0 },
            Column::Right => AppPosition {
                x: (moni_w - (px / 2)) - width,
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

    pub fn cycle_position(&mut self, grid: Vec<(f32, f32, f32, f32)>) -> Option<()> {
        let (moni_w, moni_h, active_hwnd, pos, size, px, py, _ratio, _column) =
            self.get_app_props()?;
        self.app_position = (self.app_position + 1) % grid.len();
        if let Some((x, y, w, h)) = grid.get(self.app_position) {
            let x = (moni_w as f32 * x) as i32 - (px / 2);
            let y = (moni_h as f32 * y) as i32 - (py / 2);
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

    fn assign_app_to_workspace(&mut self, workspace_index: usize, hwnd: isize, monitor: usize) {
        if self.workspaces.is_empty() {
            self.workspaces.push(Workspace {
                text: "Main".to_string(),
                active: true,
                hwnds: vec![HwndItem {
                    hwnd,
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
            let is_active = self.current_active_workspace == workspace_index;
            for hitem in workspace.hwnds.iter_mut() {
                // Now works for ALL monitors, not just MONITOR_INDEX = 0
                if let Some(appinfo) = self.apps.get(&hitem.hwnd) {
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
        for (workspace_index, workspace) in self.workspaces.iter_mut().enumerate() {
            let is_active = self.current_active_workspace == workspace_index;
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
                                parked_pos,
                            );
                        } else {
                            hitem.parked_position = Some(appinfo.position.y);
                        }
                    } else if hitem.parked_position.is_some() {
                        win_api::set_app_position(hwnd!(hitem.hwnd), appinfo.position.x, -2000);
                    }
                }
            }
        }

        // update statusbar on ALL monitors so both indicators refresh
        {
            let border = BORDER_MANAGER.lock();
            for i in 0..self.monitors.len() {
                let ws_slots = self.get_workspace_indicator(i);
                let mut bar = self.build_statusbar(i, self.show_statusbar(i));
                bar.center = ws_slots;
                _ = border.update_statusbar(i, bar);
            }
        }

        // update border rect for active app
        if self.current_active_app_hwnd > 0 {
            let h = self.current_active_app_hwnd;
            self.update_border(h);
        }
        Ok(())
    }
    pub fn activate_workspace(&mut self, workspace: &str) {
        let cursor_monitor = win_api::get_monitor_index_from_cursor(&self.monitors);
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

        // update active flag immediately so indicator reflects new state
        for (idx, ws) in self.workspaces.iter_mut().enumerate() {
            ws.active = self.current_active_workspace == idx;
        }

        if let Err(err) = self.reorder_app_pos_in_workspace_for_monitor(cursor_monitor) {
            eprintln!("activate_workspace => {err}");
        }
    }

    pub fn move_active_app_to_workspace(&mut self, workspace: &str) -> anyhow::Result<()> {
        let active_hwnd = self
            .get_active_app()
            .ok_or(anyhow::anyhow!("active app not found"))?
            .hwnd;

        // Preserve the monitor the app currently lives on
        let monitor_index = self.monitor_index_for(active_hwnd);

        let count = self.workspaces.len();
        let workspace_index = match workspace {
            "Prev" => (self.current_active_workspace + count - 1) % count,
            "Next" => (self.current_active_workspace + 1) % count,
            _ => anyhow::bail!("unknown workspace direction: {}", workspace),
        };

        self.assign_app_to_workspace(workspace_index, active_hwnd, monitor_index);
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
            h.monitors = monitors; // set directly, skip update_monitors to avoid any lock issues
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
                            handler
                                .lock()
                                .update_apps(app_info, WinEvent::ObjectLocationchange);
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
                    WinEvent::SystemMovesizeend => {}
                    WinEvent::SystemMinimizeend => {}
                    _ => {}
                }
            }
        });
        loop {
            std::thread::park();
        }
    }
}
