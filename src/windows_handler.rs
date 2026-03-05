use crate::{
    MonitorInfo, MonitorManager,
    border_manager::{BorderInfo, BorderPainter},
    col,
    config::{Direction, WinNtek},
    d, h, log_debug, log_error,
    statusbar_manager::{SlotText, get_statusbar_height},
    utils::format_speed,
    widget_manager::{SlotGrid, WidgetSlots, Workspace, WsIndicatorPos},
    win::sys::SystemInfo,
    windows_api::{WinAPI, WinRect, WindowsAppData},
    windows_manager::Shared,
};
use anyhow::{Result, anyhow};
use ntek::Serialize;
use parking_lot::Mutex;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};
use tsck_kee::KeeModifier;
use windows::Win32::Foundation::HWND;

struct StoredData {
    floating: bool,
    name: String,
    monitor: usize,
    workspace: usize,
    ratio: f32,
}
pub enum UpdateKind {
    Location,
    Title,
    Foreground,
    MoveSize,
}

pub struct WindowsHandler {
    last_update: Instant,
    active_app: Option<isize>,
    apps: Vec<WindowsAppData>,
    config: Arc<WinNtek>,
    monitor_manager: MonitorManager,
    stored_data: HashMap<isize, StoredData>,
    pub border: Shared<Option<BorderPainter>>,
    pub statusbar: Shared<Vec<isize>>,
    pub user_widgets: Shared<WidgetSlots>,
}

