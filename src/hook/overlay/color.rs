use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

pub struct Color;
impl Color {
    pub fn hex(col: u32) -> D2D1_COLOR_F {
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
