use ntek::{self, Serialize};
use ntek_derive::{NtekDes, NtekSer};
use std::collections::HashMap;
use tsck_kee::{Event, Kee, TKeePair};
use tsck_window::hook::{ArcMutWHookHandler, WindowHook};

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
enum WorkspaceFunc {
    MoveActiveWindow(Direction),
    ResizeActiveWindow(Direction),
    Debug,
    CycleColumn,
    CycleWindowSizePosition,
    CycleWindowWidth(CycleDirection),
    CycleWindowHeight(CycleDirection),
}
#[derive(Debug, NtekDes, NtekSer)]
enum SomeFunc {
    Workspace(WorkspaceFunc),
}
#[derive(Debug, NtekDes, NtekSer)]
struct WsGrid {
    width: f32,
    height: f32,
    x: f32,
    y: f32,
}
#[derive(Debug, NtekDes, NtekSer)]
struct NtekConfig {
    workspace_apps: Vec<Vec<String>>,
    workspace_grid: Vec<WsGrid>,
    move_inc: i32,
    size_inc: i32,
    hotkeys: HashMap<String, SomeFunc>,
}

macro_rules! handler {
    ($handler:expr, |$hd:ident| $block:block) => {
        let mut $hd = $handler.lock();
        $block
    };
}

impl WorkspaceFunc {
    fn do_stuff(&self, handler: ArcMutWHookHandler, conf: &NtekConfig) {
        match self {
            WorkspaceFunc::MoveActiveWindow(direction) => {
                let mut hd = handler.lock();
                {
                    let inc = conf.move_inc;
                    if let Some((x, y)) = hd.get_active_app_position() {
                        let (xx, yy) = match direction {
                            Direction::Up => (x, y - inc),
                            Direction::Down => (x, y + inc),
                            Direction::Left => (x - inc, y),
                            Direction::Right => (x + inc, y),
                        };
                        hd.set_position(xx, yy);
                    }
                };
            }
            WorkspaceFunc::ResizeActiveWindow(direction) => {
                let mut hd = handler.lock();
                let inc = conf.size_inc;
                let (w, h) = match direction {
                    Direction::Up => (0, -inc),
                    Direction::Down => (0, inc),
                    Direction::Left => (-inc, 0),
                    Direction::Right => (inc, 0),
                };
                hd.set_size(w, h);
            }
            WorkspaceFunc::Debug => {}
            WorkspaceFunc::CycleWindowSizePosition => {
                let grid = conf
                    .workspace_grid
                    .iter()
                    .map(|fs| (fs.x, fs.y, fs.width, fs.height))
                    .collect::<Vec<_>>();
                handler.lock().cycle_position(grid);
            }
            WorkspaceFunc::CycleWindowHeight(direction) => {
                handler.lock().cycle_window_height(direction.as_str());
            }
            WorkspaceFunc::CycleWindowWidth(direction) => {
                handler.lock().cycle_window_width(direction.as_str());
            }
            WorkspaceFunc::CycleColumn => {
                handler.lock().cycle_column();
            }
        }
    }
}
fn spawn_hotkee(handler: ArcMutWHookHandler) {
    let ntek_config = ntek::from_str::<NtekConfig>(include_str!("../config.ntek"))
        .expect("Failed to parse Setting");
    let mut kee = Kee::new(false);
    let kees: Vec<TKeePair> = ntek_config
        .hotkeys
        .iter()
        .map(|(k, f)| TKeePair::new(k.to_string(), f.serialize()))
        .collect::<Vec<_>>();
    kee.on_message(move |event| match event {
        Event::Keys(k, _) => {
            if let Some(somefunc) = ntek_config.hotkeys.get(k) {
                match somefunc {
                    SomeFunc::Workspace(workspace_func) => {
                        workspace_func.do_stuff(handler.clone(), &ntek_config);
                    }
                }
            }
        }
        _ => {}
    })
    .run(kees);
}
fn main() {
    WindowHook::new().bind(spawn_hotkee).run();
}
