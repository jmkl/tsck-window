use anyhow::Result;
use std::{
    fs,
    io::{self, Write, stdout},
};
use tsck_window::WindowsAppData;

fn help() {
    print!("\x1B[2J\x1B[1;1H");
    println!(
        "\x1b[33mWM Helper 0.0.1\x1b[0m
wmx [COMMAND]

[COMMAND]
  reset             : reset apps
  list              : list all active apps
"
    );
}

fn reset(apps: &Vec<WindowsAppData>) {
    for app in apps {
        println!("{}", app.name);
    }
}
fn list(apps: &Vec<WindowsAppData>) {
    for app in apps {
        println!("{}", app.name);
    }
}

fn main() -> Result<()> {
    print!("\x1B[2J\x1B[1;1H");
    let config = fs::read_to_string("log.ntek")?;
    let config = ntek::from_str::<Vec<WindowsAppData>>(&config)
        .map_err(|e| anyhow::anyhow!("Cant read file `{}`", e))?;
    stdout().flush()?;

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        help();
        return Ok(());
    }
    let arg = &args[1];
    match arg.trim() {
        "reset" => reset(&config),
        "list" => list(&config),
        _ => help(),
    }

    Ok(())
}
