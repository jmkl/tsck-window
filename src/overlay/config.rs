use ntek::{self, Serialize};
use ntek_derive::{NtekDes, NtekSer};
use std::collections::HashMap;

#[derive(Debug, NtekDes, NtekSer)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}
#[derive(Debug, NtekDes, NtekSer)]
pub enum CycleDirection {
    Prev,
    Next,
}
impl CycleDirection {
    fn as_str(&self) -> &str {
        match self {
            CycleDirection::Prev => "Prev",
            CycleDirection::Next => "Next",
        }
    }
}
#[derive(Debug, NtekDes, NtekSer)]
enum WF {
    MoveActiveApp(Direction),
    ResizeActiveApp(Direction),
    Debug,
    CycleColumn,
    CloseActiveApp,
    CycleAppOnGrid,
    CycleActiveApp(CycleDirection),
    CycleAppWidth(CycleDirection),
    CycleAppHeight(CycleDirection),
    MoveToWorkspace(CycleDirection),
    ActivateWorkspace(CycleDirection),
}

#[derive(Debug, NtekDes, NtekSer)]
enum SomeFunc {
    W(WF),
}
#[derive(Debug, NtekDes, NtekSer)]
pub struct WsGrid {
    pub width: f32,
    pub height: f32,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, NtekDes, NtekSer)]
pub struct NtekConfig {
    pub workspace_grid: Vec<WsGrid>,
    pub workspaces: Vec<String>,
    pub move_inc: i32,
    pub size_inc: i32,
    pub hotkeys: HashMap<String, SomeFunc>,
    pub blacklist: Vec<String>,
    pub size_factor: Vec<f32>,
}
