use anyhow::bail;
use ntek::Serialize;
use tsck_kee::{Event, Kee, TKeePair};
use tsck_window::overlay::{
    config::{CycleDirection, NtekConfig, SomeFunc},
    manager::OverlayManager,
};

use std::{io::Write, sync::Arc, thread};

fn main() -> anyhow::Result<()> {
    let conf_file = include_str!("../../config.ntek");
    let config = Arc::new(ntek::from_str::<NtekConfig>(conf_file).expect("Failed to parse config"));
    let manager = Arc::new(OverlayManager::new(config.clone()));
    spawn_command_interface(manager.clone());
    spawn_hotkey(manager.clone(), config.clone());
    loop {
        thread::park();
    }
}

fn help_command_interface() {
    println!(
        r#"
ws prev
ws next
ws reset
ws list
list
app move up
app move down
app move left
app move right
app resize

    "#
    );
}
fn spawn_hotkey(manager: Arc<OverlayManager>, config: Arc<NtekConfig>) {
    let mut kee = Kee::new(false);
    let kees = config
        .hotkeys
        .iter()
        .map(|(k, f)| TKeePair::new(k.to_string(), f.serialize()))
        .collect::<Vec<_>>();
    kee.on_message(move |event| match event {
        Event::Keys(k, _) => {
            if let Some(somefunc) = config.hotkeys.get(k) {
                match somefunc {
                    SomeFunc::W(ws_func) => {
                        ws_func.do_stuff(manager.clone(), config.clone());
                    }
                }
            }
        }
        _ => {}
    })
    .run(kees);
}
fn spawn_command_interface(manager: Arc<OverlayManager>) {
    let manager = manager.clone();
    thread::spawn(move || -> anyhow::Result<()> {
        loop {
            std::io::stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            match input.trim() {
                c if c.starts_with("ws") => {
                    if let Some((_, s)) = c.split_once(' ') {
                        let direction = match s {
                            "prev" => CycleDirection::Prev,
                            "next" => CycleDirection::Next,
                            _ => CycleDirection::Next,
                        };
                        manager.with_handler(|handler| {
                            handler.go_to_workspace(&direction);
                        });
                    }
                }
                "order" => manager.with_handler(|h| {
                    h.arrange_workspaces();
                }),
                "reset" => manager.with_handler(|h| {
                    h.reset_position();
                }),

                "list" => {
                    manager.with_handler(|handler| {
                        handler.apps.iter().for_each(|(_, ai)| {
                            println!("{:<10} {:?} {:?}", ai.exe, ai.position, ai.size);
                        });
                    });
                }
                _ => {
                    help_command_interface();
                }
            }
        }
    });
}
