use crate::{
    hwnd,
    overlay::{
        animation,
        app_border::BorderInfo,
        app_info::{AppInfo, AppPosition, AppSize, Column, SizeRatio},
        color,
        config::CycleDirection,
        manager::{OptBorderOverlay, STATUSBAR_HEIGHT, Shared, WM_UPDATE_STATUSBAR},
        monitor_info::StatusbarMonitorInfo,
        statusbar::{SlotText, StatusBar, StatusBarFont, Visibility},
        sys::{SystemInfo, format_speed},
        widget::{SlotGrid, WidgetSlots, WorkspaceIndicatorPosition},
        win_api,
        win_event::WinEvent,
        workspaces::{Hwnd, HwndItem, Workspace},
    },
};
use anyhow::{Context, Result, anyhow, bail};
use parking_lot::Mutex;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    thread,
    time::Duration,
};

#[derive(Debug)]
struct AppProps<'a> {
    monitor: &'a StatusbarMonitorInfo,
    active_hwnd: isize,
    position: &'a AppPosition,
    size: &'a AppSize,
    px: i32,
    py: i32,
    app: &'a AppInfo,
    size_ratio: &'a SizeRatio,
    column: &'a Column,
}
#[derive(Clone)]
pub struct OverlayHandler {
    pub blacklist: Vec<String>,
    height_selector_index: usize,
    pub statusbar: Shared<Vec<isize>>,
    width_selector_index: usize,
    pub current_active_app: Option<Hwnd>,
    pub apps: HashMap<isize, AppInfo>,
    pub monitors: Vec<StatusbarMonitorInfo>,
    pub size_factor: Vec<f32>,
    pub user_widgets: Shared<WidgetSlots>,
    pub grid_app_position: usize,
    pub border_overlay: OptBorderOverlay,
    pub top_most_apps: HashSet<isize>,
}
impl OverlayHandler {
    pub fn new() -> Self {
        Self {
            height_selector_index: 0,
            width_selector_index: 0,
            current_active_app: None,
            statusbar: Arc::new(Mutex::new(vec![])),
            apps: HashMap::new(),
            blacklist: vec![],
            monitors: vec![],
            size_factor: vec![],
            top_most_apps: HashSet::new(),
            grid_app_position: 0,
            border_overlay: Arc::new(Mutex::new(None)),
            user_widgets: Arc::new(Mutex::new(WidgetSlots {
                workspace_indicator: WorkspaceIndicatorPosition::Left,
                hwnd: None,
                ..Default::default()
            })),
        }
    }
    pub fn assign_app_to_workspace(
        &mut self,
        workspace_index: usize,
        hwnd: Hwnd,
        app_name: &str,
        monitor: usize,
    ) {
        {
            let mut guard = self.user_widgets.lock();
            let workspaces = guard.get_workspaces();
            if workspaces.is_empty() {
                workspaces.push(Workspace::new(
                    "Main",
                    vec![HwndItem::new(hwnd, app_name, monitor)],
                ));
            } else {
                for ws in &mut *workspaces {
                    ws.hwnds.retain(|h| h.hwnd != hwnd);
                }
                if let Some(ws) = workspaces.get_mut(workspace_index) {
                    ws.hwnds.push(HwndItem::new(hwnd, app_name, monitor));
                }
            }
        }
        if self.apps.contains_key(&hwnd) {
            self.reorder_app_pos_in_workspace();
        }
    }
    pub fn update_active_app(&mut self, hwnd: isize) {
        self.current_active_app = Some(hwnd);
    }
    pub fn reset_size_selector(&mut self) {}
    pub fn delete_app(&mut self, app: &AppInfo) {
        self.apps.remove(&app.hwnd);
        if let Some(ref overlay) = *self.border_overlay.lock() {
            overlay.clear_focus();
            overlay.remove_topmost(app.hwnd as isize);
        }
        if let Some(ws) = self
            .user_widgets
            .lock()
            .workspaces
            .iter_mut()
            .find(|ws| ws.hwnds.iter().any(|h| h.hwnd == app.hwnd))
        {
            ws.hwnds.retain(|h| h.hwnd != app.hwnd);
            self.current_active_app = None;
        }
    }
    fn filter_app(&mut self, app: &AppInfo) -> bool {
        let is_blacklist = self.blacklist.contains(&app.exe);
        let total_width: i32 = self.monitors.iter().map(|m| m.width).sum();
        let is_fullscreen_explorer =
            app.exe.contains("explorer.exe") && app.size.width == total_width;
        is_blacklist || is_fullscreen_explorer
    }

