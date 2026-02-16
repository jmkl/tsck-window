// ============================================================================
// Core Structures
// ============================================================================

use crate::{
    AppPosition, AppSize,
    api::{APPINFO_LIST, WinHook},
    appinfo::AppInfo,
    win_api::{self, MonitorInfo},
};

pub struct WorkspaceInfo {
    index: usize,
    monitor: usize,
    app: AppInfo,
}

pub struct WinHookHandler {
    active_index: usize,
    apps: Vec<AppInfo>,
    monitors: Vec<MonitorInfo>,
    workspace_entries: Vec<WorkspaceInfo>,
}

// ============================================================================
// Constructor & Initialization
// ============================================================================

impl WinHookHandler {
    pub fn new() -> Self {
        Self {
            active_index: 0,
            apps: Vec::new(),
            monitors: Vec::new(),
            workspace_entries: Vec::new(),
        }
    }

    pub fn init(&mut self) {
        self.refresh_apps();
        self.refresh_monitors();
        self.init_workspace_apps(&["Zed.exe", "zen.exe", "wezterm-gui.exe"]);
    }

    /// Refresh the list of active applications
    pub fn refresh_apps(&mut self) {
        let list = APPINFO_LIST.lock();
        self.apps = list
            .iter()
            .filter(|(_, ai)| !WinHook::blacklist(ai))
            .map(|(_, ai)| ai.clone())
            .collect();
    }

    /// Refresh the list of monitors
    pub fn refresh_monitors(&mut self) {
        self.monitors = win_api::get_all_monitors();
    }
}

// ============================================================================
// App Query & Retrieval APIs
// ============================================================================

impl WinHookHandler {
    /// Get all currently active apps
    pub fn get_all_apps(&mut self) -> &[AppInfo] {
        self.refresh_apps();
        self.apps.as_slice()
    }

    /// Get the currently focused app
    pub fn get_current_active_app(&mut self) -> Option<&mut AppInfo> {
        self.apps.get_mut(self.active_index)
    }

    /// Get app by index
    pub fn get_app_by_index(&mut self, index: usize) -> Option<&mut AppInfo> {
        self.apps.get_mut(index)
    }

    /// Get app by name/exe
    pub fn get_app_by_name(&mut self, name: &str) -> Option<&mut AppInfo> {
        self.apps.iter_mut().find(|f| f.exe.contains(name))
    }

    /// Get index of app by name
    pub fn get_index_by_name(&self, name: &str) -> Option<usize> {
        self.apps.iter().position(|f| f.exe.contains(name))
    }

    /// Get active app's size
    pub fn get_active_app_size(&self) -> Option<&AppSize> {
        self.apps.get(self.active_index).map(|app| &app.size)
    }

    /// Get active app's position
    pub fn get_active_app_position(&self) -> Option<&AppPosition> {
        self.apps.get(self.active_index).map(|app| &app.position)
    }

    /// Get apps on a specific monitor
    pub fn get_apps_on_monitor(&self, monitor_index: usize) -> Vec<&AppInfo> {
        let monitor = match self.monitors.get(monitor_index) {
            Some(m) => m,
            None => return Vec::new(),
        };

        self.apps
            .iter()
            .filter(|app| self.is_app_on_monitor(app, monitor))
            .collect()
    }

    /// Check if an app is on a specific monitor
    fn is_app_on_monitor(&self, app: &AppInfo, monitor: &MonitorInfo) -> bool {
        let app_center_x = app.position.x + app.size.width / 2;
        let app_center_y = app.position.y + app.size.height / 2;

        app_center_x >= monitor.left
            && app_center_x < monitor.left + monitor.width
            && app_center_y >= monitor.top
            && app_center_y < monitor.top + monitor.height
    }
}

// ============================================================================
// Focus Management APIs
// ============================================================================

impl WinHookHandler {
    /// Focus the current active app
    pub fn focus_app(&mut self) {
        if let Some(app) = self.apps.get(self.active_index) {
            app.bring_to_front();
        }
    }

    /// Focus next app in the list
    pub fn focus_next(&mut self) {
        self.next();
        self.focus_app();
    }

    /// Focus previous app in the list
    pub fn focus_prev(&mut self) {
        self.prev();
        self.focus_app();
    }

    /// Focus app by name
    pub fn focus_app_by_name(&mut self, name: &str) -> Option<()> {
        let index = self.get_index_by_name(name)?;
        self.active_index = index;
        self.focus_app();
        Some(())
    }

    /// Focus app by index
    pub fn focus_app_by_index(&mut self, index: usize) -> Option<()> {
        if index < self.apps.len() {
            self.active_index = index;
            self.focus_app();
            Some(())
        } else {
            None
        }
    }

