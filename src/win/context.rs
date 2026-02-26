use std::{
    cmp::{self, Reverse},
    collections::HashMap,
    i32,
    sync::Arc,
    time::Duration,
};

use crate::{
    col, d, dp, h, log_debug, log_error, log_warn,
    win::{
        animation::{self},
        border::{BorderInfo, BorderOverlay},
        config::{Direction, WinNtek},
        statusbar::{SlotMultiLine, SlotText, StatusBarFont},
        sys::{SystemInfo, format_speed},
        theme::th,
        util::{self, write_to_file},
        widget::{SlotGrid, WidgetSlots, WsIndicatorPos},
        winapi::{AppData, AppRect, MonitorInfo, STATUSBAR_HEIGHT, WinApp, WindowsAPI},
    },
};
use anyhow::{Result, anyhow, bail};
use ntek::Serialize;
use parking_lot::Mutex;
use tsck_kee::KeeModifier;

pub type Shared<T> = Arc<Mutex<T>>;

#[derive(Debug, Clone)]
pub struct StoredAppData {
    pub monitor: usize,
    pub ratio: f32,
    pub workspace: usize,
    pub floating: bool,
    pub manual_width: Option<i32>,
}

#[derive(Default)]
struct ModState {
    alt: bool,
    shift: bool,
    meta: bool,
    ctrl: bool,
}

pub struct AppContext {
    pub initialized: bool,
    mod_state: ModState,
    pub hotkeys: Vec<String>,
    monitors: Vec<MonitorInfo>,
    pub apps: Vec<AppData>,
    pub store_appdata: HashMap<isize, StoredAppData>,
    pub active_app: Option<isize>,

    // UI Components
    pub statusbar: Shared<Vec<isize>>,
    pub user_widgets: Shared<WidgetSlots>,
    pub border_overlay: Shared<Option<BorderOverlay>>,
    pub top_most_overlay: Shared<Option<BorderOverlay>>,
    config: Arc<WinNtek>,
}

// =============================================================================
// INITIALIZATION
// =============================================================================

impl AppContext {
    pub fn new(config: Arc<WinNtek>) -> Self {
        let hotkeys: Vec<String> = config
            .hotkeys
            .iter()
            .map(|(k, f)| format!("{:<15} {}", k, f.serialize()))
            .collect();
        Self {
            initialized: false,
            apps: Vec::new(),
            mod_state: ModState::default(),
            hotkeys,
            active_app: None,
            store_appdata: HashMap::new(),
            monitors: WindowsAPI::get_all_monitors(),

            top_most_overlay: Arc::new(Mutex::new(None)),
            border_overlay: Arc::new(Mutex::new(None)),
            statusbar: Arc::new(Mutex::new(vec![])),
            user_widgets: Arc::new(Mutex::new(WidgetSlots {
                workspace_indicator: WsIndicatorPos::Left,
                hwnd: None,
                ..Default::default()
            })),

            config,
        }
    }
    pub fn initialized(&mut self) {
        self.initialized = true;
        let rtl = self.is_rtl();

        if rtl {
            self.apps.sort_by_key(|app| Reverse(app.rect.l));
        } else {
            self.apps.sort_by_key(|app| app.rect.l);
        }
        self.apply_layout_in_workspace();
    }
}

// =============================================================================
// GETTERS & UTILITIES
// =============================================================================

impl AppContext {
    fn get_statusbar_height(&self, monitor: usize) -> i32 {
        if monitor == 0 {
            STATUSBAR_HEIGHT as i32
        } else {
            0
        }
    }

    fn get_active_app(&self) -> Result<isize> {
        self.active_app.ok_or_else(|| anyhow!("Active app not set"))
    }

    fn get_active_workspace(&self) -> usize {
        let active_monitor = WindowsAPI::get_monitor_in_cursor(&self.monitors);
        self.user_widgets
            .lock()
            .active_workspace_per_monitor
            .get(active_monitor)
            .copied()
            .unwrap_or(0)
    }

    fn get_active_monitor(&self) -> usize {
        WindowsAPI::get_monitor_in_cursor(&self.monitors)
    }

    pub fn is_blacklist(&self, app_name: &String) -> bool {
        self.config.blacklist.contains(app_name)
    }
}

// =============================================================================
// APP DATA MANAGEMENT
// =============================================================================

impl AppContext {
    fn update_stored_appdata<F>(&mut self, hwnd: isize, updater: F)
    where
        F: FnOnce(&mut StoredAppData),
    {
        if let Some(data) = self.store_appdata.get_mut(&hwnd) {
            updater(data);
        }
    }

    pub fn add_app(&mut self, init: bool, winapp: &WinApp) -> Result<()> {
        let mut app = winapp
            .get_app_info()
            .ok_or_else(|| anyhow!("Failed to get app info"))?;

        let monitor = WindowsAPI::get_app_monitor(h!(app.hwnd), &self.monitors)
            .ok_or_else(|| anyhow!("Cannot find monitor"))?;

        match self.apps.iter().position(|a| a.hwnd == app.hwnd) {
            Some(index) => {
                let hwnd = app.hwnd;
                // Update existing app
                self.apps[index] = app;
                self.update_stored_appdata(hwnd, |data| {
                    data.monitor = monitor;
                });
            }
            None => {
                if init {}
                app.rect.width = self.monitors[monitor].width / 2;
                self.store_appdata.insert(
                    app.hwnd,
                    StoredAppData {
                        monitor,
                        floating: false,
                        ratio: 0.5,
                        workspace: 0,
                        manual_width: None,
                    },
                );
                self.apps.insert(0, app);
            }
        }
        if self.initialized {
            self.apply_layout_in_workspace();
        }
        Ok(())
    }

