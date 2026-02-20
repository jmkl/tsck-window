use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::{Common::*, *},
            DirectWrite::*,
            Dwm::*,
            Dxgi::Common::*,
            Gdi::*,
        },
        System::LibraryLoader::*,
        UI::{Controls::MARGINS, WindowsAndMessaging::*},
    },
    core::*,
};

use crate::overlay::{
    color::Color,
    manager::{STATUSBAR_HEIGHT, WM_UPDATE_STATUSBAR},
    monitor_info::{self, StatusbarMonitorInfo},
};

#[derive(Clone, Debug)]
pub struct SlotText {
    pub text: String,
    pub fg: D2D1_COLOR_F,
    pub bg: D2D1_COLOR_F,
    pub font_weight: DWRITE_FONT_WEIGHT,
    pub font_style: DWRITE_FONT_STYLE,
}
impl SlotText {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            fg: Color::hex(0xFFFFFF),
            bg: Color::hex(0x08000000),
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
}

#[derive(Clone, Debug)]
pub struct StatusBarFont {
    pub family: String,
    pub size: f32,
}

impl Default for StatusBarFont {
    fn default() -> Self {
        Self {
            family: "Segoe UI".into(),
            size: 13.0,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Visibility {
    Always,
    OnFocus,
    Disable,
}

#[derive(Clone, Debug)]
pub struct StatusBar {
    pub left: Vec<SlotText>,
    pub center: Vec<SlotText>,
    pub right: Vec<SlotText>,
    pub height: f32,
    pub padding: f32,
    pub always_show: Visibility,
    pub font: StatusBarFont,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self {
            left: vec![],
            center: vec![],
            right: vec![],
            height: 28.0,
            padding: 8.0,
            always_show: Visibility::Always,
            font: StatusBarFont::default(),
        }
    }
}

struct StatusbarData {
    render_target: ID2D1HwndRenderTarget,
    border_brush: ID2D1SolidColorBrush,
    dwrite_factory: IDWriteFactory,
    statusbar_format: Option<IDWriteTextFormat>,
    statusbar: Option<StatusBar>,
    is_active_monitor: bool,
    rect: (i32, i32, i32, i32),
}

pub struct StatusbarWindow {
    hwnd: HWND,
}
impl StatusbarWindow {
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn new(monitor_info: &StatusbarMonitorInfo) -> Result<Self> {
        let d2d_factory: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }?;
        let class_name = w!("StatusbarWindowYoo");
        let hinstance = unsafe { GetModuleHandleW(None) }?;
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(Self::wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }?,
            ..Default::default()
        };

        unsafe { RegisterClassExW(&wc) };
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE
                    | WS_EX_TRANSPARENT,
                class_name,
                w!("Statusbar Overlay"),
                WS_POPUP | WS_VISIBLE,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(HINSTANCE(hinstance.0)),
                None,
            )?
        };

        let monitor_rect = monitor_info::resolve_monitor_rect(hwnd, Some(monitor_info.index));
        let width = monitor_rect.right - monitor_rect.left;
        let height = STATUSBAR_HEIGHT;
        let x = monitor_rect.left;
        let y = monitor_rect.top;
        unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                width,
                height as i32,
                SWP_NOACTIVATE,
            )
        }?;

        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };

        (unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins) })?;
        (unsafe { SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA) })?;

        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 0.0,
            dpiY: 0.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd,
            pixelSize: D2D_SIZE_U {
                width: width as u32,
                height: height as u32,
            },
            presentOptions: D2D1_PRESENT_OPTIONS_IMMEDIATELY,
        };

        let render_target = unsafe { d2d_factory.CreateHwndRenderTarget(&props, &hwnd_props) }?;

        let border_color_d2d = Color::hex(0x000000);
        let border_brush = unsafe { render_target.CreateSolidColorBrush(&border_color_d2d, None) }?;
        let dwrite_factory =
            unsafe { DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED) }?;

        let statusbar_data = Box::new(StatusbarData {
            render_target,
            border_brush,
            statusbar_format: None,
            statusbar: None,
            is_active_monitor: monitor_info.is_primary,
            dwrite_factory,
            rect: (x, y, width, height as i32),
        });
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(statusbar_data) as isize) };
        _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
        _ = unsafe { UpdateWindow(hwnd) };

        Ok(Self { hwnd })
    }
    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_PAINT => {
                let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
                if ptr != 0 {
                    let data = unsafe { &*(ptr as *const StatusbarData) };

                    unsafe {
                        data.render_target.BeginDraw();
                        data.render_target.Clear(Some(&D2D1_COLOR_F {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }));
                        let mut client_rect = RECT::default();
                        let _ = GetClientRect(hwnd, &mut client_rect);
                        let screen_width = (client_rect.right - client_rect.left) as f32;

                        if let Some(ref bar) = data.statusbar {
                            match bar.always_show {
                                Visibility::Always => {
                                    _ = draw_statusbar(data, bar, screen_width);
                                }
                                Visibility::OnFocus => {
                                    if data.is_active_monitor {
                                        _ = draw_statusbar(data, bar, screen_width);
                                    }
                                }
                                Visibility::Disable => {}
                            }
                        };

                        _ = data.render_target.EndDraw(None, None);
                    }
                }
                _ = unsafe { ValidateRect(Some(hwnd), None) };
                LRESULT(0)
            }
            WM_UPDATE_STATUSBAR => {
                let ptr = wparam.0 as *mut StatusBar;
                if !ptr.is_null() {
                    unsafe {
                        let bar = Box::from_raw(ptr);
                        let data_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                        if data_ptr != 0 {
                            let data = &mut *(data_ptr as *mut StatusbarData);

                            // rebuild text format from bar font
                            let font_wide: Vec<u16> = bar
                                .font
                                .family
                                .encode_utf16()
                                .chain(std::iter::once(0))
                                .collect();
                            data.statusbar_format = data
                                .dwrite_factory
                                .CreateTextFormat(
                                    PCWSTR(font_wide.as_ptr()),
                                    None,
                                    DWRITE_FONT_WEIGHT_NORMAL,
                                    DWRITE_FONT_STYLE_NORMAL,
                                    DWRITE_FONT_STRETCH_NORMAL,
                                    bar.font.size,
                                    w!("en-us"),
                                )
                                .ok();
                            if let Some(ref fmt) = data.statusbar_format {
                                let _ = fmt.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
                                let _ =
                                    fmt.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                                let _ = fmt.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
                            }
                            data.statusbar = Some(*bar);
                        }
                        _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }

            WM_SIZE => {
                let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
                if ptr != 0 {
                    let data = unsafe { &*(ptr as *const StatusbarData) };
                    let width = (lparam.0 & 0xFFFF) as u32;
                    let height = ((lparam.0 >> 16) & 0xFFFF) as u32;
                    let _ = unsafe { data.render_target.Resize(&D2D_SIZE_U { width, height }) };
                }
                LRESULT(0)
            }
            WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
            WM_ERASEBKGND => LRESULT(1),
            WM_DESTROY => {
                // unsafe {
                //     PostQuitMessage(0);
                // }
                // return LRESULT(0);
                let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
                if ptr != 0 {
                    let _ = unsafe { Box::from_raw(ptr as *mut StatusbarData) };
                    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
                }
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }
}

unsafe fn draw_statusbar(
    data: &StatusbarData,
    bar: &StatusBar,
    screen_width: f32,
) -> anyhow::Result<()> {
    let fmt = match &data.statusbar_format {
        Some(f) => f,
        None => anyhow::bail!("Invalid format"),
    };
    let bg_rect = D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left: 0.0,
            top: 0.0,
            right: data.rect.2 as f32,
            bottom: data.rect.3 as f32,
        },
        radiusX: 0.0,
        radiusY: 0.0,
    };
    let bg_rect_fill = unsafe {
        data.render_target
            .CreateSolidColorBrush(&Color::hex(0x2f000000), None)
    }?;
    unsafe {
        data.render_target
            .FillRoundedRectangle(&bg_rect, &bg_rect_fill);
        data.render_target
            .DrawRoundedRectangle(&bg_rect, &data.border_brush, 1.0, None)
    };
    let pad = bar.padding;
    let h = bar.height;

    let measure = |s: &SlotText| -> f32 {
        let wide: Vec<u16> = s.text.encode_utf16().collect();
        measure_text_width_layout(&data.dwrite_factory, fmt, &wide) + pad * 2.0 + 2.0
    };
    let y = 0.0;
    draw_slots(data, &bar.left, 4.0, 0.0, h, pad, false, &bar.font);

    let center_total: f32 = bar.center.iter().map(|s| measure(s)).sum();
    let center_x = (screen_width - center_total) / 2.0;
    draw_slots(data, &bar.center, center_x, y, h, pad, false, &bar.font);
    draw_slots(
        data,
        &bar.right,
        screen_width - 4.0,
        y,
        h,
        pad,
        true,
        &bar.font,
    );

    Ok(())
}

