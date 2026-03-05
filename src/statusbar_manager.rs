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

use crate::{MonitorInfo, col};

pub const WM_UPDATE_STATUSBAR: u32 = WM_USER + 2;

pub const STATUSBAR_HEIGHT: f32 = 30.0;
pub fn get_statusbar_height(monitor: usize) -> f32 {
    if monitor == 0 { STATUSBAR_HEIGHT } else { 0.0 }
}

#[derive(Clone, Debug)]
pub enum DivAnchor {
    TopLeft,
    TopCenter,
    TopRight,
    MidLeft,
    MidCenter,
    MidRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

#[derive(Clone, Debug)]
pub struct SlotDiv {
    pub lines: Vec<String>,
    pub padding: f32,
    pub line_height: f32,
    pub font: StatusBarFont,
    pub fg: D2D1_COLOR_F,
    pub bg: D2D1_COLOR_F,
    pub anchor: DivAnchor,
    pub font_weight: DWRITE_FONT_WEIGHT,
    pub font_style: DWRITE_FONT_STYLE,
}

impl SlotDiv {
    pub fn new() -> Self {
        Self {
            lines: vec![],
            padding: 0.0,
            line_height: 1.0,
            font: StatusBarFont::default(),
            fg: col!(base_content),
            bg: col!(transparent),
            anchor: DivAnchor::TopLeft,
            font_weight: DWRITE_FONT_WEIGHT_NORMAL,
            font_style: DWRITE_FONT_STYLE_NORMAL,
        }
    }
    pub fn line(mut self, line: impl Into<String>) -> Self {
        self.lines.push(line.into());
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
    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }
    pub fn line_height(mut self, line_height: f32) -> Self {
        self.line_height = line_height;
        self
    }
    pub fn font_family(mut self, font_family: &str) -> Self {
        self.font.family = font_family.to_owned();
        self
    }
    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font.size = font_size;
        self
    }

    pub fn anchor(mut self, anchor: DivAnchor) -> Self {
        self.anchor = anchor;
        self
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
    pub divs: Vec<SlotDiv>,
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
            divs: vec![],
            height: 28.0,
            padding: 8.0,
            always_show: Visibility::Always,
            font: StatusBarFont::default(),
        }
    }
}

#[derive(Debug)]
struct StatusbarData {
    render_target: ID2D1HwndRenderTarget,
    border_brush: ID2D1SolidColorBrush,
    dwrite_factory: IDWriteFactory,
    statusbar_format: Option<IDWriteTextFormat>,
    statusbar: Option<StatusBar>,
    is_active_monitor: bool,
    rect: (i32, i32, i32, i32, i32),
}

pub struct StatusbarWindow {
    hwnd: HWND,
}

