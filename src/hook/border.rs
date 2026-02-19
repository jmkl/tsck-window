use parking_lot::Mutex;
use std::sync::Arc;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::{Common::*, *},
            DirectWrite::{
                DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_WEIGHT_NORMAL, DWRITE_MEASURING_MODE_NATURAL,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
                DWRITE_TEXT_METRICS, DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory,
                IDWriteFactory, IDWriteTextFormat,
            },
            Dwm::*,
            Gdi::{
                EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, InvalidateRect,
                MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow, UpdateWindow,
                ValidateRect,
            },
        },
        System::LibraryLoader::*,
        UI::{Controls::MARGINS, WindowsAndMessaging::*},
    },
    core::*,
};

use crate::hook::{api::Hwnd, win_api::get_toolbar_height};

// Custom messages
const WM_UPDATE_COLOR: u32 = WM_USER + 1;
const WM_UPDATE_THICKNESS: u32 = WM_USER + 2;
const WM_UPDATE_RADIUS: u32 = WM_USER + 3;
const WM_UPDATE_RECT_POS: u32 = WM_USER + 4;
const WM_UPDATE_RECT_SIZE: u32 = WM_USER + 5;
const WM_UPDATE_STATUSBAR: u32 = WM_USER + 6;
const WM_SET_ACTIVE_MONITOR: u32 = WM_USER + 7;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct HwndItem {
    pub hwnd: Hwnd,
    pub app_name: String,
    pub monitor: usize,
    pub parked_position: Option<i32>,
}

#[derive(Clone, Debug)]
pub struct Workspace {
    pub text: String,
    pub active: bool,
    pub hwnds: Vec<HwndItem>,
}

#[derive(Clone, Debug)]
pub struct MonitorInfo {
    pub index: usize,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub is_primary: bool,
}

#[derive(Clone, Debug)]
pub struct SlotText {
    pub text: String,
    pub foreground: u32, // 0xRRGGBB
    pub background: u32, // 0xRRGGBB, use 0 for transparent
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

// ---------------------------------------------------------------------------
// BorderManager
// ---------------------------------------------------------------------------

struct BorderState {
    hwnds: Vec<isize>,
}

pub struct BorderManager {
    state: Arc<Mutex<BorderState>>,
}

impl BorderManager {
    pub fn new() -> Self {
        let monitor_count = Self::get_monitors().len();
        Self::new_on_monitors(&(0..monitor_count).collect::<Vec<_>>())
    }

    pub fn new_on_monitors(monitor_indices: &[usize]) -> Self {
        let mut hwnds = Vec::new();
        for &index in monitor_indices {
            match unsafe { TransparentBorderWindow::new(0xAC3E31, 2.0, 5.0, Some(index)) } {
                Ok(window) => {
                    let hwnd = window.hwnd().0 as isize;
                    eprintln!("created border window for monitor {index}: hwnd={hwnd}");
                    hwnds.push(hwnd);
                    std::mem::forget(window);
                }
                Err(e) => eprintln!("Failed to create border window for monitor {index}: {e}"),
            }
        }
        Self {
            state: Arc::new(Mutex::new(BorderState { hwnds })),
        }
    }

    pub fn get_monitors() -> Vec<MonitorInfo> {
        unsafe {
            let mut monitors: Vec<MonitorInfo> = Vec::new();
            let ptr = &mut monitors as *mut Vec<MonitorInfo>;

            unsafe extern "system" fn enum_proc(
                hmonitor: HMONITOR,
                _hdc: HDC,
                _rect: *mut RECT,
                lparam: LPARAM,
            ) -> BOOL {
                let monitors = unsafe { &mut *(lparam.0 as *mut Vec<MonitorInfo>) };
                let mut mi = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                if unsafe { GetMonitorInfoW(hmonitor, &mut mi).as_bool() } {
                    monitors.push(MonitorInfo {
                        index: monitors.len(),
                        x: mi.rcMonitor.left,
                        y: mi.rcMonitor.top,
                        width: mi.rcMonitor.right - mi.rcMonitor.left,
                        height: mi.rcMonitor.bottom - mi.rcMonitor.top,
                        is_primary: mi.dwFlags & 1 != 0,
                    });
                }
                true.into()
            }

            let _ = EnumDisplayMonitors(None, None, Some(enum_proc), LPARAM(ptr as isize));
            monitors
        }
    }