    pub fn remove_app(&mut self, hwnd: isize) -> anyhow::Result<()> {
        self.apps.retain(|a| a.hwnd != hwnd);
        self.store_appdata.remove(&hwnd);
        self.clear_selection()?;
        self.apply_layout_in_workspace();
        Ok(())
    }
}

// =============================================================================
// EVENT HANDLERS
// =============================================================================

impl AppContext {
    /// Fired while dragging the window
    pub fn on_location_change(&mut self, app: &AppData) -> anyhow::Result<()> {
        let _ = self.update_border(app);
        let maximize = WindowsAPI::is_window_maximized(h!(app.hwnd))?;
        if maximize {
            self.maximize_app(app.hwnd)?;
        }

        Ok(())
    }

    /// Fired when done resizing/repositioning the window
    pub fn on_move_size_end(&mut self, app: &AppData) {
        if let Some(monitor) = WindowsAPI::get_app_monitor(h!(app.hwnd), &self.monitors) {
            if let Some(stored_app) = self.apps.iter_mut().find(|a| a.hwnd == app.hwnd) {
                stored_app.rect = app.rect.clone();
                self.update_stored_appdata(app.hwnd, |data| {
                    data.monitor = monitor;
                });
            }
        }
        // self.apply_layout_in_workspace(false);
        self.arrange_app_after_resize_or_move(app.hwnd);
    }

    /// Fired when app gains focus
    pub fn on_focus_change(&mut self, app: &AppData) -> anyhow::Result<()> {
        self.active_app = Some(app.hwnd);
        self.widget_update_title(app);
        self.update_border(app)?;

        Ok(())
    }
}

// =============================================================================
// BORDER & UI UPDATES
// =============================================================================

impl AppContext {
    pub fn toggle_floating(&mut self) -> anyhow::Result<()> {
        let app = self.active_app.ok_or(anyhow!("No active app"))?;
        {
            let monitor_index = {
                let monitor = WindowsAPI::get_app_monitor(h!(app), &self.monitors);
                monitor
            };
            let mut app_rect: Option<AppRect> = None;
            self.update_stored_appdata(app, |hd| {
                hd.floating = !hd.floating;
                WindowsAPI::toggle_top_most(hd.floating, h!(app));
                if hd.floating {
                    if let Some(rect) = WindowsAPI::center_scale(h!(app), monitor_index) {
                        app_rect = Some(rect);
                    }
                }
            });
            if let Some(app_rect) = app_rect {
                self.update_app_rect(app, |app| app.rect = app_rect);
            }
        }
        let _app = self
            .apps
            .iter()
            .find(|a| a.hwnd == app)
            .ok_or(anyhow!("Cant find app"))?;

        // self.update_topmost_border()?;
        self.apply_layout_in_workspace();

        Ok(())
    }
    pub fn clear_selection(&mut self) -> Result<()> {
        let apps = self.get_workspace_apps();
        if apps.is_empty() {
            self.active_app = None;
            let overlay = self.border_overlay.lock();
            let overlay = overlay
                .as_ref()
                .ok_or_else(|| anyhow!("Cannot find border overlay"))?;
            overlay.clear_focus();
        }

        Ok(())
    }

    pub fn update_topmost_border(&self) -> Result<()> {
        const PADDING: i32 = 0;
        let overlay = self.top_most_overlay.lock();
        let overlay = overlay
            .as_ref()
            .ok_or_else(|| anyhow!("Cannot find border overlay"))?;

        let active = self.active_app.ok_or(anyhow!("cant find active app"))?;
        let topmost_app: Vec<isize> = self
            .store_appdata
            .iter()
            .filter_map(
                |(hwnd, data)| {
                    if data.floating { Some(*hwnd) } else { None }
                },
            )
            .collect();

        let mut apps = self
            .apps
            .iter()
            .filter(|app| topmost_app.contains(&app.hwnd))
            .collect::<Vec<_>>();

        let mut binfos = Vec::new();
        let parent_hwnd = overlay.hwnd();

        if let Some(pos) = apps.iter().position(|a| a.hwnd == active) {
            let active_app = apps[pos];
            apps.remove(pos);
            apps.insert(0, active_app);
        }
        // apps.sort_by_key(|app| WindowsAPI::get_window_z_order(app.hwnd).unwrap());

        WindowsAPI::set_top_most(parent_hwnd);
        for app in apps {
            let is_maximized = WindowsAPI::is_maximized(app.hwnd);
            let (px, py) = WindowsAPI::get_rect_padding(app.hwnd);
            let rect = WindowsAPI::get_rect(h!(app.hwnd));
            let y = if is_maximized {
                rect.t + (py / 2)
            } else {
                rect.t
            };
            let color = {
                if app.hwnd == active {
                    th().warning
                } else {
                    th().accent
                }
            };
            let info = BorderInfo {
                x: rect.l + (px / 2) + PADDING / 2,
                y: y + PADDING / 2,
                width: rect.width - px - PADDING,
                height: rect.height - py - PADDING,
                color: color,
                thickness: 2.0,
                radius: 5.0,
                blacklist: self.config.blacklist.clone(),
                target: app.hwnd,
            };
            binfos.push(info);
        }

        overlay.set_top_most(binfos);
        Ok(())
    }
    pub fn update_border(&self, app: &AppData) -> Result<()> {
        // self.update_topmost_border()?;
        let active = self.get_active_app()?;
        // if let Some(s) = self.store_appdata.get(&active) {
        //     if s.floating {
        //         let overlay = self.border_overlay.lock();
        //         let overlay = overlay
        //             .as_ref()
        //             .ok_or_else(|| anyhow!("Cannot find border overlay"))?;
        //         overlay.clear_focus();

        //         return Ok(());
        //     }
        // }
        let is_maximized = WindowsAPI::is_maximized(app.hwnd);
        let (px, py) = WindowsAPI::get_rect_padding(app.hwnd);

        let y = if is_maximized {
            app.rect.t + (py / 2)
        } else {
            app.rect.t
        };
        let is_floating_app = self
            .store_appdata
            .get(&active)
            .map(|f| f.floating)
            .unwrap_or(false);
        let (thickness, radius, padding) = if is_floating_app {
            (6.0, 8.0, -4)
        } else {
            (2.0, 5.0, 0)
        };
        let info = BorderInfo {
            x: app.rect.l + (px / 2) + padding / 2,
            y: y + padding / 2,
            width: app.rect.width - px - padding,
            height: app.rect.height - py - padding,
            color: th().error,
            thickness,
            radius,
            blacklist: self.config.blacklist.clone(),
            target: app.hwnd,
        };

        let overlay = self.border_overlay.lock();
        let overlay = overlay
            .as_ref()
            .ok_or_else(|| anyhow!("Cannot find border overlay"))?;

        if app.hwnd == active {
            overlay.set_focus(info);
        }

        Ok(())
    }

