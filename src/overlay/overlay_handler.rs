use parking_lot::Mutex;
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::WindowsAndMessaging::PostMessageW,
};

use crate::{
    hwnd,
    overlay::{
        app_info::{AppInfo, AppPosition, AppSize, Column, SizeRatio},
        color,
        config::CycleDirection,
        manager::{STATUSBAR_HEIGHT, Shared, WM_UPDATE_STATUSBAR},
        monitor_info::StatusbarMonitorInfo,
        statusbar::{SlotText, StatusBar, StatusBarFont, Visibility},
        sys::{SystemInfo, format_speed},
        user_api,
        widget::{SlotGrid, WidgetSlots, WorkspaceIndicatorPosition},
        win_api,
        win_event::WinEvent,
        workspaces::{Hwnd, HwndItem, Workspace},
    },
};
use std::{collections::HashMap, sync::Arc, thread, time::Duration};

#[derive(Debug)]
struct AppProps<'a> {
    monitor: &'a StatusbarMonitorInfo,
    active_hwnd: isize,
    position: &'a AppPosition,
    size: &'a AppSize,
    px: i32,
    py: i32,
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
                self.user_widgets.lock().set_slot(
                    SlotGrid::Left,
                    "active-app",
                    vec![
                        SlotText::new(" "),
                        SlotText::new(app.exe.clone())
                            .bg(color::WARNING)
                            .fg(color::BG),
                        SlotText::new(app.title.clone()).italic(),
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
            WinEvent::ObjectLocationchange => {}
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
        println!("{active_hwnd:?} {monitor_index:?} {monitor:?}");
        let app = self.apps.get(&active_hwnd)?;
        let (px, py) = win_api::get_rect_padding(active_hwnd);

        Some(AppProps {
            monitor: monitor,
            active_hwnd,
            position: &app.position,
            size: &app.size,
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

    fn reorder_app_pos_in_workspace(&self) {
        let guard = self.user_widgets.lock();
        for (index, workspace) in guard.workspaces.iter().enumerate() {
            for item in workspace.hwnds.iter() {
                let is_active = guard.get_active_workspace_for_monitor(item.monitor) == index;
                if let Some(appinfo) = self.apps.get(&item.hwnd) {
                    eprintln!("{}{}", is_active, appinfo.exe);
                }
            }
        }
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
    pub fn set_app_position(&self, x: i32, y: i32) {
        todo!()
    }
    pub fn set_app_size(&self, width: i32, height: i32) {
        todo!()
    }
    pub fn cycle_app_on_grid(&self, grid: Vec<(i32, i32, i32, i32)>) {
        todo!()
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
    pub fn get_workspace_at_cursor(&self) -> usize {
        win_api::get_monitor_index_from_cursor(&self.monitors)
    }
    pub fn activate_workspace(&self, workspace: &str) {
        todo!()
    }

    pub fn cycle_workspace(&self, direction: CycleDirection) {
        let monitor = self.get_active_monitor();
        let cursor_ws = self.get_workspace_at_cursor();
        let statusbar_height = self.get_statusbar_height(monitor);
        let mut userwidget = self.user_widgets.lock();

        let workspace_count = userwidget.workspaces.len();

        // Update active workspace index
        if let Some(active_workspace) = userwidget.active_workspace_per_monitor.get_mut(cursor_ws) {
            *active_workspace = match direction {
                CycleDirection::Prev => (*active_workspace + workspace_count - 1) % workspace_count,
                CycleDirection::Next => (*active_workspace + 1) % workspace_count,
            };
        }

        let active_workspace = userwidget
            .active_workspace_per_monitor
            .get(cursor_ws)
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
                            win_api::set_app_position(
                                hwnd!(appinfo.hwnd),
                                appinfo.position.x,
                                parked_pos.max(statusbar_height),
                            );
                        } else {
                            hitem.parked_position = Some(appinfo.position.y.max(statusbar_height));
                        }
                    } else if hitem.parked_position.is_some() {
                        win_api::set_app_position(hwnd!(appinfo.hwnd), appinfo.position.x, -2000);
                    }
                }
            }
        }

        userwidget.refresh_statusbar();
    }
    pub fn create_workspace(&self, title: &str, monitor_index: usize) {
        todo!()
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