    /// Internal: Update focused app based on hwnd
    pub(crate) fn on_app_focused(&mut self, hwnd: isize) {
        if let Some(index) = self.apps.iter().position(|d| d.hwnd == hwnd) {
            self.active_index = index;
        }
    }

    fn next(&mut self) {
        self.refresh_apps();
        if !self.apps.is_empty() {
            self.active_index = (self.active_index + 1) % self.apps.len();
        }
    }

    fn prev(&mut self) {
        self.refresh_apps();
        if !self.apps.is_empty() {
            let len = self.apps.len();
            self.active_index = (self.active_index + len - 1) % len;
        }
    }
}

// ============================================================================
// Single App Positioning & Sizing APIs
// ============================================================================

impl WinHookHandler {
    /// Move active app to absolute position
    pub fn move_active_app(&mut self, target_position: AppPosition) {
        if let Some(app) = self.get_current_active_app() {
            app.move_app(target_position);
        }
    }

    /// Move active app by relative offset
    pub fn move_active_app_relative(&mut self, x: i32, y: i32) {
        if let Some(app) = self.apps.get_mut(self.active_index) {
            let target_position = AppPosition {
                x: app.position.x + x,
                y: app.position.y + y,
            };
            app.move_app(target_position);
        }
    }

    /// Resize active app to target size
    pub fn resize_active_app(&mut self, target_size: AppSize) {
        if let Some(app) = self.get_current_active_app() {
            app.resize_app(target_size);
        }
    }

    /// Move and resize active app in one operation
    pub fn move_resize_active_app(&mut self, size: AppSize, position: AppPosition) {
        if let Some(app) = self.get_current_active_app() {
            app.move_resize(size, position);
        }
    }

    /// Move app by name
    pub fn move_app_by_name(&mut self, name: &str, position: AppPosition) -> Option<()> {
        let app = self.get_app_by_name(name)?;
        app.move_app(position);
        Some(())
    }

    /// Resize app by name
    pub fn resize_app_by_name(&mut self, name: &str, size: AppSize) -> Option<()> {
        let app = self.get_app_by_name(name)?;
        app.resize_app(size);
        Some(())
    }
}

// ============================================================================
// Monitor Management APIs
// ============================================================================

impl WinHookHandler {
    /// Get all monitors
    pub fn get_monitors(&self) -> &[MonitorInfo] {
        &self.monitors
    }

    /// Get monitor by index
    pub fn get_monitor(&self, index: usize) -> Option<&MonitorInfo> {
        self.monitors.get(index)
    }

    /// Get the primary monitor
    pub fn get_primary_monitor(&self) -> Option<&MonitorInfo> {
        self.monitors.iter().find(|m| m.left == 0)
    }

    /// Get current monitor (monitor containing the active app)
    pub fn get_current_monitor(&self) -> Option<&MonitorInfo> {
        let app = self.apps.get(self.active_index)?;
        self.get_monitor_containing_app(app)
    }

    /// Get monitor that contains a specific app
    pub fn get_monitor_containing_app(&self, app: &AppInfo) -> Option<&MonitorInfo> {
        self.monitors
            .iter()
            .find(|m| self.is_app_on_monitor(app, m))
    }

    /// Get monitor count
    pub fn get_monitor_count(&self) -> usize {
        self.monitors.len()
    }

    /// Move active app to a specific monitor
    pub fn move_active_app_to_monitor(&mut self, monitor_index: usize) -> Option<()> {
        let monitor = self.get_monitor(monitor_index)?;
        let position = AppPosition {
            x: monitor.left,
            y: monitor.left,
        };
        self.move_active_app(position);
        Some(())
    }

    /// Move active app to next monitor
    pub fn move_active_app_to_next_monitor(&mut self) -> Option<()> {
        let current_monitor_index = self.get_current_monitor_index()?;
        let next_index = (current_monitor_index + 1) % self.monitors.len();
        self.move_active_app_to_monitor(next_index)
    }

    /// Move active app to previous monitor
    pub fn move_active_app_to_prev_monitor(&mut self) -> Option<()> {
        let current_monitor_index = self.get_current_monitor_index()?;
        let prev_index = if current_monitor_index == 0 {
            self.monitors.len() - 1
        } else {
            current_monitor_index - 1
        };
        self.move_active_app_to_monitor(prev_index)
    }

    fn get_current_monitor_index(&self) -> Option<usize> {
        let app = self.apps.get(self.active_index)?;
        self.monitors
            .iter()
            .position(|m| self.is_app_on_monitor(app, m))
    }
}

// ============================================================================
// Workspace Management APIs
// ============================================================================

