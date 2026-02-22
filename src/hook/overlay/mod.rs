mod app_border;
mod color;
mod manager;
mod monitor_info;
mod statusbar;

#[cfg(test)]
mod tests {
    use crate::hook::{
        SystemInfo, format_speed,
        overlay::{
            color::Color,
            manager::{OverlayManager, STATUSBAR_HEIGHT},
            statusbar::{SlotText, StatusBar, StatusBarFont, Visibility},
        },
    };
    use std::sync::Arc;

    #[test]
    fn test_statusbar() {
        let overlay = Arc::new(OverlayManager::new());
        let o = overlay.clone();

        std::thread::spawn(move || {
            let mut info = SystemInfo::new();
            loop {
                let time = chrono::Local::now().format("%H:%M %a, %d %h").to_string();
                let usage = info.update();
                let bg = Color::str("#9aa1f4");
                let fg = Color::str("#191919");
                _ = o.update_statusbar(
                    0,
                    StatusBar {
                        left: vec![],
                        center: vec![SlotText::new(format!("{}", time)).fg(fg).bg(bg).black()],
                        right: vec![
                            SlotText::new(" ").fg(fg).bg(bg),
                            SlotText::new(format!(
                                "↓{} ↑{}",
                                format_speed(usage.net_download),
                                format_speed(usage.net_upload)
                            )),
                            SlotText::new("").fg(fg).bg(bg),
                            SlotText::new(format!("{:.1}%", usage.cpu_percent)),
                            SlotText::new("󰍛").fg(fg).bg(bg),
                            SlotText::new(format!(
                                "{:.1}/{:.1} GB",
                                usage.ram_used_gb, usage.ram_total_gb
                            )),
                        ],
                        height: STATUSBAR_HEIGHT,
                        padding: 10.0,
                        always_show: Visibility::OnFocus,
                        font: StatusBarFont {
                            family: "MartianMono NF".into(),
                            size: 10.0,
                        },
                    },
                );
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        });

        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    }
}
