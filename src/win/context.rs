use std::{cmp::Reverse, collections::HashMap, i32, sync::Arc, time::Duration};

use crate::{
    col, dp, h, log_debug, log_error,
    win::{
        animation::{self},
        border::{BorderInfo, BorderOverlay},
        config::{Direction, WinNtek},
        statusbar::{SlotMultiLine, SlotText, StatusBarFont},
        sys::{SystemInfo, format_speed},
        theme::th,
        util::write_to_file,
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
    pub fn is_floating_mode(&self) -> bool {
        let floating_apps = self
            .store_appdata
            .iter()
            .filter_map(
                |(hwnd, data)| {
                    if data.floating { Some(*hwnd) } else { None }
                },
            )
            .collect::<Vec<_>>();

        self.active_app.is_some_and(|f| floating_apps.contains(&f))
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
        log_debug!(
            "REMOVING APP",
            hwnd,
            "Before:",
            self.apps.len(),
            self.store_appdata.len()
        );

        self.apps.retain(|a| a.hwnd != hwnd);
        self.store_appdata.remove(&hwnd);

        // CLEANUP: Remove any dead windows that might have slipped through
        let dead_hwnds: Vec<isize> = self
            .apps
            .iter()
            .filter(|a| !WindowsAPI::is_window(h!(a.hwnd)))
            .map(|a| a.hwnd)
            .collect();

        for dead_hwnd in dead_hwnds {
            log_debug!("CLEANING UP DEAD WINDOW", dead_hwnd);
            self.apps.retain(|a| a.hwnd != dead_hwnd);
            self.store_appdata.remove(&dead_hwnd);
        }

        log_debug!("After:", self.apps.len(), self.store_appdata.len());

        self.clear_selection()?;
        self.apply_layout_in_workspace();
        Ok(())
    }
}

// =============================================================================
// EVENT HANDLERS
// =============================================================================

impl AppContext {
    // Fired while dragging the window
    pub fn on_location_change(&mut self, app: &AppData) -> anyhow::Result<()> {
        let maximize = WindowsAPI::is_window_maximized(h!(app.hwnd))?;
        if maximize {
            self.maximize_app(app.hwnd)?;
        }
        let _ = self.update_border(app.hwnd);
        Ok(())
    }

    // Fired when done resizing/repositioning the window
    pub fn on_move_size_end(&mut self, app: &AppData) {
        if let Some(monitor) = WindowsAPI::get_app_monitor(h!(app.hwnd), &self.monitors) {
            if let Some(stored_app) = self.apps.iter_mut().find(|a| a.hwnd == app.hwnd) {
                stored_app.rect = app.rect.clone();
                self.update_stored_appdata(app.hwnd, |data| {
                    data.monitor = monitor;
                });
            }
        }
        self.apply_layout_in_workspace();
        self.sync_widget_and_border("move size end");
    }

    /// Fired when app gains focus
    // pub fn on_focus_change(&mut self, hwnd: isize) -> anyhow::Result<()> {
    //     self.active_app = Some(hwnd);
    //     self.sync_widget_and_border()?;

    //     Ok(())
    // }
    pub fn sync_widget_and_border(&mut self, caller: &str) -> Result<()> {
        if let Some(hwnd) = self.active_app {
            let next_app = self
                .apps
                .iter()
                .find(|a| hwnd == a.hwnd)
                .ok_or(anyhow!("Failed to find app"))?;

            log_debug!(
                "Caller::",
                caller,
                "SYNC WIDGET AND BORDER FOR",
                &next_app.name
            );
            self.widget_update_title(hwnd)?;
            self.update_border(hwnd)?;
        }
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
    pub fn update_border(&self, hwnd: isize) -> Result<()> {
        let rect = WindowsAPI::get_rect(h!(hwnd));
        let is_maximized = WindowsAPI::is_maximized(hwnd);
        let (px, py) = WindowsAPI::get_rect_padding(hwnd);

        let y = if is_maximized {
            rect.t + (py / 2)
        } else {
            rect.t
        };
        let is_floating_app = self
            .store_appdata
            .get(&hwnd)
            .map(|f| f.floating)
            .unwrap_or(false);
        let (thickness, radius, padding, color) = if is_floating_app {
            (2.0, 5.0, 0, th().warning)
        } else {
            (2.0, 5.0, 0, th().error)
        };

        let (width, height, x, y) = (
            rect.width - px - padding,
            rect.height - py - padding,
            rect.l + (px / 2) + padding / 2,
            y + padding / 2,
        );
        log_debug!("W:", width, "H:", height, "X:", x, "Y:", y);

        let info = BorderInfo {
            x,
            y,
            width,
            height,
            color,
            thickness,
            radius,
            blacklist: self.config.blacklist.clone(),
            target: hwnd,
        };

        let is_active_app = self.active_app.map(|a| a == hwnd).unwrap_or(false);
        if is_active_app {
            let overlay_hwnd = {
                let overlay = self.border_overlay.lock();
                let overlay = overlay
                    .as_ref()
                    .ok_or_else(|| anyhow!("Cannot find border overlay"))?;
                overlay.set_focus(info);
                overlay.hwnd()
            };
            if self.is_floating_mode() {
                WindowsAPI::set_top_most(overlay_hwnd);
            } else {
                let floating = self
                    .store_appdata
                    .iter()
                    .flat_map(|(h, a)| if a.floating { Some(*h) } else { None })
                    .collect::<Vec<_>>();

                let apps = self
                    .get_workspace_apps()
                    .iter()
                    .filter(|a| !floating.contains(&a.hwnd))
                    .flat_map(|a| Some(a.hwnd))
                    .collect::<Vec<_>>();
                let top_most = {
                    let floating_hwnd = WindowsAPI::below_floating_app(&floating);
                    if floating_hwnd == 0 {
                        WindowsAPI::top_zorder_from_app(&apps)
                    } else {
                        floating_hwnd
                    }
                };
                WindowsAPI::set_top_most_after(overlay_hwnd, h!(top_most));
            }
        }

        Ok(())
    }

    fn debug_app_by_hwnd(&self, tag: &str, hwnd: isize) {
        if let Some(app) = self.apps.iter().find(|a| a.hwnd == hwnd) {
            log_debug!("=".repeat(10), tag);
            log_debug!(&app.name, dp!(app.rect));
        }
    }

    pub fn widget_update_title(&mut self, hwnd: isize) -> anyhow::Result<()> {
        let app = self.apps.iter().find(|a| a.hwnd == hwnd).ok_or(anyhow!(
            "cant find app with the provide hwnd in update_border function"
        ))?;

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
        Ok(())
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
                            SlotText::new("").fg(bg),
                            SlotText::new(format!(
                                "↓{} ↑{}",
                                format_speed(usage.net_download),
                                format_speed(usage.net_upload)
                            )),
                            SlotText::new("").fg(bg),
                            SlotText::new(format!("{:.1}%", usage.cpu_percent)),
                            SlotText::new("").fg(bg),
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
    fn get_workspace_floating_apps(&self) -> Vec<&AppData> {
        let active_workspace = self.get_active_workspace();
        let active_monitor = self.get_active_monitor();

        let workspace_hwnds: Vec<isize> = self
            .store_appdata
            .iter()
            .filter_map(|(hwnd, data)| {
                if data.workspace == active_workspace
                    && data.monitor == active_monitor
                    && data.floating
                {
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
    fn get_all_floating_app(&self) -> Vec<isize> {
        self.store_appdata
            .iter()
            .filter_map(|(hwnd, a)| if a.floating { Some(*hwnd) } else { None })
            .collect()
    }
    pub fn swap_focus(&mut self) -> Result<()> {
        let workspaces_apps = self.get_workspace_apps();
        let floating_apps = self
            .store_appdata
            .iter()
            .flat_map(|(hwnd, a)| if a.floating { Some(*hwnd) } else { None })
            .collect::<Vec<_>>();

        if !floating_apps.is_empty() {
            if let Some(active_app) = self.active_app {
                //floating app on focus
                if floating_apps.contains(&active_app) {
                    //we need to find non focus app
                    // then focus it here
                    if let Some(app) = workspaces_apps
                        .iter()
                        .filter(|a| !floating_apps.contains(&a.hwnd))
                        .collect::<Vec<_>>()
                        .get(0)
                    {
                        //we find the app
                        // focus it
                        let hwnd = app.hwnd;
                        self.active_app = Some(hwnd);
                        self.sync_widget_and_border("swap_focus::floating app");
                    }
                } else {
                    let top_floating = WindowsAPI::top_zorder_from_app(&floating_apps);
                    if let Some(app) = floating_apps.iter().find(|a| a == &&top_floating) {
                        //we get first floating app
                        //  focus it
                        self.active_app = Some(*app);
                        self.sync_widget_and_border("swap_focus::non floating app");
                    }
                }
            }
        }

        Ok(())
    }
    pub fn cycle_floating_app(&mut self, direction: &Direction) -> Result<()> {
        let floating_apps = self.get_all_floating_app();
        let apps: Vec<AppData> = self
            .get_workspace_apps()
            .into_iter()
            .filter(|a| floating_apps.contains(&a.hwnd))
            .cloned()
            .collect();
        if apps.is_empty() {
            return Ok(());
        }

        let current_active_app = {
            let app = self.get_active_app().unwrap_or_default();
            app
        };
        let current_index = apps
            .iter()
            .position(|app| app.hwnd == current_active_app)
            .unwrap_or(0);
        let new_index = (current_index + 1) % apps.len();
        let hwnd = apps[new_index].hwnd;
        WindowsAPI::focus_app(hwnd)?;
        self.active_app = Some(hwnd);
        self.sync_widget_and_border("cycle_floating_app")?;
        Ok(())
    }
    pub fn cycle_focus_app(&mut self, direction: &Direction) -> Result<()> {
        self.cycle_focus_app_impl(direction)?;
        Ok(())
    }
    fn cycle_focus_app_impl(&mut self, direction: &Direction) -> Result<()> {
        let floating_apps = self.get_all_floating_app();
        //we need to skip all floating app here
        let apps: Vec<AppData> = self
            .get_workspace_apps()
            .into_iter()
            .filter(|a| !floating_apps.contains(&a.hwnd))
            .cloned()
            .collect();

        if apps.is_empty() {
            return Ok(());
        }
        let current_active_app = {
            let app = self.get_active_app().unwrap_or_default();
            app
        };

        let current_index = apps
            .iter()
            .position(|app| app.hwnd == current_active_app)
            .unwrap_or(0);

        let new_index = match (direction, self.is_rtl()) {
            (Direction::Prev, true) => current_index.saturating_add(1),
            (Direction::Next, true) => current_index.saturating_sub(1),
            (Direction::Prev, false) => current_index.saturating_sub(1),
            (Direction::Next, false) => current_index.saturating_add(1),
        }
        .clamp(0, apps.len() - 1);

        //this is new app go activate it
        let hwnd = apps[new_index].hwnd;
        let previous_hwnd = apps[current_index].hwnd;

        // we need to set active app before this
        WindowsAPI::focus_app(hwnd)?;
        // self.active_app = Some(hwnd);

        let prev_counter = 2;
        let is_active_full = self
            .store_appdata
            .get(&hwnd)
            .is_some_and(|f| f.ratio == 1.0);
        let is_previous_full = self
            .store_appdata
            .get(&previous_hwnd)
            .is_some_and(|f| f.ratio == 1.0);

        match (current_index, new_index) {
            // Wrapping backward: at 0, press prev -> bring last index to 0
            (0, 0) => {
                let hwnd = apps[apps.len() - 1].hwnd;
                self.move_app_to_first(hwnd);
                WindowsAPI::focus_app(hwnd)?;
                // self.active_app = Some(hwnd);
            }

            // Crossing threshold or moving from fullscreen index 0
            (curr, new)
                if (new == prev_counter && curr < prev_counter)
                    || (curr == 0 && new != 0 && is_active_full)
                    || (curr == 0 && new != 0 && is_previous_full)
                    || (curr == 0 && new == prev_counter) =>
            {
                let target_hwnd = hwnd;
                for idx in 0..new {
                    self.move_app_to_last(apps[idx].hwnd);
                }
                // Update active_app and focus after moves are complete
                WindowsAPI::focus_app(target_hwnd)?;
                // self.active_app = Some(target_hwnd);
            }

            // Normal focus change - do nothing
            _ => {}
        }

        self.apply_layout_in_workspace();

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
    pub fn fill_screen(&mut self) -> Result<()> {
        let active_monitor = self.get_active_monitor();
        let monitor = &self.monitors[active_monitor];
        let active_app = self.get_active_app()?;

        Ok(())
    }
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

            // limit size factor for second app
            let new_ratio = factor[new_pos];

            let (px, _) = WindowsAPI::get_rect_padding(hwnd);
            let width = (monitor.width as f32 * new_ratio) as i32 + px;
            // Update stored ratio
            self.update_stored_appdata(hwnd, |data| {
                data.ratio = new_ratio;
            });

            // Update app rect
            let app = &mut self.apps[app_index];
            app.rect = AppRect::set_width(&app.rect, width);
            self.apply_layout_in_workspace();
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
        log_debug!("WIDTH", target.width);
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
        log_debug!("HEIGHT", target.width);
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
    pub fn switch_monitor(&mut self) -> Result<()> {
        let active_monitor = self.get_active_monitor();
        let target_monitor = if active_monitor == 0 { 1 } else { 0 };
        let monitor = &self.monitors[target_monitor];
        let (x, y) = (monitor.left + (monitor.width / 2), monitor.height / 2);
        let app_hwnd = self
            .get_workspace_apps()
            .get(0)
            .ok_or(anyhow!("Cant find app in workspace"))?
            .hwnd;
        WindowsAPI::set_cursor_pos(x, y)?;
        self.active_app = Some(app_hwnd);
        self.sync_widget_and_border("switch_monitor")?;
        Ok(())
    }
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
        for (_, app) in apps.iter().enumerate() {
            if self
                .store_appdata
                .get(&app.hwnd)
                .map(|f| f.floating)
                .unwrap_or(false)
            {
                continue;
            }
            log_debug!("RELAYOUT", &app.name, dp!(app.rect));
            WindowsAPI::to_bottom_order(app.hwnd);

            let (px, py) = WindowsAPI::get_rect_padding(app.hwnd);
            let (w, visible_w) = if apps.len() == 1 {
                (monitor.width, monitor.width - px)
            } else {
                (app.rect.width, app.rect.width - px)
            };
            let h = monitor.height + (py / 2) - toolbar_height;

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

        log_debug!("DONE RELAYOUT");
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
                WindowsAPI::show_window(app.hwnd);
                WindowsAPI::transform(
                    app.hwnd,
                    &app.rect,
                    &AppRect::xy(&app.rect, left, toolbar_height),
                )?;
            }
        }

        Ok(())
    }
    fn _store_backup(&self, log_name: &str) {
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
