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
pub enum FloatingFunction {
    ResizeFloatingW(i32),
    ResizeFloatingH(i32),
    TransformFloatingX(i32),
    TransformFloatingY(i32),
    CycleFloatingApp(Direction),
    SwapFocus,
    Null,
}

#[derive(Debug, NtekDes, NtekSer)]
pub enum AppFunction {
    Null,
    Debug,
    CycleSizeFactor,
    ToggleFloating,
    SwitchMonitor,
    CloseApp,
    SwapFocus,
    CycleWorkspace(Direction),
    MoveApp(Direction),
    CycleFocusApp(Direction),
    MoveAppToWorkspace(Direction),
    AdjustAppWidth(i32),
}

#[derive(Debug, NtekDes, NtekSer)]
pub enum AppFunc {
    Func(AppFunction),
    Hybrid(AppFunction, FloatingFunction),
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
    let kees: Vec<TKeePair> = ntek
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
                    AppFunc::Hybrid(app_function, floating_state) => {
                        let is_floating = ctx.lock().is_floating_mode();
                        if is_floating {
                            if let Err(err) = floating_state.pipe(ctx.clone(), ntek.clone()) {
                                log_error!(
                                    "Error while executing AppFunc::Func",
                                    dp!(app_function),
                                    err
                                );
                            }
                        } else {
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
        }
        tsck_kee::Event::Shutdown => {}
        tsck_kee::Event::Modifier(modifier, state) => {
            ctx.clone().lock().on_modifier_pressed(modifier, state);
        }
    })
    .run(kees);
}
impl FloatingFunction {
    pub fn pipe(&self, ctx: Shared<AppContext>, _: Arc<WinNtek>) -> anyhow::Result<()> {
        match self {
            FloatingFunction::ResizeFloatingW(value) => {
                ctx.lock().resize_width(*value)?;
            }
            FloatingFunction::ResizeFloatingH(value) => {
                ctx.lock().resize_height(*value)?;
            }
            FloatingFunction::TransformFloatingX(value) => {
                ctx.lock().transform_x(*value)?;
            }
            FloatingFunction::TransformFloatingY(value) => {
                ctx.lock().transform_y(*value)?;
            }
            FloatingFunction::CycleFloatingApp(direction) => {
                ctx.lock().cycle_floating_app(direction)?;
            }
            FloatingFunction::Null => {}
            FloatingFunction::SwapFocus => {
                ctx.lock().swap_focus()?;
            }
        }
        Ok(())
    }
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

            AppFunction::MoveAppToWorkspace(direction) => {
                ctx.lock().move_app_to_workspace(direction);
            }
            AppFunction::MoveApp(direction) => {
                ctx.lock().move_app(direction)?;
            }
            AppFunction::CycleFocusApp(direction) => {
                ctx.lock().cycle_focus_app(direction)?;
            }
            AppFunction::ToggleFloating => {
                ctx.lock().toggle_floating()?;
            }
            AppFunction::CloseApp => {
                crate::log_warn!("AppFunction::CloseApp");
            }
            AppFunction::AdjustAppWidth(value) => {
                crate::log_warn!("AppFunction::AdjustAppWidth", value)
            }
            AppFunction::SwitchMonitor => {
                ctx.lock().switch_monitor()?;
                crate::log_warn!("AppFunction::AdjustAppWidth")
            }
            AppFunction::Null => {}

            AppFunction::SwapFocus => {
                ctx.lock().swap_focus()?;
            }
        }
        Ok(())
    }
}