impl WinHookHandler {
    /// Initialize workspace with specific apps
    pub fn init_workspace_apps(&mut self, app_names: &[&str]) -> Option<()> {
        self.refresh_apps();

        let monitor = self.get_primary_monitor()?;
        let (monitor_width, monitor_height) = (monitor.width, monitor.height);

        // Clear existing workspace entries
        self.workspace_entries.clear();

        // Find and add apps to workspace
        for (index, app) in self.apps.iter().enumerate() {
            if app_names.contains(&app.exe.as_str()) {
                self.workspace_entries.push(WorkspaceInfo {
                    index,
                    monitor: 0,
                    app: app.clone(),
                });
            }
        }

        if self.workspace_entries.is_empty() {
            return None;
        }

        // Apply tiling layout
        self.apply_horizontal_tile_layout(monitor_width, monitor_height);
        Some(())
    }

    /// Get all workspace entries
    pub fn get_workspace_entries(&self) -> &[WorkspaceInfo] {
        &self.workspace_entries
    }

    /// Add app to workspace
    pub fn add_app_to_workspace(&mut self, app_name: &str, monitor_index: usize) -> Option<()> {
        let index = self.get_index_by_name(app_name)?;
        let app = self.apps.get(index)?;

        // Check if already in workspace
        if self
            .workspace_entries
            .iter()
            .any(|ws| ws.app.hwnd == app.hwnd)
        {
            return None;
        }

        self.workspace_entries.push(WorkspaceInfo {
            index,
            monitor: monitor_index,
            app: app.clone(),
        });

        Some(())
    }

    /// Remove app from workspace
    pub fn remove_app_from_workspace(&mut self, app_name: &str) -> Option<()> {
        let pos = self
            .workspace_entries
            .iter()
            .position(|ws| ws.app.exe.contains(app_name))?;
        self.workspace_entries.remove(pos);
        Some(())
    }

    /// Clear all workspace entries
    pub fn clear_workspace(&mut self) {
        self.workspace_entries.clear();
    }
}

// ============================================================================
// Layout & Arrangement APIs
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub enum LayoutType {
    HorizontalTile,
    VerticalTile,
    Grid,
    Columns(usize),
    Master,
}

impl WinHookHandler {
    /// Arrange workspace apps using specified layout on a monitor
    pub fn arrange_workspace(&mut self, layout: LayoutType, monitor_index: usize) -> Option<()> {
        let monitor = self.get_monitor(monitor_index)?;

        match layout {
            LayoutType::HorizontalTile => {
                self.apply_horizontal_tile_layout(monitor.width, monitor.height)
            }
            LayoutType::VerticalTile => {
                self.apply_vertical_tile_layout(monitor.width, monitor.height)
            }
            LayoutType::Grid => self.apply_grid_layout(monitor.width, monitor.height),
            LayoutType::Columns(count) => {
                self.apply_column_layout(monitor.width, monitor.height, count)
            }
            LayoutType::Master => self.apply_master_stack_layout(monitor.width, monitor.height),
        }

        Some(())
    }

    /// Arrange all apps on a specific monitor with horizontal tiling
    pub fn tile_apps_on_monitor(&mut self, monitor_index: usize) -> Option<()> {
        // Extract monitor values first to end the immutable borrow
        let (monitor_x, monitor_y, monitor_width, monitor_height) = {
            let monitor = self.get_monitor(monitor_index)?;
            (monitor.left, monitor.top, monitor.width, monitor.height)
        };

        let apps_on_monitor: Vec<usize> = self
            .apps
            .iter()
            .enumerate()
            .filter(|(_, app)| {
                let app_center_x = app.position.x + app.size.width / 2;
                let app_center_y = app.position.y + app.size.height / 2;
                app_center_x >= monitor_x
                    && app_center_x < monitor_x + monitor_width
                    && app_center_y >= monitor_y
                    && app_center_y < monitor_y + monitor_height
            })
            .map(|(idx, _)| idx)
            .collect();

        if apps_on_monitor.is_empty() {
            return None;
        }

        let count = apps_on_monitor.len();
        let tile_width = monitor_width / count as i32;

        for (i, app_idx) in apps_on_monitor.iter().enumerate() {
            if let Some(app) = self.apps.get_mut(*app_idx) {
                let size = AppSize {
                    width: tile_width,
                    height: monitor_height,
                };
                let position = AppPosition {
                    x: monitor_x + (tile_width * i as i32),
                    y: monitor_y,
                };
                app.move_resize(size, position);
            }
        }

        Some(())
    }
    /// Maximize active app on current monitor
    pub fn maximize_active_app_on_monitor(&mut self) -> Option<()> {
        let monitor = self.get_current_monitor()?.clone();
        let size = AppSize {
            width: monitor.width,
            height: monitor.height,
        };
        let position = AppPosition {
            x: monitor.left,
            y: monitor.left,
        };
        self.move_resize_active_app(size, position);
        Some(())
    }

