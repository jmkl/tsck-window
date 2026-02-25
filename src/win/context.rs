use std::{collections::HashMap, sync::Arc, time::Duration};

use crate::{
    col, dp, log_debug, log_error, log_warn,
    win::{
<<<<<<< HEAD
        animation::{self, AnimationEasing, CubicBezier},
=======
        animation::{self},
>>>>>>> cleanup
        border::{BorderInfo, BorderOverlay},
        config::{Direction, WinNtek},
        statusbar::SlotText,
        sys::{SystemInfo, format_speed},
        theme::th,
        widget::{SlotGrid, WidgetSlots, WsIndicatorPos},
        winapi::{AppData, AppRect, MonitorInfo, STATUSBAR_HEIGHT, WinApp, WindowsAPI},
    },
};
<<<<<<< HEAD
use anyhow::{Context, Result, anyhow, bail};
use parking_lot::Mutex;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, TranslateMessage,
};
=======
use anyhow::{Result, anyhow, bail};
use parking_lot::Mutex;
>>>>>>> cleanup

pub type Shared<T> = Arc<Mutex<T>>;

#[derive(Debug, Clone)]
pub struct StoredAppData {
    pub monitor: usize,
    pub ratio: f32,
    pub workspace: usize,
    pub floating: bool,
    pub top_most: bool,
}

pub struct AppContext {
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

#[macro_export]
macro_rules! h {
    ($hwnd:expr) => {
        windows::Win32::Foundation::HWND($hwnd as *mut std::ffi::c_void)
    };
}

// =============================================================================
// INITIALIZATION
// =============================================================================

impl AppContext {
    pub fn new(config: Arc<WinNtek>) -> Self {
        Self {
            apps: Vec::new(),
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

    pub fn add_app(&mut self, winapp: &WinApp) -> Result<()> {
        let app = winapp
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
                // Add new app
                self.store_appdata.insert(
                    app.hwnd,
                    StoredAppData {
                        monitor,
                        floating: self.config.floating.contains(&app.name),
                        ratio: 1.0,
                        workspace: 0,
                        top_most: false,
                    },
                );
                self.apps.push(app);
            }
        }

        Ok(())
    }

    pub fn remove_app(&mut self, hwnd: isize) -> anyhow::Result<()> {
        self.apps.retain(|a| a.hwnd != hwnd);
        self.store_appdata.remove(&hwnd);
        self.clear_selection()?;
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
        log_debug!("MAXIMIZED", maximize);
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
<<<<<<< HEAD
        // self.validate_workspace_entries();
=======
>>>>>>> cleanup
        self.arrange_app_on_drag_end();
    }