impl StatusbarWindow {
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }
    pub fn new(monitor_info: &MonitorInfo) -> Result<Self> {
        let d2d_factory: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }?;
        let class_name = w!("Tsck-Statusbar");
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

        let monitor_rect =
            crate::windows_api::WinAPI::resolve_monitor_rect(hwnd, Some(monitor_info.index));
        let width = monitor_rect.right - monitor_rect.left;
        let real_height = monitor_rect.bottom - monitor_rect.top;

        let height = get_statusbar_height(monitor_info.index);
        let x = monitor_rect.left;
        let y = monitor_rect.top;
        unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                width,
                real_height,
                // height as i32,
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
                height: real_height as u32,
            },
            presentOptions: D2D1_PRESENT_OPTIONS_IMMEDIATELY,
        };

        let render_target = unsafe { d2d_factory.CreateHwndRenderTarget(&props, &hwnd_props) }?;

        let border_color_d2d = col!(base_trans);
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
            rect: (x, y, width, height as i32, real_height as i32),
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
                        let screen_height = (client_rect.bottom - client_rect.top) as f32;

                        if let Some(ref bar) = data.statusbar {
                            match bar.always_show {
                                Visibility::Always => {
                                    _ = draw_widget(data, bar, screen_width, screen_height);
                                }
                                Visibility::OnFocus => {
                                    if data.is_active_monitor {
                                        _ = draw_widget(data, bar, screen_width, screen_height);
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

unsafe fn draw_widget(
    data: &StatusbarData,
    bar: &StatusBar,
    screen_width: f32,
    screen_height: f32,
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
            .CreateSolidColorBrush(&col!(base_trans), None)
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
    draw_slots(data, &bar.left, 4.0, 0.0, h, pad, false);

    let center_total: f32 = bar.center.iter().map(|s| measure(s)).sum();
    let center_x = (screen_width - center_total) / 2.0;
    draw_slots(data, &bar.center, center_x, y, h, pad, false);
    draw_slots(data, &bar.right, screen_width - 4.0, y, h, pad, true);

    if let Some(sb) = &data.statusbar {
        for mline in &sb.divs {
            draw_div_content(
                &data.render_target,
                &data.dwrite_factory,
                &mline,
                0.0,
                bar.height,
                screen_width,
                screen_height,
            )?;
        }
    }

    Ok(())
}

fn draw_div_content(
    render_target: &ID2D1HwndRenderTarget,
    dwrite_factory: &IDWriteFactory,
    div: &SlotDiv,
    x: f32,
    y: f32,
    screen_width: f32,
    screen_height: f32,
) -> anyhow::Result<()> {
    let anchor = &div.anchor;
    let lines = &div.lines;
    let padding = div.padding;
    let line_height = div.line_height;
    let font = &div.font;
    let fg = div.fg;
    let bg = div.bg;

    let font_wide: Vec<u16> = font
        .family
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let fmt = unsafe {
        dwrite_factory.CreateTextFormat(
            PCWSTR(font_wide.as_ptr()),
            None,
            div.font_weight,
            div.font_style,
            DWRITE_FONT_STRETCH_NORMAL,
            font.size,
            w!("en-us"),
        )?
    };

    let t_align = match anchor {
        DivAnchor::TopLeft | DivAnchor::MidLeft | DivAnchor::BottomLeft => {
            DWRITE_TEXT_ALIGNMENT_LEADING
        }
        DivAnchor::TopCenter | DivAnchor::MidCenter | DivAnchor::BottomCenter => {
            DWRITE_TEXT_ALIGNMENT_CENTER
        }
        DivAnchor::TopRight | DivAnchor::MidRight | DivAnchor::BottomRight => {
            DWRITE_TEXT_ALIGNMENT_TRAILING
        }
    };
    let p_align = match anchor {
        DivAnchor::TopLeft | DivAnchor::TopCenter | DivAnchor::TopRight => {
            DWRITE_PARAGRAPH_ALIGNMENT_NEAR
        }
        DivAnchor::MidLeft | DivAnchor::MidCenter | DivAnchor::MidRight => {
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER
        }
        DivAnchor::BottomLeft | DivAnchor::BottomCenter | DivAnchor::BottomRight => {
            DWRITE_PARAGRAPH_ALIGNMENT_FAR
        }
    };
    unsafe {
        fmt.SetTextAlignment(t_align)?;
        fmt.SetParagraphAlignment(p_align)?;
        fmt.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
    }

    // Measure max line width and total height
    let mut max_text_width = 0.0f32;
    let mut total_height = 0.0f32;
    let mut line_metrics: Vec<f32> = Vec::with_capacity(lines.len());

    for line in lines {
        let wide: Vec<u16> = line.encode_utf16().collect();
        let layout = unsafe { dwrite_factory.CreateTextLayout(&wide, &fmt, 10000.0, 10000.0)? };
        let mut metrics = DWRITE_TEXT_METRICS::default();
        unsafe { layout.GetMetrics(&mut metrics)? };
        max_text_width = max_text_width.max(metrics.widthIncludingTrailingWhitespace);
        let lh = metrics.height * line_height;
        total_height += lh;
        line_metrics.push(lh);
    }

    // content_width = widest line, box adds padding on both sides
    let content_width = max_text_width;
    let total_box_width = content_width + padding * 2.0;
    let total_box_height = total_height + padding; // padding/2 top + padding/2 bottom

    let (draw_x, draw_y) = match anchor {
        DivAnchor::TopLeft => (x, y),
        DivAnchor::TopCenter => (screen_width / 2.0 - total_box_width / 2.0 + x, y),
        DivAnchor::TopRight => (screen_width - total_box_width + x, y),

        DivAnchor::MidLeft => (x, screen_height / 2.0 - total_box_height / 2.0 + y),
        DivAnchor::MidCenter => (
            screen_width / 2.0 - total_box_width / 2.0 + x,
            screen_height / 2.0 - total_box_height / 2.0 + y,
        ),
        DivAnchor::MidRight => (
            screen_width - total_box_width + x,
            screen_height / 2.0 - total_box_height / 2.0 + y,
        ),

        DivAnchor::BottomLeft => (x, screen_height - total_box_height + y),
        DivAnchor::BottomCenter => (
            screen_width / 2.0 - total_box_width / 2.0 + x,
            screen_height - total_box_height + y,
        ),
        DivAnchor::BottomRight => (
            screen_width - total_box_width + x,
            screen_height - total_box_height + y,
        ),
    };

    // Draw background
    if bg.a > 0.0 {
        let bg_brush = unsafe { render_target.CreateSolidColorBrush(&bg, None)? };
        let bg_rect = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: draw_x,
                top: draw_y,
                right: draw_x + total_box_width,
                bottom: draw_y + total_box_height,
            },
            radiusX: 4.0,
            radiusY: 4.0,
        };
        unsafe { render_target.FillRoundedRectangle(&bg_rect, &bg_brush) };
    }

    // Draw text lines, inset by padding
    let text_brush = unsafe { render_target.CreateSolidColorBrush(&fg, None)? };
    let mut current_y = draw_y + padding / 2.0;

    for (line, &lh) in lines.iter().zip(line_metrics.iter()) {
        let wide: Vec<u16> = line.encode_utf16().collect();
        // layout width = content_width so TRAILING alignment has correct bounds
        let layout = unsafe { dwrite_factory.CreateTextLayout(&wide, &fmt, content_width, lh)? };

        let text_rect = D2D_RECT_F {
            left: draw_x + padding,
            top: current_y,
            right: draw_x + padding + content_width,
            bottom: current_y + lh,
        };

        unsafe {
            render_target.DrawText(
                &wide,
                &fmt,
                &text_rect,
                &text_brush,
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                DWRITE_MEASURING_MODE_NATURAL,
            )
        };

        current_y += lh;
    }

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
) {
    let gap = 2.0;

    // measure pass
    let slot_widths: Vec<f32> = slots
        .iter()
        .map(|slot| {
            let fmt = make_text_format(&data.dwrite_factory, slot).unwrap();
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
        let fmt = make_text_format(&data.dwrite_factory, slot);
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
fn make_text_format(factory: &IDWriteFactory, slot: &SlotText) -> Option<IDWriteTextFormat> {
    let font_wide: Vec<u16> = slot
        .font
        .family
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let size = slot.font.size;

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
