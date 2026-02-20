use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

pub struct Theme;
impl Theme {
    pub const FG: u32 = 0xcad3f5;
    pub const BG: u32 = 0x1e2030;
    pub const DARK_FG: u32 = 0x1e2030;
    pub const PRIMARY: u32 = 0xc6a0f6;
    pub const SUCCESS: u32 = 0xa6da95;
    pub const WARNING: u32 = 0xeed49f;
    pub const DANGER: u32 = 0xed8796;
    pub const DIM_BG: u32 = 0x08181926;
    pub const DIM_FG: u32 = 0x494d64;
}

pub const FG: D2D1_COLOR_F = Color::hex(0xcad3f5);
pub const BG: D2D1_COLOR_F = Color::hex(0x1e2030);
pub const DARK_FG: D2D1_COLOR_F = Color::hex(0x1e2030);
pub const PRIMARY: D2D1_COLOR_F = Color::hex(0xc6a0f6);
pub const SUCCESS: D2D1_COLOR_F = Color::hex(0xa6da95);
pub const WARNING: D2D1_COLOR_F = Color::hex(0xeed49f);
pub const DANGER: D2D1_COLOR_F = Color::hex(0xed8796);
pub const DIM_BG: D2D1_COLOR_F = Color::hex(0x08181926);
pub const DIM_FG: D2D1_COLOR_F = Color::hex(0x494d64);

pub struct Color;
impl Color {
    pub const fn hex(col: u32) -> D2D1_COLOR_F {
        let a = ((col >> 24) & 0xFF) as f32 / 255.0;
        D2D1_COLOR_F {
            r: ((col >> 16) & 0xFF) as f32 / 255.0,
            g: ((col >> 8) & 0xFF) as f32 / 255.0,
            b: (col & 0xFF) as f32 / 255.0,
            a: if a > 0.0 { a } else { 1.0 },
        }
    }
    pub fn str(hex: &str) -> D2D1_COLOR_F {
        let hex = hex.trim_start_matches('#');
        let value = u32::from_str_radix(hex, 16).expect("invalid color");
        match hex.len() {
            // RRGGBB
            6 => {
                let r = ((value >> 16) & 0xFF) as f32 / 255.0;
                let g = ((value >> 8) & 0xFF) as f32 / 255.0;
                let b = (value & 0xFF) as f32 / 255.0;

                D2D1_COLOR_F { r, g, b, a: 1.0 }
            }
            // RRGGBBAA
            8 => {
                let r = ((value >> 24) & 0xFF) as f32 / 255.0;
                let g = ((value >> 16) & 0xFF) as f32 / 255.0;
                let b = ((value >> 8) & 0xFF) as f32 / 255.0;
                let a = (value & 0xFF) as f32 / 255.0;

                D2D1_COLOR_F { r, g, b, a }
            }

            _ => D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.5,
            },
        }
    }
}

pub struct Clr {
    pub fg: D2D1_COLOR_F,
    pub bg: D2D1_COLOR_F,
    pub danger: D2D1_COLOR_F,
    pub primary: D2D1_COLOR_F,
    pub success: D2D1_COLOR_F,
    pub warning: D2D1_COLOR_F,
    pub dim_bg: D2D1_COLOR_F,
    pub dim_fg: D2D1_COLOR_F,
}
impl Clr {
    pub fn new() -> Self {
        Self {
            fg: Color::hex(0xcad3f5),
            bg: Color::hex(0x1e2030),
            primary: Color::hex(0xc6a0f6),
            success: Color::hex(0xa6da95),
            warning: Color::hex(0xeed49f),
            danger: Color::hex(0xed8796),
            dim_bg: Color::hex(0x08181926),
            dim_fg: Color::hex(0x494d64),
        }
    }
}