    /// Fired when app gains focus
    pub fn on_focus_change(&mut self, app: &AppData) -> anyhow::Result<()> {
        self.active_app = Some(app.hwnd);
        self.widget_update_title(app);
<<<<<<< HEAD
        let apps = self.get_workspace_apps();
=======
        // let apps = self.get_workspace_apps();
>>>>>>> cleanup
        self.update_border(app)?;

        Ok(())
    }
}

// =============================================================================
// BORDER & UI UPDATES
// =============================================================================

impl AppContext {
    pub fn toggle_top_most(&mut self) -> anyhow::Result<()> {
        let app = self.active_app.ok_or(anyhow!("No active app"))?;
        {
            self.update_stored_appdata(app, |hd| {
                hd.top_most = !hd.top_most;
                WindowsAPI::toggle_top_most(hd.top_most, h!(app));
            });
        }
<<<<<<< HEAD
        let app = self
=======
        let _app = self
>>>>>>> cleanup
            .apps
            .iter()
            .find(|a| a.hwnd == app)
            .ok_or(anyhow!("Cant find app"))?;

        self.update_topmost_border()?;

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
        let active = self.active_app.ok_or(anyhow!("cant find active app"))?;

        let topmost_app: Vec<isize> = self
            .store_appdata
            .iter()
            .filter_map(
                |(hwnd, data)| {
                    if data.top_most { Some(*hwnd) } else { None }
                },
            )
            .collect();

        let mut apps = self
            .apps
            .iter()
            .filter(|app| topmost_app.contains(&app.hwnd))
            .collect::<Vec<_>>();
        if let Some(pos) = apps.iter().position(|a| a.hwnd == active) {
            let active_app = apps[pos];
            apps.remove(pos);
            apps.insert(0, active_app);
        }
        let mut binfos = Vec::new();
        let overlay = self.top_most_overlay.lock();
        let overlay = overlay
            .as_ref()
            .ok_or_else(|| anyhow!("Cannot find border overlay"))?;
        let parent_hwnd = overlay.hwnd();

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

            let info = BorderInfo {
                x: rect.l + (px / 2) + PADDING / 2,
                y: y + PADDING / 2,
                width: rect.width - px - PADDING,
                height: rect.height - py - PADDING,
                color: th().warning,
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
        const PADDING: i32 = 0;
        self.update_topmost_border()?;
        let active = self.get_active_app()?;
        if let Some(s) = self.store_appdata.get(&active) {
            if s.top_most {
                let overlay = self.border_overlay.lock();
                let overlay = overlay
                    .as_ref()
                    .ok_or_else(|| anyhow!("Cannot find border overlay"))?;
                overlay.clear_focus();

                return Ok(());
            }
        }
        let is_maximized = WindowsAPI::is_maximized(app.hwnd);
        let (px, py) = WindowsAPI::get_rect_padding(app.hwnd);

        let y = if is_maximized {
            app.rect.t + (py / 2)
        } else {
            app.rect.t
        };

        let info = BorderInfo {
            x: app.rect.l + (px / 2) + PADDING / 2,
            y: y + PADDING / 2,
            width: app.rect.width - px - PADDING,
            height: app.rect.height - py - PADDING,
            color: th().error,
            thickness: 2.0,
            radius: 5.0,
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

    fn _update_border(&self, app: &AppData) -> Result<()> {
        const PADDING: i32 = 0;

        let active = self.get_active_app()?;
        let is_maximized = WindowsAPI::is_maximized(app.hwnd);
        let (px, py) = WindowsAPI::get_rect_padding(app.hwnd);

        let y = if is_maximized {
            app.rect.t + (py / 2)
        } else {
            app.rect.t
        };

        let info = BorderInfo {
            x: app.rect.l + (px / 2) + PADDING / 2,
            y: y + PADDING / 2,
            width: app.rect.width - px - PADDING,
            height: app.rect.height - py - PADDING,
            color: th().error,
            thickness: 2.0,
            radius: 5.0,
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
                        SlotText::new(app.title.as_str()).italic(),
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
                        vec![
                            SlotText::new(time).fg(fg).bg(bg).black(),
                            SlotText::new(date),
                        ],
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
        self.validate_workspace_entries();
    }

    pub fn move_app_to_workspace(&mut self, direction: &Direction) {
        if let Err(err) = self.move_app_to_workspace_impl(direction) {
            log_error!("Move App to Workspace", dp!(err));
        }
        self.validate_workspace_entries();
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
    fn arrange_app_on_drag_end(&mut self) {
        log_warn!("TODO: Unimplemented => arrange_app_on_drag_end");
        self.apply_layout_in_workspace();
    }

    fn validate_workspace_entries(&mut self) {
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
            let new_ratio = factor[new_pos];
            let (px, _) = WindowsAPI::get_rect_padding(hwnd);
            let width = (monitor.width as f32 * new_ratio) as i32 + px;

            log_error!("Cycling size factor to position", new_pos);

            // Update stored ratio
            self.update_stored_appdata(hwnd, |data| {
                data.ratio = new_ratio;
            });

            // Update app rect
            let app = &mut self.apps[app_index];
            app.rect = AppRect::width(&app.rect, width);

            self.apply_layout_in_workspace();
        }

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
            if self.config.floating.contains(&app.name) {
                return Ok(());
            }
            let (px, py) = WindowsAPI::get_rect_padding(app.hwnd);
            let w = monitor.width + px;
            let h = monitor.height + (py / 2) - toolbar_height;

            let target_rect = AppRect::xywh(monitor.left - px / 2, toolbar_height, w, h);
            log_debug!(w, h, dp!(target_rect));
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

<<<<<<< HEAD
        // let props = self.get_props()?;
        // let width =
        //     (self.size_factor[self.width_selector_index] * props.monitor.width as f32) as i32;
        // let height =
        //     (self.size_factor[self.height_selector_index] * props.monitor.height as f32) as i32;
        // let toolbar_height = self.get_statusbar_height(self.monitor_index_for(props.active_hwnd));

        // let w = width + props.px;
        // let h = height + (props.py / 2) - toolbar_height;
        // let x = props.monitor.x + (-(props.px / 2));
        // let y = toolbar_height;
        // win_api::set_app_size_position(hwnd!(props.active_hwnd), x, y, w, h, true);
=======
>>>>>>> cleanup
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

        for app in apps {
            if self.config.floating.contains(&app.name) {
                continue;
            }

            let (px, py) = WindowsAPI::get_rect_padding(app.hwnd);
            let w = app.rect.width;
            let h = monitor.height + (py / 2) - toolbar_height;
            let visible_w = app.rect.width - px; // Width without padding

            let target_rect = if self.is_rtl() {
                // Right-to-left: subtract width, then position
                cursor_x -= visible_w;
                AppRect::xywh(cursor_x - px / 2, toolbar_height, w, h)
            } else {
                // Left-to-right: position first, then add width
                let rect = AppRect::xywh(cursor_x - px / 2, toolbar_height, w, h);
                cursor_x += visible_w;
                rect
            };

            let _ = WindowsAPI::transform(app.hwnd, &app.rect, &target_rect);

            if let Some(stored_app) = self.apps.iter_mut().find(|a| a.hwnd == app.hwnd) {
                stored_app.rect = target_rect;
            }
        }
    }

<<<<<<< HEAD
    fn transform_app(
=======
    fn _transform_app(
>>>>>>> cleanup
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
            &AppRect::xywh(x - width, toolbar_height, w, h),
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

    pub fn debug_move(&self) -> Result<()> {
        let apps = self.get_monitor_apps(0);

        if let Some(app) = apps
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("Photoshop"))
        {
            WindowsAPI::transform(
                app.hwnd,
                &app.rect,
                &AppRect::xywh(app.rect.l, app.rect.t, app.rect.width, -app.rect.height),
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