    pub fn widget_update_title(&mut self, app: &AppData) {
        if let Some(active_app) = self.active_app {
            let title = crate::win::util::truncate(&app.title, 25);
            if active_app == app.hwnd {
                self.user_widgets.lock().set_slot(
                    SlotGrid::Left,
                    "active-app",
                    vec![
                        SlotText::new(" "),
                        SlotText::new(app.name.as_str())
                            .bg(col!(warning))
                            .fg(col!(warning_content))
                            .bold(),
                        SlotText::new(title.as_str()).italic(),
                    ],
                );
            }
        }
    }
}

// =============================================================================
// WIDGET SPAWNING
// =============================================================================

impl AppContext {
    pub fn on_modifier_pressed(&mut self, modifier: &KeeModifier, state: &bool) {
        match modifier {
            KeeModifier::Ctrl => self.mod_state.ctrl = *state,
            KeeModifier::Shift => self.mod_state.shift = *state,
            KeeModifier::Alt => self.mod_state.alt = *state,
            KeeModifier::Win => self.mod_state.meta = *state,
        }
        let show = self.mod_state.ctrl && self.mod_state.shift;
        self.render_which_key(&show);
    }
    fn render_which_key(&mut self, show: &bool) {
        let mut widget = self.user_widgets.lock();
        let slots = if *show {
            vec![SlotMultiLine {
                lines: self.hotkeys.clone(),
                padding: 20.0,
                line_height: 1.5,
                x: 0.0,
                y: 0.0,
                font: StatusBarFont::default(),
                fg: col!(warning),
                bg: col!(base_300),
            }]
        } else {
            vec![]
        };

        widget.set_multiline("test", slots);
    }
    pub fn spawn_widget(&self) {
        let user_widget = self.user_widgets.clone();

        // Wait for statusbar to be ready
        let hwnd = loop {
            if let Some(&hwnd) = self.statusbar.lock().get(0) {
                break hwnd;
            }
            std::thread::sleep(Duration::from_millis(10));
        };

        user_widget.lock().set_hwnd(Some(hwnd));

        // Spawn background thread for system monitoring
        std::thread::spawn(move || {
            let mut info = SystemInfo::new();
            loop {
                let local = chrono::Local::now();
                let time = local.format("%H:%M %p").to_string();
                let date = local.format("%a, %d %h %Y").to_string();
                let usage = info.update();

                let bg = col!(error);
                let fg = col!(error_content);
                {
                    let mut widget = user_widget.lock();
                    // Update clock
                    widget.set_slot(
                        SlotGrid::Center,
                        "clock",
                        vec![SlotText::new(time).black(), SlotText::new(date)],
                    );

                    // Update system stats
                    widget.set_slot(
                        SlotGrid::Right,
                        "tray",
                        vec![
                            SlotText::new(" ").fg(fg).bg(bg),
                            SlotText::new(format!(
                                "↓{} ↑{}",
                                format_speed(usage.net_download),
                                format_speed(usage.net_upload)
                            )),
                            SlotText::new("").fg(fg).bg(bg),
                            SlotText::new(format!("{:.1}%", usage.cpu_percent)),
                            SlotText::new("").fg(fg).bg(bg),
                            SlotText::new(format!(
                                "{:.1}/{:.1} GB",
                                usage.ram_used_gb, usage.ram_total_gb
                            )),
                        ],
                    );
                }

                std::thread::sleep(Duration::from_secs(1));
            }
        });
    }
}

// =============================================================================
// WORKSPACE MANAGEMENT
// =============================================================================

impl AppContext {
    fn shift_workspace(&mut self, direction: &Direction) -> Result<()> {
        let mut guard = self.user_widgets.lock();
        let workspace_count = guard.workspaces.len();
        let active_monitor = self.get_active_monitor();

        if let Some(active) = guard.active_workspace_per_monitor.get_mut(active_monitor) {
            *active = match direction {
                Direction::Prev => (*active + workspace_count - 1) % workspace_count,
                Direction::Next => (*active + 1) % workspace_count,
            };
        }

        guard.refresh_statusbar();
        Ok(())
    }

