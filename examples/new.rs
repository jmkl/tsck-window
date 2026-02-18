use ntek::{self, Serialize};
use ntek_derive::{NtekDes, NtekSer};
use std::{collections::HashMap, io::Write, process};
use tsck_kee::{Event, Kee, TKeePair};
use tsck_window::{
    hook::{ArcMutWHookHandler, WindowHook},
    with_handler,
};

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
    MoveActiveWindow(Direction),
    ResizeActiveWindow(Direction),
    Debug,
    CycleColumn,
    CycleWindowSizePosition,
    CycleActiveApp(CycleDirection),
    CycleWindowWidth(CycleDirection),
    CycleWindowHeight(CycleDirection),
    MoveToWorkspace(CycleDirection),
    ActivateWorkspace(CycleDirection),
}

#[derive(Debug, NtekDes, NtekSer)]
enum SomeFunc {
    W(WF),
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
    workspaces: Vec<String>,
    move_inc: i32,
    size_inc: i32,
    hotkeys: HashMap<String, SomeFunc>,
    blacklist: Vec<String>,
}

impl WF {
    fn do_stuff(&self, handler: ArcMutWHookHandler, conf: &NtekConfig) {
        match self {
            WF::MoveActiveWindow(direction) => {
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
            WF::ResizeActiveWindow(direction) => {
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
            WF::Debug => {
                if let Err(err) = handler.lock().get_next() {
                    eprintln!("Fuck {err}");
                }
            }
            WF::CycleWindowSizePosition => {
                let grid = conf
                    .workspace_grid
                    .iter()
                    .map(|fs| (fs.x, fs.y, fs.width, fs.height))
                    .collect::<Vec<_>>();
                handler.lock().cycle_position(grid);
            }
            WF::CycleWindowHeight(direction) => {
                handler.lock().cycle_window_height(direction.as_str());
            }
            WF::CycleWindowWidth(direction) => {
                handler.lock().cycle_window_width(direction.as_str());
            }
            WF::CycleColumn => {
                handler.lock().cycle_column();
            }
            WF::CycleActiveApp(direction) => {
                if let Err(err) = handler.lock().cycle_active_app(direction.as_str()) {
                    eprintln!("Error on cycle active app {err}")
                }
            }
            WF::MoveToWorkspace(direction) => {
                with_handler!(handler, |hd| {
                    if let Err(err) = hd.move_active_app_to_workspace(direction.as_str()) {
                        eprintln!("Error move app to workspace {direction:?} {err}")
                    }
                });
            }
            WF::ActivateWorkspace(worskpace) => {
                with_handler!(handler, |hd| {
                    hd.activate_workspace(worskpace.as_str());
                });
            }
        }
    }
}
fn print_help() {
    println!(
        r#"
------------------------------------------------------------
quit              : quit
list              : list all app
ws create [title] : create workspace with `title`
ws list           : list all workspace
ws move 10101 3   : move app with given hwnd to workspace
ws reset          : reset y position to 0
------------------------------------------------------------
"#
    );
}
fn spawn_command_interface(handler: ArcMutWHookHandler) {
    std::thread::spawn(move || -> anyhow::Result<()> {
        loop {
            std::io::stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            match input.trim() {
                c if c.starts_with("ws") => {
                    if let Some((_, cmd)) = c.split_once(' ') {
                        let mut iter = cmd.split_whitespace();
                        if let Some(pwd) = iter.next() {
                            match pwd {
                                "create" => {
                                    iter.next().map(|title| {
                                        with_handler!(handler, |hd| {
                                            hd.create_workspace(title, 0);
                                        });
                                    });
                                }

                                "reset" => {
                                    with_handler!(handler, |hd| {
                                        _ = hd.reset_y_position();
                                    });
                                }
                                "list" => {
                                    with_handler!(handler, |hd| {
                                        for (idx, w) in hd.get_all_workspaces().iter().enumerate() {
                                            println!("{:<5}:{} {:?}", idx, w.text, w.hwnds)
                                        }
                                    });
                                }
                                _ => print_help(),
                            }
                        }
                    } else {
                        print_help();
                    }
                }
                "list" => {
                    with_handler!(handler, |hd| {
                        for (_, ai) in hd.get_all_apps() {
                            println!("{:<20}{:<10} {}", ai.exe, ai.hwnd, ai.position.y);
                        }
                    });
                }
                "quit" | "exit" => {
                    process::exit(0);
                }
                _ => print_help(),
            }
        }
    });
}
fn spawn_hotkee(handler: ArcMutWHookHandler, ntek_config: NtekConfig) {
    spawn_command_interface(handler.clone());
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
                    SomeFunc::W(workspace_func) => {
                        workspace_func.do_stuff(handler.clone(), &ntek_config);
                    }
                }
            }
        }
        _ => {}
    })
    .run(kees);
}
fn main() -> anyhow::Result<()> {
    let ntek_config =
        ntek::from_str::<NtekConfig>(include_str!("../config.ntek")).expect("failed reading shirt");
    WindowHook::new(
        ntek_config.blacklist.clone(),
        ntek_config.workspaces.clone(),
    )
    .bind(|handler| spawn_hotkee(handler, ntek_config))
    .run();
    Ok(())
}
