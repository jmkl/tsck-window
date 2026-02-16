use std::collections::HashMap;

use crate::{
    AppPosition, AppSize,
    api::{APPINFO_LIST, WinHook},
    appinfo::AppInfo,
    fibonacci_layout::{SplitNode, calculate_fibonacci_layout_with_ratios},
    win_api::{self, MonitorInfo},
};
pub static TOOLBAR_HEIGHT: i32 = 0;
const GLOBAL_Y: i32 = TOOLBAR_HEIGHT;

#[derive(Debug, Clone)]
pub struct WorkspaceInfo {
    pub index: usize,
    pub monitor_index: usize,
    pub app: AppInfo,
}

pub struct WinHookHandler {
    active_index: usize,
    apps: Vec<AppInfo>,
    monitors: Vec<MonitorInfo>,
    workspace_entries: Vec<WorkspaceInfo>,
    swap: bool,
    pub resize_ratios: HashMap<usize, Vec<f32>>,
    pub split_nodes: HashMap<usize, Vec<SplitNode>>, // Add this
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
            swap: false,
            resize_ratios: HashMap::new(),
            split_nodes: HashMap::new(),
        }
    }

    pub fn init(&mut self) {
        self.refresh_apps();
        self.refresh_monitors();
        self.init_workspace_apps(&[&["Zed.exe", "zen.exe", "wezterm-gui.exe"], &["chrome.exe"]]);
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
// Arrange Workspace
// ============================================================================
impl WinHookHandler {
    /// Sort workspace entries by index within each monitor
    pub fn sort_by_index(&mut self) {
        self.workspace_entries
            .sort_by_key(|entry| (entry.monitor_index, entry.index));
    }

    /// Swap two entries by their indices within the same monitor
    pub fn swap_by_index(
        &mut self,
        monitor_index: usize,
        index_a: usize,
        index_b: usize,
    ) -> Result<(), String> {
        // Find positions in the vector for the specific monitor
        let pos_a = self
            .workspace_entries
            .iter()
            .position(|entry| entry.monitor_index == monitor_index && entry.index == index_a)
            .ok_or_else(|| format!("Index {} not found on monitor {}", index_a, monitor_index))?;

        let pos_b = self
            .workspace_entries
            .iter()
            .position(|entry| entry.monitor_index == monitor_index && entry.index == index_b)
            .ok_or_else(|| format!("Index {} not found on monitor {}", index_b, monitor_index))?;

        // Swap the entries in the vector
        self.workspace_entries.swap(pos_a, pos_b);

        // Update the index fields to reflect the swap
        self.workspace_entries[pos_a].index = index_a;
        self.workspace_entries[pos_b].index = index_b;

        Ok(())
    }

    pub fn swap_at_positions(
        &mut self,
        monitor_index: usize,
        pos_a: usize,
        pos_b: usize,
    ) -> Result<(), String> {
        // Get all entries for this monitor
        let monitor_entries: Vec<usize> = self
            .workspace_entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.monitor_index == monitor_index)
            .map(|(i, _)| i)
            .collect();

        if pos_a >= monitor_entries.len() {
            return Err(format!(
                "Position {} out of bounds for monitor {}",
                pos_a, monitor_index
            ));
        }
        if pos_b >= monitor_entries.len() {
            return Err(format!(
                "Position {} out of bounds for monitor {}",
                pos_b, monitor_index
            ));
        }

        let vec_pos_a = monitor_entries[pos_a];
        let vec_pos_b = monitor_entries[pos_b];

        // Swap their index values (this is what determines layout order)
        let temp_index = self.workspace_entries[vec_pos_a].index;
        self.workspace_entries[vec_pos_a].index = self.workspace_entries[vec_pos_b].index;
        self.workspace_entries[vec_pos_b].index = temp_index;

        // Re-apply layout after swap
        self.init_layout(monitor_index);

        Ok(())
    }

    pub fn handle_resize(
        &mut self,
        monitor_index: usize,
        window_index: usize,
        new_width: i32,
        new_height: i32,
    ) -> Option<()> {
        let (w, h) = {
            let monitor = self.get_monitor(monitor_index)?;
            (monitor.width, monitor.height)
        };

        let count = self.get_monitor_entries(monitor_index).len();

        // Ensure window_index is valid
        if window_index >= count {
            return None;
        }

        // Get the split node for this window
        // IMPORTANT: Search by window_index, not direct indexing
        let nodes = self.split_nodes.get(&monitor_index)?;
        let node = nodes
            .iter()
            .find(|n| n.window_index == window_index)?
            .clone();

        // Calculate what ratio this resize represents
        let ratios = self
            .resize_ratios
            .entry(monitor_index)
            .or_insert_with(|| vec![0.5; count.max(10)]);

        if ratios.len() < count {
            ratios.resize(count.max(10), 0.5);
        }

        // Update the correct ratio based on the split node
        if node.is_horizontal {
            // Horizontal split - resize affects height
            let new_ratio = (new_height as f32 / node.parent_bounds.height as f32).clamp(0.2, 0.8);
            if node.ratio_index < ratios.len() {
                ratios[node.ratio_index] = new_ratio;
            }
        } else {
            // Vertical split - resize affects width
            let new_ratio = (new_width as f32 / node.parent_bounds.width as f32).clamp(0.2, 0.8);
            if node.ratio_index < ratios.len() {
                ratios[node.ratio_index] = new_ratio;
            }
        }

        // Re-apply layout with new ratios
        self.init_layout(monitor_index)
    }
    /// Reindex all entries based on their current position within each monitor
    pub fn reindex_by_monitor(&mut self) {
        // Group by monitor and reindex
        let mut monitor_counters: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();

        for entry in self.workspace_entries.iter_mut() {
            let counter = monitor_counters.entry(entry.monitor_index).or_insert(0);
            entry.index = *counter;
            *counter += 1;
        }
    }

    /// Move an entry from one index to another within the same monitor
    pub fn move_entry(
        &mut self,
        monitor_index: usize,
        from_index: usize,
        to_index: usize,
    ) -> Result<(), String> {
        // Verify both indices are on the same monitor
        let from_pos = self
            .workspace_entries
            .iter()
            .position(|entry| entry.monitor_index == monitor_index && entry.index == from_index)
            .ok_or_else(|| {
                format!(
                    "Index {} not found on monitor {}",
                    from_index, monitor_index
                )
            })?;

        // Remove the entry
        let mut entry = self.workspace_entries.remove(from_pos);

        // Update its index
        entry.index = to_index;

        // Find where to insert it (among entries on the same monitor)
        let insert_pos = self
            .workspace_entries
            .iter()
            .position(|e| e.monitor_index == monitor_index && e.index > to_index)
            .unwrap_or_else(|| {
                // If no position found, insert after the last entry of this monitor
                self.workspace_entries
                    .iter()
                    .rposition(|e| e.monitor_index == monitor_index)
                    .map(|pos| pos + 1)
                    .unwrap_or(self.workspace_entries.len())
            });

        // Insert at new position
        self.workspace_entries.insert(insert_pos, entry);

        // Reindex only the affected monitor
        self.reindex_monitor(monitor_index);

        Ok(())
    }

    /// Reindex entries for a specific monitor only
    pub fn reindex_monitor(&mut self, monitor_index: usize) {
        let mut index_counter = 0;
        for entry in self.workspace_entries.iter_mut() {
            if entry.monitor_index == monitor_index {
                entry.index = index_counter;
                index_counter += 1;
            }
        }
    }

    /// Get entries for a specific monitor, sorted by index
    pub fn get_monitor_entries(&self, monitor_index: usize) -> Vec<&WorkspaceInfo> {
        let mut entries: Vec<&WorkspaceInfo> = self
            .workspace_entries
            .iter()
            .filter(|entry| entry.monitor_index == monitor_index)
            .collect();
        entries.sort_by_key(|entry| entry.index);
        entries
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
    pub fn focus_app(&mut self, hwnd: isize) {
        if let Some(idx) = self.apps.iter().position(|f| f.hwnd == hwnd) {
            self.active_index = idx
        }
        if let Some(app) = self.apps.iter().find(|ai| ai.hwnd == hwnd) {
            app.bring_to_front();
        }
    }
    /// Focus next app in the list
    pub fn focus_next(&mut self) {
        self.next().focus();
    }

    /// Focus previous app in the list
    pub fn focus_prev(&mut self) {
        self.prev().focus();
    }
    fn focus(&mut self) {
        if let Some(app) = self.apps.get(self.active_index) {
            app.bring_to_front();
        }
    }

    /// Focus app by name
    pub fn focus_app_by_name(&mut self, name: &str) -> Option<()> {
        let index = self.get_index_by_name(name)?;
        self.active_index = index;
        self.focus();
        Some(())
    }

    /// Focus app by index
    pub fn focus_app_by_index(&mut self, index: usize) -> Option<()> {
        if index < self.apps.len() {
            self.active_index = index;
            self.focus();
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

    fn next(&mut self) -> &mut Self {
        self.refresh_apps();
        if !self.apps.is_empty() {
            self.active_index = (self.active_index + 1) % self.apps.len();
        }
        self
    }

    fn prev(&mut self) -> &mut Self {
        self.refresh_apps();
        if !self.apps.is_empty() {
            let len = self.apps.len();
            self.active_index = (self.active_index + len - 1) % len;
        }
        self
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
    pub fn init_workspace_apps(&mut self, app_names: &[&[&str]]) -> Option<()> {
        self.refresh_apps();

        self.workspace_entries.clear();
        for (i, ws) in app_names.iter().enumerate() {
            for (index, app) in self.apps.iter().enumerate() {
                if ws.contains(&app.exe.as_str()) {
                    self.workspace_entries.push(WorkspaceInfo {
                        index,
                        monitor_index: i,
                        app: app.clone(),
                    });
                }
            }
        }

        if self.workspace_entries.is_empty() {
            return None;
        }

        self.init_layout(0);

        Some(())
    }

    /// Get all workspace entries
    pub fn get_workspace_entries(&self, monitor_index: usize) -> Vec<&WorkspaceInfo> {
        let result: Vec<&WorkspaceInfo> = self
            .workspace_entries
            .iter()
            .filter(|d| d.monitor_index == monitor_index)
            .collect();
        result
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
            monitor_index,
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

impl WinHookHandler {
    pub fn swap_layout(&mut self) {
        self.swap = !self.swap;
        self.init_layout(0);
    }

    pub fn init_layout(&mut self, monitor_index: usize) -> Option<()> {
        let monitor_entries = self.get_monitor_entries(monitor_index);
        let count = monitor_entries.len();
        if count == 0 {
            return None;
        }
        let (w, h) = {
            let monitor = self.get_monitor(monitor_index)?;
            (monitor.width, monitor.height)
        };

        let ratios = self
            .resize_ratios
            .entry(monitor_index)
            .or_insert_with(|| vec![0.5; count.max(10)]);

        if ratios.len() < count {
            ratios.resize(count.max(10), 0.5);
        }

        let positions: Vec<usize> = self
            .workspace_entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.monitor_index == monitor_index)
            .map(|(pos, _)| pos)
            .collect();
        let mut sorted_positions = positions.clone();
        sorted_positions.sort_by_key(|&pos| self.workspace_entries[pos].index);

        let (rects, nodes) = calculate_fibonacci_layout_with_ratios(w, h, count, self.swap, ratios);

        // Store nodes for later resize calculations
        self.split_nodes.insert(monitor_index, nodes);

        for (i, &vec_pos) in sorted_positions.iter().enumerate() {
            if let Some(rect) = rects.get(i) {
                let size = AppSize {
                    width: rect.width,
                    height: rect.height,
                };
                let pos = AppPosition {
                    x: rect.x,
                    y: rect.y + GLOBAL_Y,
                };
                self.workspace_entries[vec_pos].app.move_resize(size, pos);
            }
        }

        Some(())
    }
    // Add this method to find window index by HWND
    pub fn find_window_index_by_hwnd(&self, monitor_index: usize, hwnd: isize) -> Option<usize> {
        self.workspace_entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.monitor_index == monitor_index)
            .find(|(_, entry)| entry.app.hwnd == hwnd)
            .map(|(_, entry)| entry.index)
    }

    pub fn find_window_vec_pos_by_hwnd(&self, hwnd: isize) -> Option<usize> {
        self.workspace_entries
            .iter()
            .position(|entry| entry.app.hwnd == hwnd)
    }

    pub fn handle_resize_by_hwnd(
        &mut self,
        monitor_index: usize,
        hwnd: isize,
        new_width: i32,
        new_height: i32,
    ) -> Option<()> {
        let window_index = self.find_window_index_by_hwnd(monitor_index, hwnd)?;
        self.handle_resize(monitor_index, window_index, new_width, new_height)
    }
}
