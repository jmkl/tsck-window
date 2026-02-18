use anyhow::Result;
use ntek::{self, Serialize};
use ntek_derive::{NtekDes, NtekSer};
use std::collections::HashMap;
use tsck_kee::{Event, Kee, TKeePair};
use tsck_window::{AppPosition, AppSize, MutexWinHookHandler, api::WinHook, with_handler};

#[derive(Debug, NtekDes, NtekSer)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, NtekDes, NtekSer)]
enum WorkspaceFunc {
    MoveActiveWindow(Direction),
    ResizeActiveWindow(Direction),
    Debug,
    CycleWorkSpace,
}
impl WorkspaceFunc {
    fn do_stuff(&self, handler: MutexWinHookHandler, conf: &NtekConfig) {
        match self {
            WorkspaceFunc::MoveActiveWindow(direction) => {
                with_handler!(handler, |hd| {
                    let app = { hd.get_active_app_position() };
                    if let Some(app) = app {
                        let value = match direction {
                            Direction::Up => AppPosition {
                                x: app.x,
                                y: app.y - conf.move_inc,
                            },
                            Direction::Down => AppPosition {
                                x: app.x,
                                y: app.y + conf.move_inc,
                            },
                            Direction::Left => AppPosition {
                                x: app.x - conf.move_inc,
                                y: app.y,
                            },
                            Direction::Right => AppPosition {
                                x: app.x + conf.move_inc,
                                y: app.y,
                            },
                        };
                        hd.move_active_app(value);
                    }
                });
            }
            WorkspaceFunc::ResizeActiveWindow(direction) => {
                with_handler!(handler, |hd| {
                    let app = { hd.get_active_app_size() };
                    if let Some(app) = app {
                        let value = match direction {
                            Direction::Up => AppSize {
                                width: app.width,
                                height: app.height - conf.size_inc,
                            },
                            Direction::Down => AppSize {
                                width: app.width,
                                height: app.height + conf.size_inc,
                            },
                            Direction::Left => AppSize {
                                width: app.width - conf.size_inc,
                                height: app.height,
                            },
                            Direction::Right => AppSize {
                                width: app.width + conf.size_inc,
                                height: app.height,
                            },
                        };
                        hd.resize_active_app(value);
                    }
                });
            }
            WorkspaceFunc::Debug => {
                with_handler!(handler, |hd| {
                    println!(
                        "{:#?}",
                        hd.get_workspace_entries(0)
                            .iter()
                            .map(|f| f.app.exe.clone())
                            .collect::<Vec<_>>()
                    );
                    hd.reindex_by_monitor();
                });
            }
            WorkspaceFunc::CycleWorkSpace => {
                with_handler!(handler, |hd| {
                    _ = hd.swap_at_positions(0, 0, 2);
                    hd.swap_layout();
                });
            }
        }
    }
}

#[derive(Debug, NtekDes, NtekSer)]
enum SomeFunc {
    Workspace(WorkspaceFunc),
}

#[derive(Debug, NtekDes, NtekSer)]
struct NtekConfig {
    workspace_apps: Vec<Vec<String>>,
    move_inc: i32,
    size_inc: i32,
    hotkeys: HashMap<String, SomeFunc>,
}

fn spawn_hotkey(handler: MutexWinHookHandler) -> anyhow::Result<()> {
    let ntek =
        ntek::from_str::<NtekConfig>(include_str!("../config.ntek")).expect("Failed to parse");

    let mut kee = Kee::new(false);
    let kees: Vec<TKeePair> = ntek
        .hotkeys
        .iter()
        .map(|(k, f)| TKeePair::new(k.to_string(), f.serialize()))
        .collect::<Vec<_>>();
    kee.on_message(move |event| match event {
        Event::Keys(k, _) => {
            if let Some(somefunc) = ntek.hotkeys.get(k) {
                match somefunc {
                    SomeFunc::Workspace(workspace_func) => {
                        workspace_func.do_stuff(handler.clone(), &ntek);
                    }
                }
            }
        }
        _ => {}
    })
    .run(kees);

    Ok(())
}

fn main() -> anyhow::Result<()> {
    WinHook::new()
        .bind(|handler| {
            _ = spawn_hotkey(handler);
        })
        .run();
    Ok(())
}
