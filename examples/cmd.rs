use std::{io::Write, thread};

use tsck_window::{MutexWinHookHandler, with_handler};

fn print_command() {
    println!(
        r#"
list       : list all apps
find       : find app by its name
move       : move window to coordinate move=100,100 relative to its current position
monitor    : get monitor infos
cycle      : cycle size
"#
    );
}
fn spawn_cmd(handler: MutexWinHookHandler) {
    thread::spawn(move || {
        loop {
            std::io::stdout().flush().expect("Error flushing");
            let mut input = String::new();
            std::io::stdin()
                .read_line(&mut input)
                .expect("Error reading line");
            let input = input.trim();
            match input {
                "list" => {
                    with_handler!(handler, |hd| {
                        let apps = hd.get_all_apps();
                        println!("APP COUNT: {}", apps.len());
                        for app in apps.iter() {
                            println!("{:<20}{:<20}{:?}", app.exe, app.hwnd, app.position);
                        }
                    });
                }
                "monitor" => {
                    with_handler!(handler, |hd| {
                        hd.get_monitors();
                    });
                }
                c if c.starts_with("cycle") => {
                    let result = c.split_whitespace().last().and_then(|c| {
                        c.split_once('=').and_then(|(app, args)| {
                            with_handler!(handler, |hd| {
                                println!("YO {}", app);
                                // hd.cycle_app_sizepos_by_name(app);
                            });
                            Some(true)
                        })
                    });
                    if result.is_none() {
                        print_command();
                    }
                }
                c if c.starts_with("move") => {
                    let result = c.split_whitespace().last().and_then(|c| {
                        c.split_once('=').and_then(|(app, coords)| {
                            with_handler!(handler, |hd| {
                                coords.split_once(',').and_then(|(x, y)| -> Option<bool> {
                                    let x = x.parse::<i32>().ok()?;
                                    let y = y.parse::<i32>().ok()?;
                                    let app = hd.get_app_by_name(app)?;
                                    // app.move_to(tsck_window::AppPosition {
                                    //     x: app.position.x + x,
                                    //     y: app.position.y + y,
                                    // });
                                    Some(true)
                                });
                            });
                            Some(true)
                        })
                    });
                    if result.is_none() {
                        print_command();
                    }
                }
                c if c.starts_with("find") => {
                    if let Some(app) = c.split_whitespace().last() {
                        println!("{:#?}", app);
                        with_handler!(handler, |hd| {
                            if let Some(app) = hd.get_app_by_name(app) {
                                println!("{:#?}", app);
                            }
                        });
                    }
                }
                _ => {
                    print_command();
                }
            }
        }
    });
}
fn main() {}