    pub fn cycle_workspace(&mut self, direction: &Direction) {
        if let Err(err) = self.shift_workspace(direction) {
            log_error!("Cycle Workspace", dp!(err));
        }
        self.toggle_visibility_on_workspace();
    }

    pub fn move_app_to_workspace(&mut self, direction: &Direction) {
        if let Err(err) = self.move_app_to_workspace_impl(direction) {
            log_error!("Move App to Workspace", dp!(err));
        }
        self.toggle_visibility_on_workspace();
    }

    fn move_app_to_workspace_impl(&mut self, direction: &Direction) -> Result<()> {
        self.shift_workspace(direction)?;

        let active_app = self.get_active_app()?;
        let active_workspace = self.get_active_workspace();

        let is_workspace_app = self.apps.iter().any(|app| app.hwnd == active_app);

        if is_workspace_app {
            self.update_stored_appdata(active_app, |data| {
                data.workspace = active_workspace;
            });
        }

        Ok(())
    }
    fn arrange_app_after_resize_or_move(&mut self, hwnd: isize) {
        let apps = self
            .get_workspace_apps()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();

        let active_monitor = self.get_active_monitor();
        let monitor = match self.monitors.get(active_monitor) {
            Some(m) => m,
            None => return,
        };

        // Find which index this window is
        let resized_index = apps.iter().position(|a| a.hwnd == hwnd);

        if let Some(app) = self.apps.iter().find(|a| a.hwnd == hwnd) {
            let (px, _) = WindowsAPI::get_rect_padding(hwnd);
            let current_width = app.rect.width;

            // Store the manual width
            if let Some(app_data) = self.store_appdata.get_mut(&hwnd) {
                app_data.manual_width = Some(current_width);
            }

            // If user resized index 1, adjust index 0 to fill remaining space
            if resized_index == Some(1) {
                if let Some(index_0_app) = apps.get(0) {
                    let (px1, _) = WindowsAPI::get_rect_padding(hwnd);
                    let index_1_visible_width = current_width - px1;

                    let remaining_width = monitor.width - index_1_visible_width;
                    let (px0, _) = WindowsAPI::get_rect_padding(index_0_app.hwnd);
                    let index_0_full_width = remaining_width + px0;

                    // Store manual width for index 0
                    if let Some(app_data) = self.store_appdata.get_mut(&index_0_app.hwnd) {
                        app_data.manual_width = Some(index_0_full_width);
                    }
                }
            }

            // If user resized index 0, adjust index 1 to fill remaining space
            if resized_index == Some(0) {
                if let Some(index_1_app) = apps.get(1) {
                    let (px0, _) = WindowsAPI::get_rect_padding(hwnd);
                    let index_0_visible_width = current_width - px0;

                    let remaining_width = monitor.width - index_0_visible_width;
                    let (px1, _) = WindowsAPI::get_rect_padding(index_1_app.hwnd);
                    let index_1_full_width = remaining_width + px1;

                    // Store manual width for index 1
                    if let Some(app_data) = self.store_appdata.get_mut(&index_1_app.hwnd) {
                        app_data.manual_width = Some(index_1_full_width);
                    }
                }
            }
        }

        // Reapply layout respecting the new manual widths
        self.apply_layout_in_workspace();
    }

    fn toggle_visibility_on_workspace(&mut self) {
        let active_workspace = self.get_active_workspace();
        let active_monitor = self.get_active_monitor();

        for app in &self.apps {
            if let Some(data) = self.store_appdata.get(&app.hwnd) {
                if data.monitor == active_monitor {
                    if data.workspace == active_workspace {
                        WindowsAPI::show_window(app.hwnd);
                    } else {
                        WindowsAPI::hide_window(app.hwnd);
                    }
                }
            }
        }
        if let Err(err) = self.clear_selection() {
            log_error!("Error clearing selection", err);
        }
    }
}

// =============================================================================
// APP NAVIGATION & FOCUS
// =============================================================================

impl AppContext {
    fn get_workspace_apps(&self) -> Vec<&AppData> {
        let active_workspace = self.get_active_workspace();
        let active_monitor = self.get_active_monitor();

        let workspace_hwnds: Vec<isize> = self
            .store_appdata
            .iter()
            .filter_map(|(hwnd, data)| {
                if data.workspace == active_workspace && data.monitor == active_monitor {
                    Some(*hwnd)
                } else {
                    None
                }
            })
            .collect();

        self.apps
            .iter()
            .filter(|app| workspace_hwnds.contains(&app.hwnd))
            .collect()
    }

    pub fn cycle_app(&mut self, direction: &Direction) {
        if !self.initialized {
            self.initialized = true;
        }
        if let Err(err) = self.cycle_app_impl(direction) {
            log_error!("Cycle App", err);
        }
    }

    fn cycle_app_impl(&mut self, direction: &Direction) -> Result<()> {
        let active_app = self.get_active_app()?;
        let filtered_apps = self.get_workspace_apps();

        if filtered_apps.is_empty() {
            return Ok(());
        }

        let current_pos = filtered_apps
            .iter()
            .position(|app| app.hwnd == active_app)
            .unwrap_or(0);

        let new_pos = match (direction, self.is_rtl()) {
            (Direction::Prev, true) => {
                if current_pos == 0 {
                    filtered_apps.len() - 1
                } else {
                    current_pos - 1
                }
            }
            (Direction::Next, true) => (current_pos + 1) % filtered_apps.len(),

            (Direction::Prev, false) => (current_pos + 1) % filtered_apps.len(),
            (Direction::Next, false) => {
                if current_pos == 0 {
                    filtered_apps.len() - 1
                } else {
                    current_pos - 1
                }
            }
        };

        self.active_app = Some(filtered_apps[new_pos].hwnd);
        Ok(())
    }

