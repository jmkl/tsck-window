#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Column {
    Left,
    Right,
}

#[derive(Debug, PartialEq, Clone)]
pub struct SizeRatio {
    pub width: f32,
    pub height: f32,
}
#[derive(Debug, Copy, PartialEq, Clone)]
pub struct AppPosition {
    pub x: i32,
    pub y: i32,
}
impl AppPosition {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct AppSize {
    pub width: i32,
    pub height: i32,
}
impl AppSize {
    pub fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct AppInfo {
    pub hwnd: isize,
    pub exe: String,
    pub exe_path: String,
    pub size: AppSize,
    pub position: AppPosition,
    pub title: String,
    pub class: String,
    pub column: Column,
    pub size_ratio: SizeRatio,
}
