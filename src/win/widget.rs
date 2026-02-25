use std::collections::BTreeMap;

use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::WindowsAndMessaging::PostMessageW,
};

use crate::{
    col,
    win::{
        statusbar::{SlotText, StatusBar, StatusBarFont, Visibility, WM_UPDATE_STATUSBAR},
        winapi::STATUSBAR_HEIGHT,
    },
};

pub type Hwnd = isize;
#[derive(Clone, Debug)]
pub struct HwndItem {
    pub hwnd: Hwnd,
    pub app_name: String,
    pub monitor: usize,
    pub floating: bool,
    pub parked_position: Option<i32>,
}
impl HwndItem {
    pub fn new(hwnd: Hwnd, app_name: &str, monitor: usize) -> Self {
        Self {
            hwnd,
            app_name: app_name.to_string(),
            monitor,
            floating: false,
            parked_position: None,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Workspace {
    pub text: String,
    pub active: bool,
    pub hwnds: Vec<HwndItem>,
}

impl Workspace {
    pub fn new(ws: &str, hwnds: Vec<HwndItem>) -> Self {
        Self {
            text: ws.to_string(),
            active: true,
            hwnds,
        }
    }
}

pub enum WsIndicatorPos {
    Left,
    Center,
    Right,
    None,
}
pub enum SlotGrid {
    Left,
    Center,
    Right,
}
pub struct WidgetSlots {
    pub left: BTreeMap<String, Vec<SlotText>>,
    pub center: BTreeMap<String, Vec<SlotText>>,
    pub right: BTreeMap<String, Vec<SlotText>>,
    pub workspace_indicator: WsIndicatorPos,
    pub hwnd: Option<isize>,
    pub workspaces: Vec<Workspace>,
    pub active_workspace_per_monitor: Vec<usize>,
}

impl Default for WidgetSlots {
    fn default() -> Self {
        Self {
            left: BTreeMap::new(),
            center: BTreeMap::new(),
            right: BTreeMap::new(),
            workspace_indicator: WsIndicatorPos::Center,
            hwnd: None,
            workspaces: vec![],
            active_workspace_per_monitor: vec![0; 2],
        }
    }
}

impl WidgetSlots {
    pub fn get_workspaces(&mut self) -> &mut Vec<Workspace> {
        &mut self.workspaces
    }
    pub fn get_active_workspace_for_monitor(&self, monitor: usize) -> usize {
        self.active_workspace_per_monitor
            .get(monitor)
            .copied()
            .unwrap_or(0)
    }
    pub fn set_hwnd(&mut self, hwnd: Option<isize>) {
        self.hwnd = hwnd;
    }
    pub fn set_slot(&mut self, grid: SlotGrid, key: &str, slots: Vec<SlotText>) {
        {
            match grid {
                SlotGrid::Left => {
                    self.left.insert(key.into(), slots);
                }
                SlotGrid::Center => {
                    self.center.insert(key.into(), slots);
                }
                SlotGrid::Right => {
                    self.right.insert(key.into(), slots);
                }
            }
        }

        self.refresh_statusbar();
    }
    fn update_statusbar(&self, target_hwnd: isize, statusbar: StatusBar) -> anyhow::Result<()> {
        let hwnd = HWND(target_hwnd as *mut std::ffi::c_void);
        unsafe {
            PostMessageW(
                Some(hwnd),
                WM_UPDATE_STATUSBAR,
                WPARAM(Box::into_raw(Box::new(statusbar)) as usize),
                LPARAM(0),
            )?;
        }
        Ok(())
    }
    fn get_workspace_indicator(
        &self,
        workspaces: &Vec<Workspace>,
        active_workspace_per_monitor: &Vec<usize>,
        monitor_index: usize,
    ) -> Vec<SlotText> {
        let active = active_workspace_per_monitor
            .get(monitor_index)
            .copied()
            .unwrap_or(0);
        workspaces
            .iter()
            .enumerate()
            .map(|(idx, ws)| {
                let has_apps = ws.hwnds.iter().any(|h| h.monitor == monitor_index);
                SlotText::new(format!("{}", ws.text))
                    .fg(if has_apps && active == idx {
                        col!(primary_content)
                    } else {
                        if active == idx {
                            col!(base_300)
                        } else {
                            col!(dim_content)
                        }
                    })
                    .bg({
                        if active == idx {
                            col!(success)
                        } else {
                            col!(base_300)
                        }
                    })
                    .bold()
            })
            .collect()
    }
    pub fn refresh_statusbar(&mut self) {
        let ws =
            self.get_workspace_indicator(&self.workspaces, &self.active_workspace_per_monitor, 0);
        let mut left = self.left.values().flatten().cloned().collect();
        let mut center = self.center.values().flatten().cloned().collect();
        let mut right = self.right.values().flatten().cloned().collect();
        match self.workspace_indicator {
            WsIndicatorPos::Left => {
                let mut m = ws;
                m.extend(left);
                left = m;
            }
            WsIndicatorPos::Center => {
                let mut m = ws;
                m.extend(center);
                center = m;
            }
            WsIndicatorPos::Right => {
                let mut m = ws;
                m.extend(right);
                right = m;
            }
            WsIndicatorPos::None => {}
        }
        let statusbar = StatusBar {
            left,
            center,
            right,
            height: STATUSBAR_HEIGHT,
            padding: 10.0,
            always_show: Visibility::OnFocus,
            font: StatusBarFont {
                family: "MartianMono NF".into(),
                size: 10.0,
            },
        };
        if let Some(raw) = self.hwnd {
            _ = self.update_statusbar(raw, statusbar);
        }
    }
}
