use tsck_window::sc::HotkeeManager;

fn main() {
    let manager = HotkeeManager::new();
    manager.event_loop();
}
