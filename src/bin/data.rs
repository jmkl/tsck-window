use tsck_window::win::manager::WinManager;
struct Rand {
    state: u64,
}

impl Rand {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x as u32
    }

    fn r#gen(&mut self, len: u32) -> u32 {
        self.next() % len
    }
}

fn main() {
    WinManager::new().event_loop();
}