impl WindowsHandler {
    pub fn new(
        apps: Vec<WindowsAppData>,
        config: Arc<WinNtek>,
        statusbar: Shared<Vec<isize>>,
    ) -> Self {
        let workspaces = config
            .workspaces
            .iter()
            .enumerate()
            .map(|(i, ws)| Workspace {
                text: ws.to_string(),
                active: i == 0,
                hwnds: vec![],
            })
            .collect::<Vec<_>>();
        Self {
            apps,
            last_update: Instant::now(),
            config,
            statusbar,
            monitor_manager: MonitorManager::new(),
            stored_data: HashMap::new(),
            active_app: None,
            border: Arc::new(Mutex::new(None)),
            user_widgets: Arc::new(Mutex::new(WidgetSlots {
                workspace_indicator: WsIndicatorPos::Left,
                hwnd: None,
                workspaces: workspaces,
                ..Default::default()
            })),
        }
    }
    pub fn init(&mut self) {
        let blacklist = self.get_blacklist();
        let mut apps = self
            .apps
            .iter()
            .filter(|a| !blacklist.contains(&a.name))
            .cloned()
            .collect::<Vec<_>>();
        apps.sort_by_key(|f| f.rect.x);
        for app in apps {
            let hwnd = app.hwnd;
            if let Ok(monitor) = self.monitor_manager.get_app_monitor(h!(hwnd)) {
                let mut ratio = app.rect.width as f32 / monitor.width as f32;
                ratio = if ratio > 0.9 - f32::EPSILON {
                    1.0
                } else {
                    ratio
                };
                self.stored_data.insert(
                    hwnd,
                    StoredData {
                        name: app.name.clone(),
                        floating: false,
                        ratio,
                        monitor: monitor.index,
                        workspace: 0,
                    },
                );
            }
        }
        if let Err(err) = self.arrange_layout() {
            log_error!("Error while arrange layout in init ", err);
        }
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
        let config = self
            .config
            .hotkeys
            .iter()
            .map(|(k, v)| format!("{}\t{}", k, v.serialize()))
            .collect::<Vec<_>>()
            .join("\n");

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
    fn widget_update_title(&mut self, app: &WindowsAppData) -> anyhow::Result<()> {
        let title = crate::win::util::truncate(&app.title, 25);
        let mut widget = self.user_widgets.lock();
        widget.set_slot(
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

        Ok(())
    }
    fn get_blacklist(&self) -> &[String] {
        &self.config.blacklist
    }

    fn floating_apps(&self) -> Vec<isize> {
        self.stored_data
            .iter()
            .filter_map(|(hwnd, app)| if app.floating { Some(*hwnd) } else { None })
            .collect()
    }
    pub fn is_floating_mode(&self) -> bool {
        false
    }
    pub fn on_modifier_pressed(&self, modifier: &KeeModifier, state: &bool) {}
    pub fn resize_width(&self, val: i32) -> Result<()> {
        Ok(())
    }
    pub fn resize_height(&self, val: i32) -> Result<()> {
        Ok(())
    }
    pub fn transform_x(&self, val: i32) -> Result<()> {
        Ok(())
    }
    pub fn transform_y(&self, val: i32) -> Result<()> {
        Ok(())
    }
    pub fn cycle_floating_app(&self, direction: &Direction) -> Result<()> {
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
    pub fn cycle_focus_app(&mut self, direction: &Direction) -> Result<()> {
        let (_, monitor, workspace_apps) = self.workspace_props()?;

        if workspace_apps.is_empty() {
            return Ok(());
        }

        if let Some(current_active_app) = self.active_app {
            if let Some(current_index) = workspace_apps
                .iter()
                .position(|app| app.hwnd == current_active_app)
            {
                let new_index = match (direction, self.is_rtl()?) {
                    (Direction::Prev, true) => current_index.saturating_add(1),
                    (Direction::Next, true) => current_index.saturating_sub(1),
                    (Direction::Prev, false) => current_index.saturating_sub(1),
                    (Direction::Next, false) => current_index.saturating_add(1),
                }
                .clamp(0, workspace_apps.len() - 1);

                let hwnd = workspace_apps[new_index].hwnd;
                let previous_hwnd = workspace_apps[current_index].hwnd;
                let _app_count_in_monitor = if self.is_rtl()? {
                    let count = workspace_apps
                        .iter()
                        .filter(|app| (app.rect.width + app.rect.x) > monitor.left)
                        .count();
                    count
                } else {
                    2
                };
                WinAPI::focus_app(hwnd)?;
                let prev_counter = 2;
                let is_active_full = self.stored_data.get(&hwnd).is_some_and(|f| f.ratio == 1.0);
                let is_previous_full = self
                    .stored_data
                    .get(&previous_hwnd)
                    .is_some_and(|f| f.ratio == 1.0);

                match (current_index, new_index) {
                    // Wrapping backward: at 0, press prev -> bring last index to 0
                    (0, 0) => {
                        let hwnd = workspace_apps[workspace_apps.len() - 1].hwnd;
                        self.move_app_to_first(hwnd);
                        WinAPI::focus_app(hwnd)?;
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
                            self.move_app_to_last(workspace_apps[idx].hwnd);
                        }
                        // Update active_app and focus after moves are complete
                        WinAPI::focus_app(target_hwnd)?;
                    }

                    // Normal focus change - do nothing
                    _ => {}
                }

                if let Err(err) = self.arrange_layout() {
                    log_error!("Error arrange layout", err);
                }
            }
        } else {
            //active app is not set, force the focus app to index 1
            let (_, _, apps) = self.workspace_props()?;
            if apps.len() > 0
                && let Some(app) = apps.get(0)
            {
                self.active_app = Some(app.hwnd);
                WinAPI::focus_app(app.hwnd)?;
            }
        }
        self.sync_z_order();

        Ok(())
    }
    fn find_real_index(&self, hwnd: isize) -> Option<usize> {
        self.apps.iter().position(|a| a.hwnd == hwnd)
    }
    pub fn move_app(&mut self, direction: &Direction) -> Result<()> {
        let active_app = self.get_active_app()?;
        let (_, _, apps) = self.workspace_props()?;
        if let Some(index) = apps.iter().position(|app| app.hwnd == active_app) {
            let sibling = match (direction, self.is_rtl()?) {
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
            self.arrange_layout()?;
        }
        Ok(())
    }
    //==============================================================================//
    // tag         : WORKSPACES
    // description :
    //==============================================================================

    fn clear_selection(&mut self) -> Result<()> {
        let (_, _, apps) = self.workspace_props()?;
        if apps.is_empty() {
            self.active_app = None;
            let overlay = self.border.lock();
            let overlay = overlay
                .as_ref()
                .ok_or_else(|| anyhow!("Cannot find border overlay"))?;
            overlay.clear_focus();
        }
        Ok(())
    }

    //it return current_workspace, currenct_monitor, workspace_apps
    fn workspace_props(&self) -> Result<(usize, &MonitorInfo, Vec<WindowsAppData>)> {
        let floating_app = self.floating_apps();
        let monitor = self.monitor_manager.get_monitor_in_cursor()?;
        let active_workspace = self
            .user_widgets
            .lock()
            .active_workspace_per_monitor
            .get(monitor.index)
            .copied()
            .unwrap_or(0);
        let workspace_hwnds: Vec<isize> = self
            .stored_data
            .iter()
            .filter_map(|(hwnd, data)| {
                if data.workspace == active_workspace && data.monitor == monitor.index {
                    Some(*hwnd)
                } else {
                    None
                }
            })
            .collect();

        let workspace_apps = self
            .apps
            .iter()
            .filter(|app| workspace_hwnds.contains(&app.hwnd) && !floating_app.contains(&app.hwnd))
            .cloned()
            .collect::<Vec<_>>();
        Ok((active_workspace, monitor, workspace_apps))
    }
    fn toggle_visibility_on_workspace(&mut self) -> Result<()> {
        let (active_workspace, active_monitor, _) = self.workspace_props()?;

        for app in &self.apps {
            if let Some(data) = self.stored_data.get(&app.hwnd) {
                if data.monitor == active_monitor.index {
                    if data.workspace == active_workspace {
                        WinAPI::show_window(app.hwnd);
                    } else {
                        WinAPI::hide_window(app.hwnd);
                    }
                }
            }
        }
        self.clear_selection()?;

        Ok(())
    }
    pub fn move_app_to_monitor(&mut self, direction: &Direction) -> Result<()> {
        let monitor_index = {
            let (_, m, _) = self.workspace_props()?;
            m.index
        };
        if let Some(active_app) = self.active_app {
            let (new_monitor_index, x, y) = {
                let target_monitor = match direction {
                    Direction::Prev => self.monitor_manager.next(monitor_index)?,
                    Direction::Next => self.monitor_manager.next(monitor_index)?,
                };
                let (x, y) = (
                    target_monitor.left + (target_monitor.width / 2),
                    target_monitor.height / 2,
                );
                (target_monitor.index, x, y)
            };

            {
                self.update_stored_data(active_app, |sd| sd.monitor = new_monitor_index);
                self.arrange_layout()?;
                std::thread::sleep(Duration::from_millis(10));
                WinAPI::set_cursor_position(x, y)?;
                self.arrange_layout()?;
            }
        }
        Ok(())
    }
    pub fn move_app_to_workspace(&self, direction: &Direction) -> Result<()> {
        Ok(())
    }
    pub fn cycle_workspace(&mut self, direction: &Direction) -> Result<()> {
        {
            let mut widgets = self.user_widgets.lock();
            let workspace_count = widgets.workspaces.len();
            let active_monitor = self.monitor_manager.get_monitor_in_cursor()?;
            if let Some(active) = widgets
                .active_workspace_per_monitor
                .get_mut(active_monitor.index)
            {
                *active = match direction {
                    Direction::Prev => (*active + workspace_count - 1) % workspace_count,
                    Direction::Next => (*active + 1) % workspace_count,
                };
            }
            widgets.refresh_statusbar();
        }
        self.toggle_visibility_on_workspace()?;
        Ok(())
    }
    fn nearest_factor(&self, value: f32, v: &[f32]) -> Option<usize> {
        v.iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                (**a - value)
                    .abs()
                    .partial_cmp(&(**b - value).abs())
                    .unwrap()
            })
            .map(|(index, _)| index)
    }
    fn get_active_app(&self) -> Result<isize> {
        let app = self.active_app.ok_or(anyhow!("Cant find active app"))?;

        Ok(app)
    }

    pub fn adjust_app_width(&mut self, value: i32) -> Result<()> {
        let (_, monitor, _) = self.workspace_props()?;
        let active_app = self.get_active_app()?;
        let app_index = self
            .apps
            .iter()
            .position(|a| a.hwnd == active_app)
            .ok_or_else(|| anyhow!("Active app not found"))?;
        let (hwnd, app_width) = {
            let app = &self.apps[app_index];
            (app.hwnd, app.rect.width)
        };
        let mut new_ratio = (app_width + value) as f32 / monitor.width as f32;
        if new_ratio > 1.0 {
            new_ratio = 1.0
        };
        self.scale_app_by(hwnd, app_index, new_ratio, monitor.width as f32)?;
        self.arrange_layout()?;
        Ok(())
    }
    pub fn cycle_size_factor(&mut self) -> Result<()> {
        let size_factor = &self.config.size_factor;
        let (_, monitor, _) = self.workspace_props()?;
        let active_app = self.get_active_app()?;
        let app_index = self
            .apps
            .iter()
            .position(|a| a.hwnd == active_app)
            .ok_or_else(|| anyhow!("Active app not found"))?;

        let hwnd = self.apps[app_index].hwnd;
        let current_ratio = self
            .stored_data
            .get(&hwnd)
            .map(|data| data.ratio)
            .unwrap_or(1.0);
        if let Some(index) = self.nearest_factor(current_ratio, size_factor) {
            let new_pos = (index + 1) % size_factor.len();
            let new_ratio = size_factor[new_pos];
            self.scale_app_by(hwnd, app_index, new_ratio, monitor.width as f32)?;
            self.arrange_layout()?;
        }

        Ok(())
    }
    fn scale_app_by(
        &mut self,
        hwnd: isize,
        app_index: usize,
        new_ratio: f32,
        monitor_width: f32,
    ) -> Result<()> {
        let padding = WinAPI::get_rect_padding(h!(hwnd));
        let width = (monitor_width * new_ratio) as i32 + padding.x;
        self.update_stored_data(hwnd, |up| {
            up.ratio = new_ratio;
        });
        let app = &mut self.apps[app_index];
        let mut target_rect = app.rect.clone();
        target_rect.width = width;
        app.rect = target_rect;
        Ok(())
    }
    pub fn swap_focus(&mut self) -> Result<()> {
        let monitor = self.monitor_manager.get_monitor_in_cursor()?;
        let ws_apps = self
            .stored_data
            .iter()
            .filter_map(|(h, a)| {
                if a.monitor == monitor.index {
                    Some(*h)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let ws_apps = self
            .apps
            .iter()
            .filter(|a| ws_apps.contains(&a.hwnd))
            .collect::<Vec<_>>();
        let floating_apps = self
            .stored_data
            .iter()
            .filter_map(|(hwnd, a)| if a.floating { Some(*hwnd) } else { None })
            .collect::<Vec<_>>();
        log_debug!("Floating:", floating_apps.len(), " WS:", ws_apps.len());
        if !floating_apps.is_empty() {
            let (_, _, apps) = self.workspace_props()?;
            log_debug!(
                ws_apps
                    .iter()
                    .map(|a| a.name.clone())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            if let Some(active_app) = self.active_app {
                if floating_apps.contains(&active_app) {
                    if let Some(app) = apps.get(0) {
                        self.active_app = Some(app.hwnd);
                        self.update_border(app)?;
                        self.sync_z_order();
                    }
                } else {
                    if let Some(app) = floating_apps.get(0) {
                        if let Some(app) = ws_apps.iter().find(|a| a.hwnd == *app) {
                            self.active_app = Some(app.hwnd);
                            self.update_border(app)?;
                            self.sync_z_order();
                        }
                    }
                }
            }
        }
        Ok(())
    }
    pub fn switch_monitor(&mut self) -> Result<()> {
        // set cursor position
        {
            let (_, monitor, _) = self.workspace_props()?;
            let target_monitor = self.monitor_manager.next(monitor.index)?;
            let (x, y) = (
                target_monitor.left + (target_monitor.width / 2),
                target_monitor.height / 2,
            );
            WinAPI::set_cursor_position(x, y)?;
        }
        {
            let (_, _, apps) = self.workspace_props()?;
            if let Some(app) = apps.get(0) {
                self.active_app = Some(app.hwnd);
                self.update_border(app)?;
            }
        }
        Ok(())
    }
    pub fn toggle_floating(&mut self) -> Result<()> {
        if let Some(app) = self.active_app {
            let monitor = {
                let monitor = self.monitor_manager.get_monitor_in_cursor()?;
                monitor.to_owned()
            };
            let mut app_rect: Option<WinRect> = None;
            self.update_stored_data(app, |hd| {
                hd.floating = !hd.floating;

                WinAPI::toggle_top_most(hd.floating, h!(app));
                if hd.floating {
                    let rect = WinAPI::center_scale(h!(app), &monitor);
                    app_rect = Some(rect);
                }
            });
            if let Some(app_rect) = app_rect {
                self.update_rect(app, |ud| {
                    ud.rect = app_rect;
                });
            }
        }
        if let Err(err) = self.arrange_layout() {
            log_error!("Failed to arrange layout while toggle floating mode", err);
        }
        Ok(())
    }

    fn get_top_app(&self) -> Result<HWND> {
        // let appss = self.apps.iter().map(|f| f.name.clone()).collect::<Vec<_>>();
        // log_debug!(appss.join(","));
        let hwnds: HashSet<isize> = self.apps.iter().map(|f| f.hwnd).collect();
        WinAPI::get_top_zorder_of(hwnds)
    }

    fn update_stored_data<F>(&mut self, hwnd: isize, updater: F)
    where
        F: FnOnce(&mut StoredData),
    {
        if let Some(data) = self.stored_data.get_mut(&hwnd) {
            updater(data);
        }
    }
    fn update_rect<F: FnOnce(&mut WindowsAppData)>(&mut self, hwnd: isize, updater: F) {
        if let Some(old_app) = self.apps.iter_mut().find(|old| old.hwnd == hwnd) {
            updater(old_app);
        }
    }
    fn update_app_rect(&mut self, app: &WindowsAppData) -> Result<()> {
        let monitor_index = { self.monitor_manager.get_monitor_in_cursor()?.index };
        // let ratio = app.rect.width as f32 / monitor.width as f32;
        if let Some(old_app) = self.apps.iter_mut().find(|old| old.hwnd == app.hwnd) {
            old_app.rect = app.rect.clone();
        }
        self.update_stored_data(app.hwnd, |hd| {
            hd.monitor = monitor_index;
        });
        Ok(())
    }

    fn update_border(&self, app: &WindowsAppData) -> Result<()> {
        {
            // let border = self.border.lock();
            // let border = border.as_ref().ok_or_else(|| anyhow!("Cant find border"))?;
            // border.clear_focus();
        };
        let y = if app.is_maximised {
            app.rect.y + (app.padding.y / 2)
        } else {
            app.rect.y
        };
        let x = app.rect.x + (app.padding.x / 2);
        let width = app.rect.width - app.padding.x;
        let height = app.rect.height - app.padding.y;
        let target = Some(app.hwnd);
        let is_floating = self
            .stored_data
            .get(&app.hwnd)
            .and_then(|f| if f.floating { Some(true) } else { None })
            .unwrap_or(false);
        let (thickness, color) = if is_floating {
            (2.0, col!(error))
        } else {
            (1.0, col!(warning))
        };
        let info = BorderInfo {
            x,
            y,
            width,
            height,
            top_most: is_floating,
            color,
            thickness,
            radius: 5.0,
            target,
        };

        let border_hwnd = {
            let border = self.border.lock();
            let border = border.as_ref().ok_or_else(|| anyhow!("Cant find border"))?;
            border.set_focus(info);
            border.hwnd()
        };

        if let Ok(top) = self.get_top_app()
            && !is_floating
        {
            WinAPI::order_z_order(border_hwnd, top);
        }

        Ok(())
    }
    fn is_rtl(&self) -> Result<bool> {
        let active_monitor = self.monitor_manager.get_monitor_in_cursor()?.index;
        Ok(active_monitor == 0)
    }
    fn sync_z_order(&mut self) {
        self.stored_data.iter().for_each(|(hwnd, app)| {
            WinAPI::toggle_top_most(app.floating, h!(*hwnd));
        });
    }

    pub fn arrange_layout(&mut self) -> Result<()> {
        let now = Instant::now();

        #[cfg(debug_assertions)]
        {
            use crate::log_info;
            let elapsed = self.last_update.elapsed().as_secs();
            log_info!("Update after :", elapsed, "s");
            self.last_update = Instant::now();
        }
        let app_position = {
            let monitor = self.monitor_manager.get_monitor_in_cursor()?;
            let toolbar_height = get_statusbar_height(monitor.index) as i32;
            let hwnds = self
                .stored_data
                .iter()
                .filter_map(|(hwnd, app)| {
                    if app.monitor == monitor.index {
                        Some(*hwnd)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let apps = self
                .apps
                .iter()
                .filter(|app_info| hwnds.contains(&app_info.hwnd))
                .collect::<Vec<_>>();
            if apps.is_empty() {
                return Ok(());
            }

            let mut cursor_x = if self.is_rtl()? {
                monitor.left + monitor.width
            } else {
                monitor.left
            };

            let mut app_new_pos = Vec::new();

            for (_, app) in apps.iter().enumerate() {
                if self
                    .stored_data
                    .get(&app.hwnd)
                    .map(|f| f.floating)
                    .unwrap_or(false)
                {
                    continue;
                }
                let p = WinAPI::get_rect_padding(h!(app.hwnd));

                let (px, py) = (p.x, p.y);
                let (w, visible_w) = if apps.len() == 1 {
                    (monitor.width + px, monitor.width)
                } else {
                    (app.rect.width, app.rect.width - px)
                };
                let h = monitor.height + (py / 2) - toolbar_height;
                let target_rect = if self.is_rtl()? {
                    cursor_x -= visible_w;
                    WinRect {
                        x: cursor_x - px / 2,
                        y: toolbar_height,
                        width: w,
                        height: h,
                    }
                } else {
                    let rect = WinRect {
                        x: cursor_x - px / 2,
                        y: toolbar_height,
                        width: w,
                        height: h,
                    };
                    cursor_x += visible_w;
                    rect
                };
                // log_debug!("Arrange", &app.name);
                WinAPI::set_position(app.hwnd, &target_rect, true);
                app_new_pos.push((app.hwnd, target_rect));
            }

            app_new_pos
        };

        for (app, rect) in app_position {
            if let Some(old_app) = self.apps.iter_mut().find(|a| a.hwnd == app) {
                old_app.rect = rect;
            }
        }

        #[cfg(debug_assertions)]
        {
            let elapsed = now.elapsed().as_millis();
            log_error!("arrange layout in: ", elapsed, "ms");
        }
        self.write_log()?;
        Ok(())
    }
    fn write_log(&self) -> Result<()> {
        let apps = ntek::to_str(&self.apps);
        std::fs::write("log.ntek", apps)?;
        Ok(())
    }
    //==============================================================================//
    // tag         : Add or Remove Apps entries
    // description : this is where add, remove app happen
    //==============================================================================//
    pub fn add_app(&mut self, app: &WindowsAppData) -> Result<()> {
        if self.get_blacklist().contains(&app.name) {
            return Ok(());
        }
        log_debug!("add new app", &app.name, app.hwnd);
        if !self.apps.iter().find(|a| a.hwnd == app.hwnd).is_some() {
            log_debug!("INSERT", &app.name, app.hwnd);
            self.apps.insert(0, app.clone());
        }
        let monitor = self.monitor_manager.get_app_monitor(h!(app.hwnd))?;
        if !self.stored_data.contains_key(&app.hwnd) {
            self.stored_data.insert(
                app.hwnd,
                StoredData {
                    floating: false,
                    ratio: 0.5,
                    name: app.name.clone(),
                    monitor: monitor.index,
                    workspace: 0,
                },
            );
        }
        self.arrange_layout()?;
        Ok(())
    }

    fn add_app_if_not_listed(&mut self, app: &WindowsAppData) -> Result<()> {
        if self.get_blacklist().contains(&app.name) {
            return Ok(());
        }

        if self.stored_data.get(&app.hwnd).is_none() {
            self.add_app(app)?;
            self.arrange_layout()?;
        }
        Ok(())
    }

    pub fn update_app(&mut self, app: &WindowsAppData, kind: UpdateKind) -> Result<()> {
        match kind {
            UpdateKind::Location => {
                if let Ok(_) = self.add_app_if_not_listed(app) {
                    if let Some(active_app) = self.active_app {
                        if app.hwnd == active_app {
                            self.update_border(app)?;
                        }
                        if WinAPI::is_maximized(h!(active_app))? {
                            let monitor = self.monitor_manager.get_monitor_in_cursor()?;
                            let new_ratio = 1.0;
                            let app_index = self
                                .apps
                                .iter()
                                .position(|a| a.hwnd == active_app)
                                .ok_or(anyhow!("Cant find the app index"))?;
                            self.scale_app_by(
                                active_app,
                                app_index,
                                new_ratio,
                                monitor.width as f32,
                            )?;
                            self.arrange_layout()?;
                        }
                    }
                }
            }
            UpdateKind::Title => {
                {
                    if let Err(err) = self.widget_update_title(app) {
                        log_error!("Error changing title", err);
                    }
                }
                if let Err(err) = self.add_app_if_not_listed(app) {
                    log_error!("Failed adding app to the list: ", err);
                }
            }
            UpdateKind::Foreground => {
                self.active_app = Some(app.hwnd);
                if let Err(err) = self.widget_update_title(app) {
                    log_error!("Error changing title", err);
                }
                self.update_border(app)?;
                self.sync_z_order();
            }
            UpdateKind::MoveSize => {
                self.update_app_rect(app)?;
                if let Err(err) = self.arrange_layout() {
                    log_error!("Error arrange layout", err);
                }
            }
        }

        Ok(())
    }
    pub fn test_debug(&self) -> Result<()> {
        log_error!("TEST DEBUG");
        let apps = WinAPI::walk_z_order()?;
        log_debug!(d!(apps));

        Ok(())
    }
    pub fn delete_app(&mut self, hwnd: isize) -> Result<()> {
        log_debug!("DELETE", hwnd);
        self.apps.retain(|a| a.hwnd != hwnd);
        self.stored_data.remove(&hwnd);
        self.arrange_layout()?;
        Ok(())
    }
}