    fn move_app_to_last(&mut self, hwnd: isize) {
        if let Some(pos) = self.apps.iter().position(|a| a.hwnd == hwnd) {
            let app = self.apps.remove(pos);
            self.apps.push(app);
        }
    }
    fn move_app_to_first(&mut self, hwnd: isize) {
        if let Some(pos) = self.apps.iter().position(|a| a.hwnd == hwnd) {
            let app = self.apps.remove(pos);
            self.apps.insert(0, app);
        }
    }
    // pub fn focus_app(&mut self, direction: &Direction) -> Result<()> {
    //     let apps: Vec<AppData> = self.get_workspace_apps().into_iter().cloned().collect();
    //     if apps.is_empty() {
    //         return Ok(());
    //     }
    //     let active_app = {
    //         let app = self.get_active_app().unwrap_or_default();
    //         app
    //     };
    //     let current_index = apps
    //         .iter()
    //         .position(|app| app.hwnd == active_app)
    //         .unwrap_or(0);
    //     let new_index = match (direction, self.is_rtl()) {
    //         (Direction::Prev, true) => current_index.saturating_add(1),
    //         (Direction::Next, true) => current_index.saturating_sub(1),
    //         (Direction::Prev, false) => current_index.saturating_sub(1),
    //         (Direction::Next, false) => current_index.saturating_add(1),
    //     }
    //     .clamp(0, apps.len() - 1);
    //     let new_app = &apps[new_index];
    //     WindowsAPI::focus_app(new_app)?;
    //     self.on_focus_change(new_app)?;

    //     // Count floating apps
    //     let floating_count = self
    //         .apps
    //         .iter()
    //         .filter(|app| {
    //             self.store_appdata
    //                 .get(&app.hwnd)
    //                 .map(|h| h.floating)
    //                 .unwrap_or(false)
    //         })
    //         .count();

    //     let prev_counter = 2 + floating_count;

    //     // When cycling forward and crossing the threshold
    //     if new_index == prev_counter && current_index < prev_counter {
    //         if let Some(first_app_hwnd) = self.apps.get(0).map(|app| app.hwnd) {
    //             self.move_app_to_last(first_app_hwnd);
    //             self.reorder_floating_after_first();
    //             self.apply_layout_in_workspace();
    //         }
    //     }

    //     // When cycling backward at the start
    //     if new_index == 0 && current_index == 0 {
    //         if let Some(last_app_hwnd) = self.apps.last().map(|app| app.hwnd) {
    //             self.move_app_to_first(last_app_hwnd);
    //             self.reorder_floating_after_first();
    //             self.apply_layout_in_workspace();
    //         }
    //     }

    //     Ok(())
    // }

    // fn reorder_floating_after_first(&mut self) {
    //     if self.apps.is_empty() {
    //         return;
    //     }

    //     // Collect all floating app hwnds (excluding the first app)
    //     let floating_hwnds: Vec<isize> = self
    //         .apps
    //         .iter()
    //         .skip(1)
    //         .filter(|app| {
    //             self.store_appdata
    //                 .get(&app.hwnd)
    //                 .map(|h| h.floating)
    //                 .unwrap_or(false)
    //         })
    //         .map(|app| app.hwnd)
    //         .collect();

    //     // Move each floating app to position right after index 0
    //     for (i, hwnd) in floating_hwnds.iter().enumerate() {
    //         if let Some(pos) = self.apps.iter().position(|a| a.hwnd == *hwnd) {
    //             let app = self.apps.remove(pos);
    //             self.apps.insert(1 + i, app); // Insert after first app + previously inserted floating apps
    //         }
    //     }
    // }