    pub fn update_app_title(&mut self, app: &AppInfo) {
        if let Some(active) = self.current_active_app {
            if active == app.hwnd {
                let appname = app.exe.strip_suffix(".exe").unwrap_or(app.exe.as_str());
                self.user_widgets.lock().set_slot(
                    SlotGrid::Left,
                    "active-app",
                    vec![
                        SlotText::new(" "),
                        SlotText::new(appname)
                            .bg(color::WARNING)
                            .fg(color::BG)
                            .bold(),
                        SlotText::new(app.title.as_str()).italic(),
                    ],
                );
            }
        }
    }

    pub fn update_apps(&mut self, app: AppInfo, event: WinEvent) {
        match event {
            WinEvent::ObjectCreate => {
                if self.filter_app(&app) {
                    return;
                }
                if win_api::is_top_most(hwnd!(app.hwnd)) {
                    self.top_most_apps.insert(app.hwnd);
                }
                let monitor =
                    win_api::get_monitor_index(hwnd!(app.hwnd), &self.monitors).unwrap_or(0);
                self.assign_app_to_workspace(0, app.hwnd, &app.exe, monitor);
            }
            WinEvent::ObjectShow => {
                if self.filter_app(&app) {
                    return;
                }
                let monitor =
                    win_api::get_monitor_index(hwnd!(app.hwnd), &self.monitors).unwrap_or(0);
                let active_workspace = self
                    .user_widgets
                    .lock()
                    .get_active_workspace_for_monitor(monitor);
                self.assign_app_to_workspace(active_workspace, app.hwnd, &app.exe, monitor);
            }
            WinEvent::Done => {
                self.update_app_parking_position(app.hwnd, app.position.y);
            }
            WinEvent::ObjectLocationchange => {
                self.update_border(&app);
                if app.hwnd == self.current_active_app.unwrap_or_default() {
                    //force check the app location
                }
            }
            WinEvent::SystemForeground => {
                let active_hwnd = {
                    let active = self.current_active_app;
                    active
                };
                match active_hwnd {
                    Some(hwnd) => {
                        if hwnd != app.hwnd {
                            self.current_active_app = Some(hwnd);
                        }
                    }
                    None => {
                        self.current_active_app = Some(app.hwnd);
                    }
                }
                {
                    self.update_app_title(&app);
                }
            }
            _ => {}
        }
        if self.filter_app(&app) {
            return;
        }
        if let Some(old_app) = self.apps.get_mut(&app.hwnd) {
            let old_ratio = old_app.size_ratio.clone();
            let old_column = old_app.column.clone();
            *old_app = AppInfo {
                size_ratio: old_ratio,
                column: old_column,
                ..app
            }
        } else {
            self.apps.insert(app.hwnd, app);
        }
    }