    /// Snap active app to half of monitor (left or right)
    pub fn snap_active_app_half(&mut self, left: bool) -> Option<()> {
        let monitor = self.get_current_monitor()?.clone();
        let size = AppSize {
            width: monitor.width / 2,
            height: monitor.height,
        };
        let position = AppPosition {
            x: if left {
                monitor.left
            } else {
                monitor.left + monitor.width / 2
            },
            y: monitor.left,
        };
        self.move_resize_active_app(size, position);
        Some(())
    }

    /// Snap active app to quarter of monitor
    pub fn snap_active_app_quarter(&mut self, top: bool, left: bool) -> Option<()> {
        let monitor = self.get_current_monitor()?.clone();
        let size = AppSize {
            width: monitor.width / 2,
            height: monitor.height / 2,
        };
        let position = AppPosition {
            x: if left {
                monitor.left
            } else {
                monitor.left + monitor.width / 2
            },
            y: if top {
                monitor.left
            } else {
                monitor.left + monitor.height / 2
            },
        };
        self.move_resize_active_app(size, position);
        Some(())
    }

    /// Apply horizontal tile layout to workspace apps
    fn apply_horizontal_tile_layout(&mut self, width: i32, height: i32) {
        let count = self.workspace_entries.len();
        if count == 0 {
            return;
        }

        let tile_width = width / count as i32;

        for (i, ws_info) in self.workspace_entries.iter_mut().enumerate() {
            let size = AppSize {
                width: tile_width,
                height,
            };
            let position = AppPosition {
                x: tile_width * i as i32,
                y: 0,
            };
            ws_info.app.move_resize(size, position);
        }
    }

    /// Apply vertical tile layout to workspace apps
    fn apply_vertical_tile_layout(&mut self, width: i32, height: i32) {
        let count = self.workspace_entries.len();
        if count == 0 {
            return;
        }

        let tile_height = height / count as i32;

        for (i, ws_info) in self.workspace_entries.iter_mut().enumerate() {
            let size = AppSize {
                width,
                height: tile_height,
            };
            let position = AppPosition {
                x: 0,
                y: tile_height * i as i32,
            };
            ws_info.app.move_resize(size, position);
        }
    }

    /// Apply grid layout to workspace apps
    fn apply_grid_layout(&mut self, width: i32, height: i32) {
        let count = self.workspace_entries.len();
        if count == 0 {
            return;
        }

        let cols = (count as f32).sqrt().ceil() as i32;
        let rows = (count as f32 / cols as f32).ceil() as i32;
        let tile_width = width / cols;
        let tile_height = height / rows;

        for (i, ws_info) in self.workspace_entries.iter_mut().enumerate() {
            let row = i as i32 / cols;
            let col = i as i32 % cols;

            let size = AppSize {
                width: tile_width,
                height: tile_height,
            };
            let position = AppPosition {
                x: col * tile_width,
                y: row * tile_height,
            };
            ws_info.app.move_resize(size, position);
        }
    }

    /// Apply column layout with specified number of columns
    fn apply_column_layout(&mut self, width: i32, height: i32, columns: usize) {
        let count = self.workspace_entries.len();
        if count == 0 || columns == 0 {
            return;
        }

        let col_width = width / columns as i32;
        let apps_per_col = (count as f32 / columns as f32).ceil() as usize;

        for (i, ws_info) in self.workspace_entries.iter_mut().enumerate() {
            let col = i / apps_per_col;
            let row = i % apps_per_col;
            let apps_in_this_col = (count - col * apps_per_col).min(apps_per_col);
            let tile_height = height / apps_in_this_col as i32;

            let size = AppSize {
                width: col_width,
                height: tile_height,
            };
            let position = AppPosition {
                x: col as i32 * col_width,
                y: row as i32 * tile_height,
            };
            ws_info.app.move_resize(size, position);
        }
    }

    /// Apply master-stack layout (one large window + stacked smaller windows)
    fn apply_master_stack_layout(&mut self, width: i32, height: i32) {
        let count = self.workspace_entries.len();
        if count == 0 {
            return;
        }

        if count == 1 {
            // Just one window - maximize it
            let ws_info = &mut self.workspace_entries[0];
            ws_info
                .app
                .move_resize(AppSize { width, height }, AppPosition { x: 0, y: 0 });
            return;
        }

        // Master window takes 60% of width
        let master_width = (width as f32 * 0.6) as i32;
        let stack_width = width - master_width;
        let stack_height = height / (count - 1) as i32;

        // First window is master
        self.workspace_entries[0].app.move_resize(
            AppSize {
                width: master_width,
                height,
            },
            AppPosition { x: 0, y: 0 },
        );

        // Rest are stacked
        for (i, ws_info) in self.workspace_entries.iter_mut().enumerate().skip(1) {
            let size = AppSize {
                width: stack_width,
                height: stack_height,
            };
            let position = AppPosition {
                x: master_width,
                y: (i - 1) as i32 * stack_height,
            };
            ws_info.app.move_resize(size, position);
        }
    }
}