    pub fn focus_app(&mut self, direction: &Direction) -> Result<()> {
        let apps: Vec<AppData> = self.get_workspace_apps().into_iter().cloned().collect();

        if apps.is_empty() {
            return Ok(());
        }
        let active_app = {
            let app = self.get_active_app().unwrap_or_default();
            app
        };

        let current_index = apps
            .iter()
            .position(|app| app.hwnd == active_app)
            .unwrap_or(0);

        let new_index = match (direction, self.is_rtl()) {
            (Direction::Prev, true) => current_index.saturating_add(1),
            (Direction::Next, true) => current_index.saturating_sub(1),
            (Direction::Prev, false) => current_index.saturating_sub(1),
            (Direction::Next, false) => current_index.saturating_add(1),
        }
        .clamp(0, apps.len() - 1);
        let new_app = &apps[new_index];
        WindowsAPI::focus_app(new_app)?;
        self.on_focus_change(new_app)?;

        let floating_apps = self
            .store_appdata
            .iter()
            .flat_map(|(a, h)| if h.floating { Some(a) } else { None })
            .collect::<Vec<_>>();

        let prev_counter = 2;
        let is_active_full = self
            .store_appdata
            .get(&active_app)
            .is_some_and(|f| f.ratio == 1.0);
        log_error!("SIZE RATIO IS FULL", is_active_full);

        if new_index == prev_counter && current_index < prev_counter {
            self.move_app_to_last(apps[0].hwnd);
            self.apply_layout_in_workspace();
        }

        if new_index == 0 && current_index == 0 {
            self.move_app_to_first(apps[apps.len() - 1].hwnd);
            self.apply_layout_in_workspace();
        }

        Ok(())
    }
    fn find_real_index(&self, hwnd: isize) -> Option<usize> {
        self.apps.iter().position(|a| a.hwnd == hwnd)
    }
    pub fn move_app(&mut self, direction: &Direction) -> Result<()> {
        let active_app = self.get_active_app()?;
        let apps: Vec<AppData> = self.get_workspace_apps().into_iter().cloned().collect();

        if let Some(index) = apps.iter().position(|app| app.hwnd == active_app) {
            let sibling = match (direction, self.is_rtl()) {
                (Direction::Prev, true) => index.saturating_add(1).clamp(0, apps.len() - 1),
                (Direction::Next, true) => index.saturating_sub(1).clamp(0, apps.len() - 1),
                (Direction::Prev, false) => index.saturating_sub(1).clamp(0, apps.len() - 1),
                (Direction::Next, false) => index.saturating_add(1).clamp(0, apps.len() - 1),
            };
            let nextapp = &apps[sibling];
            let app = &apps[index];
            let r = self.find_real_index(app.hwnd).ok_or(anyhow!("Cant"))?;
            let l = self.find_real_index(nextapp.hwnd).ok_or(anyhow!("Cant"))?;
            self.apps.swap(r, l);
            self.apply_layout_in_workspace();
        }

        Ok(())
    }
    fn sync_app_order(&mut self, ws_apps: &mut Vec<AppData>) {
        let id: HashMap<_, _> = ws_apps.drain(..).map(|e| (e.hwnd, e)).collect();
        ws_apps.extend(self.apps.iter().filter_map(|v| id.get(&v.hwnd)).cloned());
    }
}

// =============================================================================
// SIZE MANAGEMENT
// =============================================================================

impl AppContext {
    pub fn cycle_size_factor(&mut self) -> Result<()> {
        let active_monitor = self.get_active_monitor();
        let monitor = &self.monitors[active_monitor];
        let factor = &self.config.size_factor;
        let active_app = self.get_active_app()?;

        let app_index = self
            .apps
            .iter()
            .position(|a| a.hwnd == active_app)
            .ok_or_else(|| anyhow!("App not found"))?;

        let hwnd = self.apps[app_index].hwnd;
        let current_ratio = self
            .store_appdata
            .get(&hwnd)
            .map(|data| data.ratio)
            .unwrap_or(1.0);

        if let Some(pos) = factor.iter().position(|&r| r == current_ratio) {
            let new_pos = (pos + 1) % factor.len();
            let current_app_index = self
                .get_workspace_apps()
                .iter()
                .position(|a| a.hwnd == active_app)
                .unwrap_or(0);

            // limit size factor for second app
            let mut new_ratio = factor[new_pos];
            if current_app_index == 1 && new_ratio > 0.75 {
                new_ratio = 0.75;
            }
            let (px, _) = WindowsAPI::get_rect_padding(hwnd);
            let width = (monitor.width as f32 * new_ratio) as i32 + px;
            // Update stored ratio
            self.update_stored_appdata(hwnd, |data| {
                data.ratio = new_ratio;
            });

            // Update app rect
            let app = &mut self.apps[app_index];
            let hwnd = app.hwnd;
            app.rect = AppRect::set_width(&app.rect, width);
            self.arrange_app_after_resize_or_move(hwnd);
        }

        Ok(())
    }
    fn floating_app(&self) -> anyhow::Result<&AppData> {
        if let Some(app) = self.active_app {
            if let Some(a) = self.store_appdata.get(&app) {
                if a.floating {
                    return self
                        .apps
                        .iter()
                        .find(|a| a.hwnd == app)
                        .ok_or(anyhow!("Cant find app"));
                }
            }
        }
        bail!("not a floating app")
    }

