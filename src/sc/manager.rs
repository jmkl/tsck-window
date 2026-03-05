pub struct HotkeeManager;
impl HotkeeManager {
    pub fn new() -> Self {
        Self {}
    }
    pub fn event_loop(&self) {
        loop {
            std::thread::park();
        }
    }
}