    pub fn run_message_loop(&self) {
        unsafe {
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    fn broadcast(&self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> anyhow::Result<()> {
        let state = self.state.lock();
        for &raw in &state.hwnds {
            let hwnd = HWND(raw as *mut std::ffi::c_void);
            unsafe { PostMessageW(Some(hwnd), msg, wparam, lparam)? };
        }
        Ok(())
    }

    fn send_to_monitor(
        &self,
        monitor_index: usize,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> anyhow::Result<()> {
        let state = self.state.lock();
        let raw = state
            .hwnds
            .get(monitor_index)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("No border window for monitor {monitor_index}"))?;
        let hwnd = HWND(raw as *mut std::ffi::c_void);
        unsafe { PostMessageW(Some(hwnd), msg, wparam, lparam)? };
        Ok(())
    }

    // --- rect ---

    pub fn update_rect_position_on(
        &self,
        monitor_index: usize,
        x: i32,
        y: i32,
    ) -> anyhow::Result<()> {
        // pack both signed values into a single i64 via LPARAM
        let packed = ((y as i64) << 32) | (x as u32 as i64);
        self.send_to_monitor(
            monitor_index,
            WM_UPDATE_RECT_POS,
            WPARAM(0),
            LPARAM(packed as isize),
        )
    }

    pub fn update_rect_size_on(
        &self,
        monitor_index: usize,
        width: i32,
        height: i32,
    ) -> anyhow::Result<()> {
        self.send_to_monitor(
            monitor_index,
            WM_UPDATE_RECT_SIZE,
            WPARAM(width as usize),
            LPARAM(height as isize),
        )
    }

    // --- appearance ---

    pub fn update_color(&self, color: u32) -> anyhow::Result<()> {
        self.broadcast(WM_UPDATE_COLOR, WPARAM(color as usize), LPARAM(0))
    }

    pub fn update_thickness(&self, thickness: f32) -> anyhow::Result<()> {
        self.broadcast(
            WM_UPDATE_THICKNESS,
            WPARAM(thickness.to_bits() as usize),
            LPARAM(0),
        )
    }

    pub fn update_corner_radius(&self, radius: f32) -> anyhow::Result<()> {
        self.broadcast(
            WM_UPDATE_RADIUS,
            WPARAM(radius.to_bits() as usize),
            LPARAM(0),
        )
    }

    // --- statusbar ---

    pub fn update_statusbar(&self, monitor_index: usize, bar: StatusBar) -> anyhow::Result<()> {
        let state = self.state.lock();
        let raw = state
            .hwnds
            .get(monitor_index)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("No border window for monitor {monitor_index}"))?;
        let hwnd = HWND(raw as *mut std::ffi::c_void);
        unsafe {
            PostMessageW(
                Some(hwnd),
                WM_UPDATE_STATUSBAR,
                WPARAM(Box::into_raw(Box::new(bar)) as usize),
                LPARAM(0),
            )?;
        }
        Ok(())
    }

    pub fn set_active_monitor(&self, monitor_index: usize) -> anyhow::Result<()> {
        let state = self.state.lock();
        for (i, &raw) in state.hwnds.iter().enumerate() {
            let hwnd = HWND(raw as *mut std::ffi::c_void);
            unsafe {
                PostMessageW(
                    Some(hwnd),
                    WM_SET_ACTIVE_MONITOR,
                    WPARAM((i == monitor_index) as usize),
                    LPARAM(0),
                )?;
            }
        }
        Ok(())
    }

    // --- visibility ---

    pub fn set_visible_on(&self, monitor_index: usize, visible: bool) -> anyhow::Result<()> {
        let state = self.state.lock();
        let raw = state
            .hwnds
            .get(monitor_index)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("No border window for monitor {monitor_index}"))?;
        let hwnd = HWND(raw as *mut std::ffi::c_void);
        unsafe { _ = ShowWindow(hwnd, if visible { SW_SHOW } else { SW_HIDE }) };
        Ok(())
    }

    // --- accessors ---

    pub fn hwnd(&self) -> HWND {
        let state = self.state.lock();
        HWND(state.hwnds.first().copied().unwrap_or(0) as *mut std::ffi::c_void)
    }

    pub fn hwnd_for(&self, monitor_index: usize) -> Option<HWND> {
        let state = self.state.lock();
        state
            .hwnds
            .get(monitor_index)
            .map(|&raw| HWND(raw as *mut std::ffi::c_void))
    }

    pub fn window_count(&self) -> usize {
        self.state.lock().hwnds.len()
    }

    pub fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

impl Drop for BorderManager {
    fn drop(&mut self) {
        if Arc::strong_count(&self.state) == 1 {
            let state = self.state.lock();
            for &raw in &state.hwnds {
                let hwnd = HWND(raw as *mut std::ffi::c_void);
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Internal window state
// ---------------------------------------------------------------------------

struct WindowData {
    render_target: ID2D1HwndRenderTarget,
    border_brush: ID2D1SolidColorBrush,
    thickness: f32,
    corner_radius: f32,
    rect_x: f32,
    rect_y: f32,
    rect_width: f32,
    rect_height: f32,
    dwrite_factory: IDWriteFactory,
    statusbar: Option<StatusBar>,
    statusbar_format: Option<IDWriteTextFormat>,
    is_active_monitor: bool,
    monitor_index: usize,
}

struct TransparentBorderWindow {
    hwnd: HWND,
}

impl TransparentBorderWindow {
    unsafe fn new(
        border_color: u32,
        border_thickness: f32,
        corner_radius: f32,
        monitor_index: Option<usize>,
    ) -> anyhow::Result<Self> {
        let d2d_factory: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)? };

        let class_name = w!("FullscreenBorderWindow");
        let hinstance = unsafe { GetModuleHandleW(None)? };

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(Self::wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
            ..Default::default()
        };
        let _ = unsafe { RegisterClassExW(&wc) };

        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE
                    | WS_EX_TRANSPARENT,
                class_name,
                w!("Border Overlay"),
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

        let monitor_rect = Self::resolve_monitor_rect(hwnd, monitor_index);
        let screen_width = monitor_rect.right - monitor_rect.left;
        let screen_height = monitor_rect.bottom - monitor_rect.top;

        unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                monitor_rect.left,
                monitor_rect.top,
                screen_width,
                screen_height,
                SWP_NOACTIVATE,
            )?;
        }

        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins)? };
        unsafe { SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)? };

        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
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
                width: screen_width as u32,
                height: screen_height as u32,
            },
            presentOptions: D2D1_PRESENT_OPTIONS_IMMEDIATELY,
        };

        let render_target = unsafe { d2d_factory.CreateHwndRenderTarget(&props, &hwnd_props)? };

        let border_color_d2d = D2D1_COLOR_F {
            r: ((border_color >> 16) & 0xFF) as f32 / 255.0,
            g: ((border_color >> 8) & 0xFF) as f32 / 255.0,
            b: (border_color & 0xFF) as f32 / 255.0,
            a: 1.0,
        };
        let border_brush = unsafe { render_target.CreateSolidColorBrush(&border_color_d2d, None)? };

        let dwrite_factory =
            unsafe { DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED)? };

        let window_data = Box::new(WindowData {
            render_target,
            border_brush,
            thickness: border_thickness,
            corner_radius,
            rect_x: 0.0,
            rect_y: 0.0,
            rect_width: 0.0,
            rect_height: 0.0,
            dwrite_factory,
            statusbar: None,
            statusbar_format: None,
            is_active_monitor: true,
            monitor_index: monitor_index.unwrap_or(0),
        });

        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(window_data) as isize);
            _ = InvalidateRect(Some(hwnd), None, false);
            _ = UpdateWindow(hwnd);
        }

        Ok(Self { hwnd })
    }

    fn resolve_monitor_rect(hwnd: HWND, monitor_index: Option<usize>) -> RECT {
        if let Some(index) = monitor_index {
            let monitors = BorderManager::get_monitors();
            if let Some(m) = monitors.get(index) {
                return RECT {
                    left: m.x,
                    top: m.y,
                    right: m.x + m.width,
                    bottom: m.y + m.height,
                };
            }
        }
        unsafe {
            let hmonitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let _ = GetMonitorInfoW(hmonitor, &mut mi);
            mi.rcMonitor
        }
    }

    fn hwnd(&self) -> HWND {
        self.hwnd
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
                    let data = unsafe { &*(ptr as *const WindowData) };
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
                                    draw_statusbar(data, bar, screen_width);
                                }
                                Visibility::OnFocus => {
                                    if data.is_active_monitor {
                                        draw_statusbar(data, bar, screen_width);
                                    }
                                }
                                Visibility::Disable => {}
                            }
                        };
                        let toolbar_height = get_toolbar_height(data.monitor_index);
                        let half = data.thickness / 2.0;
                        let rounded_rect = D2D1_ROUNDED_RECT {
                            rect: D2D_RECT_F {
                                left: data.rect_x + half,
                                top: data.rect_y + half + toolbar_height as f32,
                                right: data.rect_x + data.rect_width - half,
                                bottom: data.rect_y + data.rect_height - half
                                    + toolbar_height as f32,
                            },
                            radiusX: data.corner_radius,
                            radiusY: data.corner_radius,
                        };
                        data.render_target.DrawRoundedRectangle(
                            &rounded_rect,
                            &data.border_brush,
                            data.thickness,
                            None,
                        );

                        let _ = data.render_target.EndDraw(None, None);
                    }
                }
                let _ = unsafe { ValidateRect(Some(hwnd), None) };
                LRESULT(0)
            }

            WM_UPDATE_COLOR => {
                let color = wparam.0 as u32;
                let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
                if ptr != 0 {
                    let data = unsafe { &*(ptr as *const WindowData) };
                    let d2d_color = D2D1_COLOR_F {
                        r: ((color >> 16) & 0xFF) as f32 / 255.0,
                        g: ((color >> 8) & 0xFF) as f32 / 255.0,
                        b: (color & 0xFF) as f32 / 255.0,
                        a: 1.0,
                    };
                    unsafe {
                        data.border_brush.SetColor(&d2d_color);
                        _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }

            WM_UPDATE_RECT_POS => {
                // unpack signed x/y from packed i64
                let packed = lparam.0 as i64;
                let x = (packed & 0xFFFFFFFF) as i32;
                let y = ((packed >> 32) & 0xFFFFFFFF) as i32;
                let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
                if ptr != 0 {
                    let data = unsafe { &mut *(ptr as *mut WindowData) };
                    data.rect_x = x as f32;
                    data.rect_y = y as f32;
                    unsafe {
                        _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }

            WM_UPDATE_RECT_SIZE => {
                let width = wparam.0 as i32;
                let height = lparam.0 as i32;
                let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
                if ptr != 0 {
                    let data = unsafe { &mut *(ptr as *mut WindowData) };
                    data.rect_width = width as f32;
                    data.rect_height = height as f32;
                    unsafe {
                        _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }

            WM_UPDATE_THICKNESS => {
                let thickness = f32::from_bits(wparam.0 as u32);
                let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
                if ptr != 0 {
                    let data = unsafe { &mut *(ptr as *mut WindowData) };
                    data.thickness = thickness;
                    unsafe {
                        _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }

            WM_UPDATE_RADIUS => {
                let radius = f32::from_bits(wparam.0 as u32);
                let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
                if ptr != 0 {
                    let data = unsafe { &mut *(ptr as *mut WindowData) };
                    data.corner_radius = radius;
                    unsafe {
                        _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }

            WM_UPDATE_STATUSBAR => {
                let ptr = wparam.0 as *mut StatusBar;
                if !ptr.is_null() {
                    unsafe {
                        let bar = Box::from_raw(ptr);
                        let data_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                        if data_ptr != 0 {
                            let data = &mut *(data_ptr as *mut WindowData);

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

            WM_SET_ACTIVE_MONITOR => {
                let is_active = wparam.0 != 0;
                let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
                if ptr != 0 {
                    let data = unsafe { &mut *(ptr as *mut WindowData) };
                    data.is_active_monitor = is_active;
                    unsafe {
                        _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }

            WM_SIZE => {
                let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
                if ptr != 0 {
                    let data = unsafe { &*(ptr as *const WindowData) };
                    let width = (lparam.0 & 0xFFFF) as u32;
                    let height = ((lparam.0 >> 16) & 0xFFFF) as u32;
                    let _ = unsafe { data.render_target.Resize(&D2D_SIZE_U { width, height }) };
                }
                LRESULT(0)
            }

            WM_ERASEBKGND => LRESULT(1),
            WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),

            WM_DESTROY => {
                let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
                if ptr != 0 {
                    let _ = unsafe { Box::from_raw(ptr as *mut WindowData) };
                    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
                }
                LRESULT(0)
            }

            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }
}

// ---------------------------------------------------------------------------
// Statusbar rendering
// ---------------------------------------------------------------------------

unsafe fn draw_statusbar(data: &WindowData, bar: &StatusBar, screen_width: f32) {
    let fmt = match &data.statusbar_format {
        Some(f) => f,
        None => return,
    };

    let pad = bar.padding;
    let h = bar.height;

    let measure = |s: &SlotText| -> f32 {
        let wide: Vec<u16> = s.text.encode_utf16().collect();
        measure_text_width_layout(&data.dwrite_factory, fmt, &wide) + pad * 2.0 + 2.0
    };
    let y = 0.0;
    draw_slots(data, fmt, &bar.left, 4.0, 0.0, h, pad, false);

    let center_total: f32 = bar.center.iter().map(|s| measure(s)).sum();
    let center_x = (screen_width - center_total) / 2.0;
    draw_slots(data, fmt, &bar.center, center_x, y, h, pad, false);
    draw_slots(data, fmt, &bar.right, screen_width - 4.0, y, h, pad, true);
}

fn draw_slots(
    data: &WindowData,
    fmt: &IDWriteTextFormat,
    slots: &[SlotText],
    start_x: f32,
    y: f32,
    height: f32,
    padding: f32,
    right_align: bool,
) {
    let gap = 2.0;
    let slot_widths: Vec<f32> = slots
        .iter()
        .map(|slot| {
            let wide: Vec<u16> = slot.text.encode_utf16().collect();
            measure_text_width_layout(&data.dwrite_factory, fmt, &wide) + padding * 2.0
        })
        .collect();

    let total_w = slot_widths.iter().sum::<f32>() + gap * (slots.len().saturating_sub(1)) as f32; // gap only between slots, not after last

    let mut x = if right_align {
        start_x - total_w
    } else {
        start_x
    };
    for (slot, &sw) in slots.iter().zip(slot_widths.iter()) {
        let wide: Vec<u16> = slot.text.encode_utf16().collect();
        let slot_w = sw;

        // background pill
        if slot.background != 0 {
            let a = ((slot.background >> 24) & 0xFF) as f32 / 255.0;
            let bg = D2D1_COLOR_F {
                r: ((slot.background >> 16) & 0xFF) as f32 / 255.0,
                g: ((slot.background >> 8) & 0xFF) as f32 / 255.0,
                b: (slot.background & 0xFF) as f32 / 255.0,
                // if no alpha byte provided (0x00RRGGBB), default to fully opaque
                a: if a > 0.0 { a } else { 1.0 },
            };
            if let Ok(brush) = unsafe { data.render_target.CreateSolidColorBrush(&bg, None) } {
                let bg_rect = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: x,
                        top: y + 2.0,
                        right: x + slot_w,
                        bottom: y + height - 2.0,
                    },
                    radiusX: 4.0,
                    radiusY: 4.0,
                };
                unsafe { data.render_target.FillRoundedRectangle(&bg_rect, &brush) };
            }
        }

        // text
        let fg = D2D1_COLOR_F {
            r: ((slot.foreground >> 16) & 0xFF) as f32 / 255.0,
            g: ((slot.foreground >> 8) & 0xFF) as f32 / 255.0,
            b: (slot.foreground & 0xFF) as f32 / 255.0,
            a: 1.0,
        };
        if let Ok(brush) = unsafe { data.render_target.CreateSolidColorBrush(&fg, None) } {
            let text_rect = D2D_RECT_F {
                left: x + padding,
                top: y,
                right: x + slot_w - padding,
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

        x += slot_w + gap;
    }
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
