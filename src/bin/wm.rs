use tsck_window::{WindowsManager, deadlock_detector};

fn main() {
    deadlock_detector();
    let manager = WindowsManager::new();
    manager.event_loop();
}
