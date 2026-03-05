use windows::Win32::Graphics::{
    Direct2D::Common::D2D1_COLOR_F,
    DirectWrite::{
        DWRITE_FONT_STYLE, DWRITE_FONT_STYLE_ITALIC, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
        DWRITE_FONT_WEIGHT_BLACK, DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL,
    },
};

use crate::col;

#[derive(Clone, Debug)]
pub struct SlotMultiLine {
    pub lines: Vec<String>,
    pub padding: f32,
    pub line_height: f32,
    pub x: f32,
    pub y: f32,
    pub font: StatusBarFont,
    pub fg: D2D1_COLOR_F,
    pub bg: D2D1_COLOR_F,
}

#[derive(Clone, Debug)]
pub struct SlotText {
    pub text: String,
    pub fg: D2D1_COLOR_F,
    pub bg: D2D1_COLOR_F,
    pub font: StatusBarFont,
    pub font_weight: DWRITE_FONT_WEIGHT,
    pub font_style: DWRITE_FONT_STYLE,
}
impl SlotText {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            fg: col!(base_content),
            bg: col!(transparent),
            font: StatusBarFont::default(),
            font_weight: DWRITE_FONT_WEIGHT_NORMAL,
            font_style: DWRITE_FONT_STYLE_NORMAL,
        }
    }
    pub fn bold(mut self) -> Self {
        self.font_weight = DWRITE_FONT_WEIGHT_BOLD;
        self
    }
    pub fn black(mut self) -> Self {
        self.font_weight = DWRITE_FONT_WEIGHT_BLACK;
        self
    }
    pub fn italic(mut self) -> Self {
        self.font_style = DWRITE_FONT_STYLE_ITALIC;
        self
    }
    pub fn fg(mut self, fg: D2D1_COLOR_F) -> Self {
        self.fg = fg;
        self
    }
    pub fn bg(mut self, bg: D2D1_COLOR_F) -> Self {
        self.bg = bg;
        self
    }
    pub fn set_font(mut self, family: String, size: f32) -> Self {
        self.font = StatusBarFont { family, size };
        self
    }
}

#[derive(Clone, Debug)]
pub struct StatusBarFont {
    pub family: String,
    pub size: f32,
}

impl Default for StatusBarFont {
    fn default() -> Self {
        Self {
            family: "MartianMono NF".into(),
            size: 10.0,
        }
    }
}
