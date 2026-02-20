pub type Hwnd = isize;
#[derive(Clone, Debug)]
pub struct HwndItem {
    pub hwnd: Hwnd,
    pub app_name: String,
    pub monitor: usize,
    pub parked_position: Option<i32>,
}
impl HwndItem {
    pub fn new(hwnd: Hwnd, app_name: &str, monitor: usize) -> Self {
        Self {
            hwnd,
            app_name: app_name.to_string(),
            monitor,
            parked_position: None,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Workspace {
    pub text: String,
    pub active: bool,
    pub hwnds: Vec<HwndItem>,
}

impl Workspace {
    pub fn new(ws: &str, hwnds: Vec<HwndItem>) -> Self {
        Self {
            text: ws.to_string(),
            active: true,
            hwnds,
        }
    }
}
