use anyhow::bail;
use tsck_window::overlay::{
    config::{CycleDirection, NtekConfig},
    manager::OverlayManager,
};

use std::{io::Write, sync::Arc, thread};

fn main() -> anyhow::Result<()> {
    let conf_file = include_str!("../../config.ntek");
    let config = Arc::new(ntek::from_str::<NtekConfig>(conf_file).expect("Failed to parse config"));
    let manager = Arc::new(OverlayManager::new(config.clone()));
    spawn_command_interface(manager.clone());
    loop {
        thread::park();
    }
}

fn help_command_interface() {
    println!(
        r#"


    "#
    );
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
                            handler.cycle_workspace(direction);
                        });
                    }
                }
                _ => {}
            }
        }

        Ok(())
    });
}