    fn update_app_rect<F>(&mut self, hwnd: isize, process: F)
    where
        F: FnOnce(&mut AppData),
    {
        if let Some(app) = self.apps.iter_mut().find(|a| a.hwnd == hwnd) {
            process(app)
        }
    }
    pub fn resize_width(&mut self, val: i32) -> anyhow::Result<()> {
        let app = self.floating_app()?;
        let target = AppRect::add_to_width(&app.rect, val);
        let hwnd = app.hwnd;
        self.update_app_rect(app.hwnd, |md| {
            if let Err(err) = WindowsAPI::transform_to(hwnd, &target) {
                log_error!("Error tranform to ", err);
            } else {
                md.rect = target;
            }
        });
        Ok(())
    }
    pub fn resize_height(&mut self, val: i32) -> anyhow::Result<()> {
        let app = self.floating_app()?;
        let target = AppRect::add_to_height(&app.rect, val);
        let hwnd = app.hwnd;
        self.update_app_rect(hwnd, |md| {
            if let Err(err) = WindowsAPI::transform_to(hwnd, &target) {
                log_error!("Error resize_height to ", err);
            } else {
                md.rect = target;
            }
        });
        Ok(())
    }
    pub fn transform_x(&mut self, val: i32) -> anyhow::Result<()> {
        let app = self.floating_app()?;
        let r = &app.rect;
        let target = AppRect::move_x(&r, val);
        let hwnd = app.hwnd;
        self.update_app_rect(hwnd, |md| {
            if let Err(err) = WindowsAPI::transform_to(hwnd, &target) {
                log_error!("Error transform_x to ", err);
            } else {
                md.rect = target;
            }
        });
        Ok(())
    }
    pub fn transform_y(&mut self, val: i32) -> anyhow::Result<()> {
        let app = self.floating_app()?;
        let r = &app.rect;
        let target = AppRect::move_y(&r, val);
        let hwnd = app.hwnd;
        self.update_app_rect(hwnd, |md| {
            if let Err(err) = WindowsAPI::transform_to(hwnd, &target) {
                log_error!("Error tranform to ", err);
            } else {
                md.rect = target;
            }
        });
        Ok(())
    }
    pub fn maximize_app(&mut self, h: isize) -> anyhow::Result<()> {
        {
            let active_monitor = self.get_active_monitor();
            let toolbar_height = self.get_statusbar_height(active_monitor);
            let app = self
                .apps
                .iter_mut()
                .find(|a| a.hwnd == h)
                .ok_or(anyhow!("Cant find the app"))?;

            let monitor = match self.monitors.get(active_monitor) {
                Some(m) => m,
                None => bail!("No Monitor found!"),
            };
            if self
                .store_appdata
                .get(&app.hwnd)
                .map(|f| f.floating)
                .unwrap_or(false)
            {
                return Ok(());
            }

            let (px, py) = WindowsAPI::get_rect_padding(app.hwnd);
            let w = monitor.width + px;
            let h = monitor.height + (py / 2) - toolbar_height;

            let target_rect = AppRect::new(monitor.left - px / 2, toolbar_height, w, h);
            let _ = WindowsAPI::transform_to(app.hwnd, &target_rect);

            app.rect = target_rect;
        }
        {
            let app = self
                .apps
                .iter()
                .find(|a| a.hwnd == h)
                .ok_or(anyhow!("Cant find the app"))?;
            self.update_stored_appdata(app.hwnd, |sd| {
                sd.ratio = 1.0;
            });
        }

        Ok(())
    }
}

// =============================================================================
// LAYOUT & ARRANGEMENT
// =============================================================================

impl AppContext {
    pub fn is_rtl(&self) -> bool {
        let active_monitor = self.get_active_monitor();
        active_monitor == 0
    }
    // pub fn apply_layout_in_workspace(&mut self, respect_manual_resize: bool) {
    //     let active_monitor = self.get_active_monitor();
    //     let toolbar_height = self.get_statusbar_height(active_monitor);
    //     let apps = self
    //         .get_workspace_apps()
    //         .into_iter()
    //         .cloned()
    //         .collect::<Vec<_>>();
    //     if apps.is_empty() {
    //         return;
    //     }
    //     let monitor = match self.monitors.get(active_monitor) {
    //         Some(m) => m,
    //         None => return,
    //     };

    //     let mut cursor_x = if self.is_rtl() {
    //         monitor.left + monitor.width
    //     } else {
    //         monitor.left
    //     };

    //     let monitor_width = monitor.width;

    //     for (index, app) in apps.iter().enumerate() {
    //         if !self.active_app.is_some_and(|active| app.hwnd == active) {
    //             WindowsAPI::to_bottom_order(app.hwnd);
    //         }

    //         if self
    //             .store_appdata
    //             .get(&app.hwnd)
    //             .map(|f| f.floating)
    //             .unwrap_or(false)
    //         {
    //             continue;
    //         }

    //         let (px, py) = WindowsAPI::get_rect_padding(app.hwnd);
    //         let h = monitor.height + (py / 2) - toolbar_height;

    //         // Check if user manually set width for this window
    //         let manual_width = if respect_manual_resize {
    //             self.store_appdata
    //                 .get(&app.hwnd)
    //                 .and_then(|f| f.manual_width)
    //         } else {
    //             None
    //         };

    //         let (w, visible_w) = if let Some(manual) = manual_width {
    //             // User manually resized - use that width
    //             (manual, manual - px)
    //         } else if index == 1 {
    //             // Auto layout for index 1
    //             let index_0_width = apps
    //                 .get(0)
    //                 .map(|a| {
    //                     let (px0, _) = WindowsAPI::get_rect_padding(a.hwnd);
    //                     if respect_manual_resize {
    //                         self.store_appdata
    //                             .get(&a.hwnd)
    //                             .and_then(|f| f.manual_width)
    //                             .map(|w| w - px0)
    //                             .unwrap_or(a.rect.width - px0)
    //                     } else {
    //                         a.rect.width - px0
    //                     }
    //                 })
    //                 .unwrap_or(0);
    //             let index_0_percentage = (index_0_width as f32 / monitor_width as f32) * 100.0;

    //             if index_0_percentage <= 75.0 {
    //                 let remaining_visible = monitor_width - index_0_width;
    //                 let full_width = remaining_visible + px;
    //                 (full_width, remaining_visible)
    //             } else {
    //                 // Index 0 is >75%, calculate remaining space (will be negative or very small)
    //                 let remaining_visible = monitor_width - index_0_width;
    //                 // Use the old width but it will be positioned off-screen
    //                 (app.rect.width, app.rect.width - px)
    //             }
    //         } else {
    //             (app.rect.width, app.rect.width - px)
    //         };

    //         let target_rect = if self.is_rtl() {
    //             cursor_x -= visible_w;

    //             // If cursor_x is less than monitor.left, we're off-screen
    //             // Don't adjust for padding in that case
    //             let x_pos = if cursor_x < monitor.left {
    //                 cursor_x // Off-screen: use raw cursor position (will be negative)
    //             } else {
    //                 cursor_x - px / 2 // On-screen: adjust for padding
    //             };