fn draw_slots(
    data: &StatusbarData,
    slots: &[SlotText],
    start_x: f32,
    y: f32,
    height: f32,
    padding: f32,
    right_align: bool,
    base_font: &StatusBarFont, // add this
) {
    let gap = 2.0;

    // measure pass
    let slot_widths: Vec<f32> = slots
        .iter()
        .map(|slot| {
            let fmt = make_text_format(&data.dwrite_factory, slot, base_font).unwrap();
            let wide: Vec<u16> = slot.text.encode_utf16().collect();
            measure_text_width_layout(&data.dwrite_factory, &fmt, &wide) + padding * 1.0
        })
        .collect();

    let total_w = slot_widths.iter().sum::<f32>() + gap * slots.len().saturating_sub(1) as f32;

    let mut x = if right_align {
        start_x - total_w
    } else {
        start_x
    };

    for (slot, &sw) in slots.iter().zip(slot_widths.iter()) {
        let fmt = make_text_format(&data.dwrite_factory, slot, base_font);
        let wide: Vec<u16> = slot.text.encode_utf16().collect();
        let padding_y = 6.0;

        // background pill
        if let Ok(brush) = unsafe { data.render_target.CreateSolidColorBrush(&slot.bg, None) } {
            let bg_rect = D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left: x,
                    top: y + padding_y,
                    right: x + sw,
                    bottom: y + height - padding_y,
                },
                radiusX: 4.0,
                radiusY: 4.0,
            };
            unsafe { data.render_target.FillRoundedRectangle(&bg_rect, &brush) };
        }

        // text
        if let (Some(fmt), Ok(brush)) = (fmt.as_ref(), unsafe {
            data.render_target.CreateSolidColorBrush(&slot.fg, None)
        }) {
            let text_rect = D2D_RECT_F {
                left: x + padding,
                top: y,
                right: x + sw - padding,
                bottom: y + height,
            };
            unsafe {
                data.render_target.DrawText(
                    &wide,
                    fmt,
                    &text_rect,
                    &brush,
                    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                    DWRITE_MEASURING_MODE_NATURAL,
                )
            };
        }

        x += sw + gap;
    }
}
fn make_text_format(
    factory: &IDWriteFactory,
    slot: &SlotText,
    base: &StatusBarFont,
) -> Option<IDWriteTextFormat> {
    let font_wide: Vec<u16> = base
        .family
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let size = base.size;

    let fmt = unsafe {
        factory
            .CreateTextFormat(
                PCWSTR(font_wide.as_ptr()),
                None,
                slot.font_weight,
                slot.font_style,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("en-us"),
            )
            .ok()?
    };
    unsafe {
        let _ = fmt.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
        let _ = fmt.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
        let _ = fmt.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
    }
    Some(fmt)
}
fn measure_text_width_layout(
    factory: &IDWriteFactory,
    fmt: &IDWriteTextFormat,
    wide: &[u16],
) -> f32 {
    unsafe {
        factory
            .CreateTextLayout(wide, fmt, 10000.0, 10000.0)
            .ok()
            .and_then(|layout| {
                let mut metrics = DWRITE_TEXT_METRICS::default();
                layout.GetMetrics(&mut metrics).ok()?;
                Some(metrics.widthIncludingTrailingWhitespace)
            })
            .unwrap_or(40.0)
    }
}
