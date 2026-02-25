use std::{collections::HashMap, io::Write, sync::Arc, thread};

use ntek::Serialize;
use ntek_derive::{NtekDes, NtekSer};
use tsck_kee::{Kee, TKeePair};

use crate::{
    dp, log_error,
    win::context::{AppContext, Shared},
};

#[derive(Debug, NtekDes, NtekSer)]
pub enum Direction {
    Prev,
    Next,
}

#[derive(Debug, NtekDes, NtekSer)]
pub enum AppFunction {
    Debug,
    CycleSizeFactor,
    ToggleFloating,
    CycleWorkspace(Direction),
    MoveApp(Direction),
    FocusApp(Direction),
    CycleApp(Direction),
    MoveAppToWorkspace(Direction),
    ResizeWidth(i32),
    ResizeHeight(i32),
    TransformX(i32),
    TransformY(i32),
}

#[derive(Debug, NtekDes, NtekSer)]
pub enum AppFunc {
    Func(AppFunction),
}

#[derive(Debug, NtekDes, NtekSer)]
pub struct WinNtek {
    pub hotkeys: HashMap<String, AppFunc>,
    pub workspaces: Vec<String>,
    pub blacklist: Vec<String>,
    pub size_factor: Vec<f32>,
}

pub fn spawn_commandline(ctx: Shared<AppContext>) {
    let ctx = ctx.clone();
    thread::spawn(move || -> anyhow::Result<()> {
        loop {
            std::io::stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;

            match input.trim() {
                "list" => {
                    ctx.lock().debug_list_app()?;
                }
                "move" => {
                    ctx.lock().debug_move()?;
                }
                "reset" => {
                    ctx.lock().debug_reset()?;
                }
                "quit" => {
                    std::process::exit(0);
                }
                _ => {
                    print!("\x1B[2J\x1B[1;1H");
                    println!(
                        r#"
reset
                      "#
                    )
                }
            }
        }
    });
}
pub fn spawn_hotkee(ntek: Arc<WinNtek>, ctx: Shared<AppContext>) {
    let mut k = Kee::new(false);
    let kees = ntek
        .hotkeys
        .iter()
        .map(|(k, f)| TKeePair::new(k, f.serialize()))
        .collect();
    let ntek = ntek.clone();
    let ctx = ctx.clone();
    k.on_message(move |event| match event {
        tsck_kee::Event::Keys(k, _func) => {
            if let Some(fnc) = ntek.clone().hotkeys.get(k) {
                match fnc {
                    AppFunc::Func(app_function) => {
                        if let Err(err) = app_function.pipe(ctx.clone(), ntek.clone()) {
                            log_error!(
                                "Error while executing AppFunc::Func",
                                dp!(app_function),
                                err
                            );
                        }
                    }
                }
            }
        }
        tsck_kee::Event::Shutdown => {}
    })
    .run(kees);
}
impl AppFunction {
    pub fn pipe(&self, ctx: Shared<AppContext>, _: Arc<WinNtek>) -> anyhow::Result<()> {
        match self {
            AppFunction::Debug => {
                ctx.lock().apply_layout_in_workspace();
            }
            AppFunction::CycleSizeFactor => {
                ctx.lock().cycle_size_factor()?;
            }
            AppFunction::CycleWorkspace(direction) => {
                ctx.lock().cycle_workspace(direction);
            }
            AppFunction::CycleApp(direction) => {
                ctx.lock().cycle_app(direction);
            }
            AppFunction::MoveAppToWorkspace(direction) => {
                ctx.lock().move_app_to_workspace(direction);
            }
            AppFunction::MoveApp(direction) => {
                ctx.lock().move_app(direction)?;
            }
            AppFunction::FocusApp(direction) => {
                ctx.lock().focus_app(direction)?;
            }
            AppFunction::ToggleFloating => {
                ctx.lock().toggle_floating()?;
            }
            AppFunction::ResizeWidth(value) => {
                ctx.lock().resize_width(*value)?;
            }
            AppFunction::ResizeHeight(value) => {
                ctx.lock().resize_height(*value)?;
            }
            AppFunction::TransformX(value) => {
                ctx.lock().transform_x(*value)?;
            }
            AppFunction::TransformY(value) => {
                ctx.lock().transform_y(*value)?;
            }
        }
        Ok(())
    }
}