    //             AppRect::new(x_pos, toolbar_height, w, h)
    //         } else {
    //             let x_pos = if cursor_x >= monitor.left + monitor.width {
    //                 cursor_x // Off-screen to the right
    //             } else {
    //                 cursor_x - px / 2 // On-screen
    //             };

    //             let rect = AppRect::new(x_pos, toolbar_height, w, h);
    //             cursor_x += visible_w;
    //             rect
    //         };

    //         let _ = WindowsAPI::transform(app.hwnd, &app.rect, &target_rect);
    //         if let Some(stored_app) = self.apps.iter_mut().find(|a| a.hwnd == app.hwnd) {
    //             stored_app.rect = target_rect;
    //         }
    //     }
    //     self.store_backup("app_data.ntek");
    // }
    pub fn apply_layout_in_workspace(&mut self) {
        let active_monitor = self.get_active_monitor();
        let toolbar_height = self.get_statusbar_height(active_monitor);

        let apps = self
            .get_workspace_apps()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();

        if apps.is_empty() {
            return;
        }

        let monitor = match self.monitors.get(active_monitor) {
            Some(m) => m,
            None => return,
        };

        // Initialize cursor based on direction
        let mut cursor_x = if self.is_rtl() {
            monitor.left + monitor.width
        } else {
            monitor.left // Start from left
        };

        for (index, app) in apps.iter().enumerate() {
            if self
                .store_appdata
                .get(&app.hwnd)
                .map(|f| f.floating)
                .unwrap_or(false)
            {
                continue;
            }

            let (px, py) = WindowsAPI::get_rect_padding(app.hwnd);
            let w = app.rect.width;
            let h = monitor.height + (py / 2) - toolbar_height;
            let visible_w = app.rect.width - px; // Width without padding

            let target_rect = if self.is_rtl() {
                cursor_x -= visible_w;
                AppRect::new(cursor_x - px / 2, toolbar_height, w, h)
            } else {
                // Left-to-right: position first, then add width
                let rect = AppRect::new(cursor_x - px / 2, toolbar_height, w, h);
                cursor_x += visible_w;
                rect
            };

            let _ = WindowsAPI::transform(app.hwnd, &app.rect, &target_rect);

            if let Some(stored_app) = self.apps.iter_mut().find(|a| a.hwnd == app.hwnd) {
                stored_app.rect = target_rect;
            }
        }
    }

    fn _transform_app(
        &self,
        app: &AppData,
        xpos: i32,
        ratio: f32,
        rtl: bool,
        width: i32,
    ) -> Option<i32> {
        let active_monitor = self.get_active_monitor();
        let monitor = self.monitors.get(active_monitor)?;
        let (px, py) = WindowsAPI::get_rect_padding(app.hwnd);
        let toolbar_height = self.get_statusbar_height(active_monitor);

        let w = (monitor.width as f32 * ratio) as i32 + px;
        let h = monitor.height + (py / 2) - toolbar_height;

        let x = if ratio == 0.0 {
            if rtl {
                monitor.right - monitor.width
            } else {
                monitor.left
            }
        } else if rtl {
            monitor.right - xpos - w + px / 2
        } else {
            monitor.left + xpos - px / 2
        };

        animation::animate_window(
            app.hwnd,
            &app.rect,
            &AppRect::new(x - width, toolbar_height, w, h),
        );

        Some(w - px)
    }
}

// =============================================================================
// DEBUG UTILITIES
// =============================================================================

impl AppContext {
    fn get_monitor_apps(&self, monitor: usize) -> Vec<AppData> {
        let workspace_hwnds: Vec<isize> = self
            .store_appdata
            .iter()
            .filter_map(|(hwnd, data)| {
                if data.monitor == monitor {
                    Some(*hwnd)
                } else {
                    None
                }
            })
            .collect();

        self.apps
            .iter()
            .filter(|app| workspace_hwnds.contains(&app.hwnd))
            .cloned()
            .collect()
    }

    pub fn debug_reset(&mut self) -> Result<()> {
        for app in &self.apps {
            if let Some(data) = self.store_appdata.get(&app.hwnd) {
                let left = self.monitors[data.monitor].left;
                let monitor =
                    WindowsAPI::get_app_monitor(h!(app.hwnd), &self.monitors).unwrap_or(0);
                let toolbar_height = self.get_statusbar_height(monitor);

                WindowsAPI::transform(
                    app.hwnd,
                    &app.rect,
                    &AppRect::xy(&app.rect, left, toolbar_height),
                )?;
            }
        }

        Ok(())
    }
    fn store_backup(&self, log_name: &str) {
        let str = ntek::to_str_pretty(&self.apps);
        if let Err(err) = write_to_file(log_name, &str) {
            log_error!("Failed to write log ", log_name, "with Error:", err);
        }
    }
    pub fn debug_move(&self) -> Result<()> {
        let apps = self.get_monitor_apps(0);

        if let Some(app) = apps
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("Photoshop"))
        {
            WindowsAPI::transform(
                app.hwnd,
                &app.rect,
                &AppRect::new(app.rect.l, app.rect.t, app.rect.width, -app.rect.height),
            )?;
        }

        Ok(())
    }

    pub fn debug_list_app(&self) -> Result<()> {
        let active_monitor = self.get_active_monitor();

        for app in self.get_monitor_apps(active_monitor) {
            log_debug!(
                app.name,
                app.title,
                format!(
                    "{}:{} {}:{}",
                    app.rect.width, app.rect.height, app.rect.l, app.rect.t
                )
            );
        }

        Ok(())
    }
}