    pub fn update_app_parking_position(&mut self, hwnd: Hwnd, ypos: i32) {
        for ws in self
            .user_widgets
            .lock()
            .workspaces
            .iter_mut()
            .filter(|w| w.active)
        {
            if let Some(h) = ws.hwnds.iter_mut().find(|h| h.hwnd == hwnd) {
                h.parked_position = Some(ypos);
            }
        }
    }
    fn monitor_index_for(&self, hwnd: Hwnd) -> usize {
        win_api::get_monitor_index(hwnd!(hwnd), &self.monitors).unwrap_or(0)
    }
    pub fn get_statusbar_height(&self, monitor: usize) -> i32 {
        if monitor == 0 {
            STATUSBAR_HEIGHT as i32
        } else {
            0
        }
    }
    fn get_props(&self) -> Option<AppProps<'_>> {
        let active_hwnd = self.current_active_app?;
        let monitor_index = self.monitor_index_for(active_hwnd);
        let monitor = self.monitors.get(monitor_index)?;
        let app = self.apps.get(&active_hwnd)?;
        let (px, py) = win_api::get_rect_padding(active_hwnd);

        Some(AppProps {
            monitor: monitor,
            active_hwnd,
            position: &app.position,
            size: &app.size,
            app,
            px,
            py,
            size_ratio: &app.size_ratio,
            column: &app.column,
        })
    }
    pub fn fake_maximize(&mut self) -> Option<()> {
        let props = self.get_props()?;
        let width =
            (self.size_factor[self.width_selector_index] * props.monitor.width as f32) as i32;
        let height =
            (self.size_factor[self.height_selector_index] * props.monitor.height as f32) as i32;
        let toolbar_height = self.get_statusbar_height(self.monitor_index_for(props.active_hwnd));

        let w = width + props.px;
        let h = height + (props.py / 2) - toolbar_height;
        let x = props.monitor.x + (-(props.px / 2));
        let y = toolbar_height;
        win_api::set_app_size_position(hwnd!(props.active_hwnd), x, y, w, h, true);
        Some(())
    }

    //==============================================================================//
    // tag         : MONITOR BITS
    // description :
    //==============================================================================//
    fn get_active_monitor(&self) -> usize {
        win_api::get_monitor_index_from_cursor(&self.monitors)
    }
    //==============================================================================//
    // tag         : APP POSITION and SIZE
    // description : this part where we manipulate size and position of the app
    //==============================================================================//
    // GETTER
    pub fn get_all_apps(&self) {
        todo!()
    }
    // SETTER
    pub fn update_border(&self, app: &AppInfo) -> Option<()> {
        let active = self.current_active_app?;
        let is_top_most = self.top_most_apps.contains(&app.hwnd);
        // let is_top_most = win_api::is_top_most(hwnd!(active));

        let overlay = self.border_overlay.lock();
        let overlay = overlay.as_ref()?;

        win_api::force_border_to_front(overlay.hwnd());

        const PADDING: i32 = 2;
        let is_maximized = win_api::is_maximized(app.hwnd);
        let (px, py) = win_api::get_rect_padding(app.hwnd);
        let y = if is_maximized {
            app.position.y + (py / 2)
        } else {
            app.position.y
        };

        let info = BorderInfo {
            x: app.position.x + (px / 2) + PADDING / 2,
            y: y + PADDING / 2,
            width: app.size.width - (px) - PADDING,
            height: app.size.height - (py) - PADDING,
            color: if is_top_most {
                color::Theme::WARNING
            } else {
                color::Theme::DANGER
            },
            thickness: 2.0,
            radius: 5.0,
        };

        if app.hwnd == active {
            overlay.set_focus(app.hwnd, info.clone());
        }
        if self.top_most_apps.contains(&app.hwnd) {
            if app.position.y > -1000 {
                overlay.set_topmost(app.hwnd as isize, info);
            } else {
                overlay.remove_topmost(app.hwnd as isize);
            }
        } else {
            overlay.remove_topmost(app.hwnd as isize);
        }

        Some(())
    }

    pub fn toggle_top_most(&mut self) -> Option<()> {
        let active_app = self.current_active_app?;
        let app = self.apps.get(&active_app)?;
        let overlay = self.border_overlay.lock();
        let overlay = overlay.as_ref()?;
        let topmost = win_api::toggle_top_most(hwnd!(app.hwnd), overlay.hwnd());
        if topmost {
            self.top_most_apps.insert(app.hwnd);
        } else {
            self.top_most_apps.remove(&app.hwnd);
        }
        Some(())
    }

    pub fn set_app_position(&self, x: i32, y: i32) {
        todo!()
    }
    pub fn set_app_size(&self, width: i32, height: i32) {
        todo!()
    }
    pub fn cycle_app_on_grid(&mut self, grid: &[(f32, f32, f32, f32)]) -> Option<()> {
        if grid.is_empty() {
            return None;
        }
        self.grid_app_position = (self.grid_app_position + 1) % grid.len();
        let props = self.get_props()?;
        let moni = props.monitor;
        let toolbar_height = self.get_statusbar_height(self.monitor_index_for(props.active_hwnd));
        if let Some((x, y, w, h)) = grid.get(self.grid_app_position) {
            let x = moni.x + ((moni.width as f32 * x) as i32 - (props.px / 2));
            let y = (moni.height as f32 * y) as i32 - (props.py / 2) + toolbar_height;
            let w = (moni.width as f32 * w) as i32 + props.px;
            let h = (moni.height as f32 * h) as i32 + props.py - toolbar_height;
            animation::animate_window(
                props.active_hwnd,
                props.position.clone(),
                AppPosition::new(x, y),
                props.size.clone(),
                AppSize::new(w, h),
                animation::AnimationEasing::EaseOutQuart,
            );
        }
        Some(())
    }
    pub fn cycle_app_width(&self, direction: &str) {
        todo!()
    }
    pub fn cycle_app_height(&self, direction: &str) {
        todo!()
    }
    pub fn cycle_column(&self, direction: &str) {
        todo!()
    }
    pub fn reset_all_position(&self) {
        todo!()
    }
    pub fn close_active_app(&self) {
        todo!()
    }
    //==============================================================================//
    // tag         : WORKSPACE
    // description : this is workspace area
    //==============================================================================//
    pub fn get_all_workspaces(&self) {
        todo!()
    }
    pub fn reset_position(&self) -> Result<()> {
        for ws in self.user_widgets.lock().workspaces.iter_mut() {
            for hwnd_item in ws.hwnds.iter_mut() {
                let ai = self
                    .apps
                    .get(&hwnd_item.hwnd)
                    .ok_or(anyhow::anyhow!("can't find app"))?;
                win_api::set_app_position(
                    hwnd!(ai.hwnd),
                    ai.position.x,
                    self.get_statusbar_height(hwnd_item.monitor),
                );
            }
        }
        Ok(())
    }

    pub fn go_to_workspace(&self, direction: &CycleDirection) {
        let mut userwidget = self.user_widgets.lock();
        let active_monitor = self.get_active_monitor();
        let workspace_count = userwidget.workspaces.len();
        let monitor = self.get_active_monitor();
        let statusbar_height = self.get_statusbar_height(monitor);

        if let Some(active_workspace) = userwidget
            .active_workspace_per_monitor
            .get_mut(active_monitor)
        {
            *active_workspace = match direction {
                CycleDirection::Prev => (*active_workspace + workspace_count - 1) % workspace_count,
                CycleDirection::Next => (*active_workspace + 1) % workspace_count,
            };
        }
        let active_workspace = userwidget
            .active_workspace_per_monitor
            .get(active_monitor)
            .copied()
            .unwrap_or(0);
        // Update app position
        for (wi, workspace) in userwidget.workspaces.iter_mut().enumerate() {
            let is_active = wi == active_workspace;

            for hitem in workspace.hwnds.iter_mut() {
                if hitem.monitor != monitor {
                    continue;
                }

                if let Some(appinfo) = self.apps.get(&hitem.hwnd) {
                    if is_active {
                        if let Some(parked_pos) = hitem.parked_position {
                            animation::animate_position(
                                appinfo.hwnd,
                                appinfo,
                                AppPosition {
                                    x: appinfo.position.x,
                                    y: parked_pos.max(statusbar_height),
                                },
                                animation::AnimationEasing::EaseInBounce,
                            );
                            // win_api::set_app_position(
                            //     hwnd!(appinfo.hwnd),
                            //     appinfo.position.x,
                            //     parked_pos.max(statusbar_height),
                            // );
                        } else {
                            hitem.parked_position = Some(appinfo.position.y.max(statusbar_height));
                        }
                    } else if hitem.parked_position.is_some() {
                        animation::animate_position(
                            appinfo.hwnd,
                            appinfo,
                            AppPosition {
                                x: appinfo.position.x,
                                y: -2000,
                            },
                            animation::AnimationEasing::EaseInOutCirc,
                        );
                        // win_api::set_app_position(hwnd!(appinfo.hwnd), appinfo.position.x, -2000);
                    }
                }
            }
        }

        userwidget.refresh_statusbar();
    }
    pub fn create_workspace(&self, title: &str, monitor_index: usize) {
        todo!()
    }
    pub fn move_active_to_workspace(&mut self, workspace: &CycleDirection) -> anyhow::Result<()> {
        let (workspace_index, hwnd, exe, moni_index, count, current) = {
            let props = self.get_props().ok_or(anyhow!("Cant find app"))?;
            let (count, current) = {
                let w = self.user_widgets.lock();
                let count = w.workspaces.len();
                let current = w.get_active_workspace_for_monitor(props.monitor.index);
                (count, current)
            };
            let workspace_index = match workspace {
                CycleDirection::Prev => (current + count - 1) % count,
                CycleDirection::Next => (current + 1) % count,
            };
            (
                workspace_index,
                props.active_hwnd,
                props.app.exe.clone(),
                props.monitor.index,
                count,
                current,
            )
        };
        self.assign_app_to_workspace(workspace_index, hwnd, &exe, moni_index);
        self.go_to_workspace(workspace);
        Ok(())
    }

    pub fn arrange_workspaces(&self) {
        self.reorder_app_pos_in_workspace();
    }
    fn debug_app(&self, ws: usize, monitor: usize, app: &AppInfo) {
        println!(
            "{} {:<3} {:<20} {} {}",
            ws, monitor, app.exe, app.position, app.size
        );
    }
    fn reorder_app_pos_in_workspace(&self) {
        let mut workspaces = {
            let ws = self.user_widgets.lock();
            ws.workspaces.clone()
        };
        let guard = self.user_widgets.lock();
        for (index, workspace) in workspaces.iter_mut().enumerate() {
            for item in workspace.hwnds.iter_mut() {
                let is_active = guard.get_active_workspace_for_monitor(item.monitor) == index;
                if let Some(app) = self.apps.get(&item.hwnd) {
                    if is_active {
                        let statusbar_height = self.get_statusbar_height(item.monitor);
                        if let Some(parked_pos) = item.parked_position {
                            win_api::set_app_position(
                                hwnd!(app.hwnd),
                                app.position.x,
                                parked_pos.max(statusbar_height),
                            );
                        } else {
                            item.parked_position = Some(app.position.y.max(statusbar_height));
                        }
                    } else if item.parked_position.is_some() {
                        win_api::set_app_position(hwnd!(app.hwnd), app.position.x, 2000);
                    }
                    self.debug_app(index, item.monitor, app);
                }
            }
        }
    }
    //==============================================================================//
    // tag         : STATUSBAR BITS
    // description : slot for statusbar
    //==============================================================================//
    // fn user_widget_mut(&self) -> parking_lot::MutexGuard<'_, WidgetSlots> {
    //     self.user_widgets.lock()
    // }

    // pub fn set_slot(&self, grid: SlotGrid, key: impl Into<String>, slots: Vec<SlotText>) {
    //     {
    //         let mut user_widgets = self.user_widget_mut();
    //         match grid {
    //             SlotGrid::Left => {
    //                 user_widgets.left.insert(key.into(), slots);
    //             }
    //             SlotGrid::Center => {
    //                 user_widgets.center.insert(key.into(), slots);
    //             }
    //             SlotGrid::Right => {
    //                 user_widgets.right.insert(key.into(), slots);
    //             }
    //         }
    //     }
    //     self.refresh_statusbar();
    // }

    // fn refresh_statusbar(&self) {
    //     let widgets = self.user_widget_mut();
    //     let ws = self.get_workspace_indicator(self.get_active_monitor());
    //     let mut left = widgets.left.values().flatten().cloned().collect();
    //     let mut center = widgets.center.values().flatten().cloned().collect();
    //     let mut right = widgets.right.values().flatten().cloned().collect();
    //     match widgets.workspace_indicator {
    //         WorkspaceIndicatorPosition::Left => {
    //             let mut m = ws;
    //             m.extend(left);
    //             left = m;
    //         }
    //         WorkspaceIndicatorPosition::Center => {
    //             let mut m = ws;
    //             m.extend(center);
    //             center = m;
    //         }
    //         WorkspaceIndicatorPosition::Right => {
    //             let mut m = ws;
    //             m.extend(right);
    //             right = m;
    //         }
    //         WorkspaceIndicatorPosition::None => {}
    //     }
    //     let statusbar = StatusBar {
    //         left,
    //         center,
    //         right,
    //         height: STATUSBAR_HEIGHT,
    //         padding: 10.0,
    //         always_show: Visibility::OnFocus,
    //         font: StatusBarFont {
    //             family: "MartianMono NF".into(),
    //             size: 10.0,
    //         },
    //     };
    //     _ = self.update_statusbar(0, statusbar);
    // }
    // pub fn update_statusbar(&self, monitor_index: usize, bar: StatusBar) -> anyhow::Result<()> {
    //     let raw = loop {
    //         if let Some(&r) = self.statusbar.lock().get(monitor_index) {
    //             break r;
    //         }
    //         std::thread::sleep(std::time::Duration::from_millis(10));
    //     };
    //     let hwnd = HWND(raw as *mut std::ffi::c_void);
    //     unsafe {
    //         PostMessageW(
    //             Some(hwnd),
    //             WM_UPDATE_STATUSBAR,
    //             WPARAM(Box::into_raw(Box::new(bar)) as usize),
    //             LPARAM(0),
    //         )?;
    //     }
    //     Ok(())
    // }

    //==============================================================================//
    // tag         : WIDGET
    // description :
    //==============================================================================//
    pub fn spawn_widget(&self) {
        let user_widget = self.user_widgets.clone();

        let hwnd = loop {
            if let Some(&r) = self.statusbar.lock().get(0) {
                break r;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        {
            user_widget.lock().set_hwnd(Some(hwnd));
        }
        std::thread::spawn(move || {
            let mut info = SystemInfo::new();

            loop {
                let time = chrono::Local::now().format("%H:%M %a, %d %h").to_string();
                let usage = info.update();
                let bg = color::DANGER;
                let fg = color::DARK_FG;
                {
                    let mut me = user_widget.lock();
                    me.set_slot(
                        SlotGrid::Center,
                        "clock",
                        vec![SlotText::new(format!("{}", time)).fg(fg).bg(bg).black()],
                    );
                    me.set_slot(
                        SlotGrid::Right,
                        "tray",
                        vec![
                            SlotText::new(" ").fg(fg).bg(bg),
                            SlotText::new(format!(
                                "↓{} ↑{}",
                                format_speed(usage.net_download),
                                format_speed(usage.net_upload)
                            )),
                            SlotText::new("").fg(fg).bg(bg),
                            SlotText::new(format!("{:.1}%", usage.cpu_percent)),
                            SlotText::new("󰍛").fg(fg).bg(bg),
                            SlotText::new(format!(
                                "{:.1}/{:.1} GB",
                                usage.ram_used_gb, usage.ram_total_gb
                            )),
                        ],
                    );
                }

                thread::sleep(Duration::from_secs(1));
            }
        });
    }
}
