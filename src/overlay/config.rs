use ntek::{self, Serialize};
use ntek_derive::{NtekDes, NtekSer};
use std::{collections::HashMap, sync::Arc};

use crate::overlay::manager::OverlayManager;

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
    pub fn as_str(&self) -> &str {
        match self {
            CycleDirection::Prev => "Prev",
            CycleDirection::Next => "Next",
        }
    }
}
#[derive(Debug, NtekDes, NtekSer)]
pub enum WF {
    MoveActiveApp(Direction),
    ResizeActiveApp(Direction),
    Debug,
    CycleColumn,
    CloseActiveApp,
    CycleAppOnGrid,
    ToggleTopMost,
    CycleActiveApp(CycleDirection),
    CycleAppWidth(CycleDirection),
    CycleAppHeight(CycleDirection),
    MoveToWorkspace(CycleDirection),
    GoToWorkspace(CycleDirection),
}

#[derive(Debug, NtekDes, NtekSer)]
pub enum SomeFunc {
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
impl WF {
    pub fn do_stuff(&self, handler: Arc<OverlayManager>, conf: Arc<NtekConfig>) {
        match self {
            WF::MoveActiveApp(direction) => {
                handler.with_handler(|hd| {
                    let inc = conf.move_inc;
                    // if let Some((x, y)) = hd.get_active_app_position() {
                    //     let (xx, yy) = match direction {
                    //         Direction::Up => (x, y - inc),
                    //         Direction::Down => (x, y + inc),
                    //         Direction::Left => (x - inc, y),
                    //         Direction::Right => (x + inc, y),
                    //     };
                    //     hd.set_position(xx, yy);
                    // }
                });
            }
            WF::ResizeActiveApp(direction) => {
                handler.with_handler(|hd| {
                    let inc = conf.size_inc;
                    let (w, h) = match direction {
                        Direction::Up => (0, -inc),
                        Direction::Down => (0, inc),
                        Direction::Left => (-inc, 0),
                        Direction::Right => (inc, 0),
                    };
                    // hd.set_size(w, h);
                });
            }
            WF::Debug => {}
            WF::CycleAppOnGrid => {
                let grid = conf
                    .workspace_grid
                    .iter()
                    .map(|fs| (fs.x, fs.y, fs.width, fs.height))
                    .collect::<Vec<_>>();

                handler.with_handler(|hd| {
                    hd.cycle_app_on_grid(&grid);
                });
            }
            WF::CycleAppHeight(direction) => {
                handler.with_handler(|hd| {
                    hd.cycle_app_height(direction.as_str());
                });
            }
            WF::CycleAppWidth(direction) => {
                handler.with_handler(|hd| {
                    hd.cycle_app_width(direction.as_str());
                });
            }
            WF::CycleColumn => {
                handler.with_handler(|hd| {
                    // hd.cycle_column();
                });
            }
            WF::CycleActiveApp(direction) => {
                handler.with_handler(|hd| {

                    // if let Err(err) = handler.lock().cycle_active_app(direction.as_str()) {
                    //     eprintln!("Error on cycle active app {err}")
                    // }
                });
            }
            WF::ToggleTopMost => {
                handler.with_handler(|hd| {
                    hd.toggle_top_most();
                });
            }
            WF::MoveToWorkspace(direction) => {
                handler.with_handler(|hd| {
                    if let Err(err) = hd.move_active_to_workspace(direction) {
                        eprintln!("Error move app to workspace {direction:?} {err}")
                    }
                });
            }
            WF::GoToWorkspace(worskpace) => {
                handler.with_handler(|hd| {
                    hd.go_to_workspace(worskpace);
                });
            }
            WF::CloseActiveApp => {
                handler.with_handler(|hd| {
                    hd.close_active_app();
                });
            }
        }
    }
}
