use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::{Common::*, *},
            DirectWrite::{
                DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
                DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
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

use crate::hook::{api::Hwnd, app_info::AppPosition};

macro_rules! hwnd {
    ($self:ident) => {
        HWND($self.hwnd as *mut std::ffi::c_void)
    };
}

// Custom messages
const WM_UPDATE_COLOR: u32 = WM_USER + 1;
const WM_UPDATE_THICKNESS: u32 = WM_USER + 2;
const WM_UPDATE_RADIUS: u32 = WM_USER + 3;
const WM_UPDATE_WORKSPACES: u32 = WM_USER + 4;
const WM_UPDATE_RECT_POS: u32 = WM_USER + 5;
const WM_UPDATE_RECT_SIZE: u32 = WM_USER + 6;

#[derive(Clone, Debug)]
pub struct HwndItem {
    pub hwnd: Hwnd,
    pub monitor: usize,
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

struct BorderState {
    hwnd: isize,
}

pub struct BorderManager {
    pub state: Arc<Mutex<BorderState>>,
}

impl BorderManager {
    /// Create a new BorderManager on the primary monitor
    pub fn new() -> Self {
        Self::new_on_monitor(None)
    }

    /// Create a BorderManager on a specific monitor (None = primary, Some(index) = specific monitor)
    pub fn new_on_monitor(monitor_index: Option<usize>) -> Self {
        let window = unsafe {
            TransparentBorderWindow::new(
                0xAC3E31, // red color
                2.0,      // border thickness
                5.0,      // corner radius
                monitor_index,
            )
            .unwrap()
        };

        let state = Arc::new(Mutex::new(BorderState {
            hwnd: window.hwnd().0 as isize,
        }));

        std::mem::forget(window);

        Self { state }
    }

    /// Get list of all available monitors
    pub fn get_monitors() -> Vec<MonitorInfo> {
        unsafe {
            let mut monitors = Vec::new();
            let monitors_ptr = &mut monitors as *mut Vec<MonitorInfo>;

            unsafe extern "system" fn enum_proc(
                hmonitor: HMONITOR,
                _hdc: HDC,
                _rect: *mut RECT,
                lparam: LPARAM,
            ) -> BOOL {
                let monitors = unsafe { &mut *(lparam.0 as *mut Vec<MonitorInfo>) };

                let mut monitor_info = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };

                if unsafe { GetMonitorInfoW(hmonitor, &mut monitor_info).as_bool() } {
                    let info = MonitorInfo {
                        index: monitors.len(),
                        x: monitor_info.rcMonitor.left,
                        y: monitor_info.rcMonitor.top,
                        width: monitor_info.rcMonitor.right - monitor_info.rcMonitor.left,
                        height: monitor_info.rcMonitor.bottom - monitor_info.rcMonitor.top,
                        is_primary: monitor_info.dwFlags & 1 != 0, // MONITORINFOF_PRIMARY = 1
                    };
                    monitors.push(info);
                }

                true.into()
            }

            let _ = EnumDisplayMonitors(None, None, Some(enum_proc), LPARAM(monitors_ptr as isize));
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

    /// Update the bordered rectangle position
    pub fn update_rect_position(&self, x: i32, y: i32) -> anyhow::Result<()> {
        let state = self.state.lock();
        unsafe {
            PostMessageW(
                Some(hwnd!(state)),
                WM_UPDATE_RECT_POS,
                WPARAM(x as usize),
                LPARAM(y as isize),
            )?;
        }
        Ok(())
    }

    /// Update the bordered rectangle size
    pub fn update_rect_size(&self, width: i32, height: i32) -> anyhow::Result<()> {
        let state = self.state.lock();
        unsafe {
            PostMessageW(
                Some(hwnd!(state)),
                WM_UPDATE_RECT_SIZE,
                WPARAM(width as usize),
                LPARAM(height as isize),
            )?;
        }
        Ok(())
    }

    /// Update both position and size of the bordered rectangle
    pub fn update_rect_bounds(
        &self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> anyhow::Result<()> {
        self.update_rect_position(x, y)?;
        self.update_rect_size(width, height)?;
        Ok(())
    }

    /// Update workspaces
    pub fn update_workspaces(&self, workspaces: Vec<Workspace>) -> anyhow::Result<()> {
        let state = self.state.lock();
        unsafe {
            PostMessageW(
                Some(hwnd!(state)),
                WM_UPDATE_WORKSPACES,
                WPARAM(Box::into_raw(Box::new(workspaces)) as usize),
                LPARAM(0),
            )?;
        }
        Ok(())
    }

    /// Update corner radius
    pub fn update_corner_radius(&self, radius: f32) -> anyhow::Result<()> {
        let state = self.state.lock();
        unsafe {
            PostMessageW(
                Some(hwnd!(state)),
                WM_UPDATE_RADIUS,
                WPARAM(radius.to_bits() as usize),
                LPARAM(0),
            )?;
        }
        Ok(())
    }

    /// Update color
    pub fn update_color(&self, color: u32) -> anyhow::Result<()> {
        let state = self.state.lock();
        unsafe {
            PostMessageW(
                Some(hwnd!(state)),
                WM_UPDATE_COLOR,
                WPARAM(color as usize),
                LPARAM(0),
            )?;
        }
        Ok(())
    }

    /// Update thickness
    pub fn update_thickness(&self, thickness: f32) -> anyhow::Result<()> {
        let state = self.state.lock();
        unsafe {
            PostMessageW(
                Some(hwnd!(state)),
                WM_UPDATE_THICKNESS,
                WPARAM(thickness.to_bits() as usize),
                LPARAM(0),
            )?;
        }
        Ok(())
    }

    /// Set visibility
    pub fn set_visible(&self, visible: bool) -> anyhow::Result<()> {
        let state = self.state.lock();
        unsafe {
            _ = ShowWindow(hwnd!(state), if visible { SW_SHOW } else { SW_HIDE });
        }
        Ok(())
    }

    /// Get window handle
    pub fn hwnd(&self) -> HWND {
        let state = self.state.lock();
        hwnd!(state)
    }

    /// Clone the manager
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
            unsafe {
                let _ = DestroyWindow(hwnd!(state));
            }
        }
    }
}

struct WorkspaceData {
    text: Vec<u16>,
    active: bool,
}

struct WindowData {
    render_target: ID2D1HwndRenderTarget,
    border_brush: ID2D1SolidColorBrush,
    active_bg_brush: ID2D1SolidColorBrush,
    inactive_bg_brush: ID2D1SolidColorBrush,
    text_brush: ID2D1SolidColorBrush,
    text_format: IDWriteTextFormat,
    thickness: f32,
    corner_radius: f32,
    workspaces: Vec<WorkspaceData>,
    rect_x: f32,
    rect_y: f32,
    rect_width: f32,
    rect_height: f32,
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

        // Create fullscreen window
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

        // Get the target monitor
        let monitor_rect = if let Some(index) = monitor_index {
            // Get specific monitor by index
            let monitors = BorderManager::get_monitors();
            monitors
                .get(index)
                .map(|m| RECT {
                    left: m.x,
                    top: m.y,
                    right: m.x + m.width,
                    bottom: m.y + m.height,
                })
                .unwrap_or_else(|| {
                    // Fallback to primary if index is invalid
                    let hmonitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY) };
                    let mut monitor_info = MONITORINFO {
                        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                        ..Default::default()
                    };
                    unsafe { GetMonitorInfoW(hmonitor, &mut monitor_info).unwrap() };
                    monitor_info.rcMonitor
                })
        } else {
            // Use primary monitor
            let hmonitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY) };
            let mut monitor_info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            unsafe {
                _ = GetMonitorInfoW(hmonitor, &mut monitor_info);
            };
            monitor_info.rcMonitor
        };

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

        // Enable DWM transparency
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins)? };
        unsafe { SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)? };

        // Create render target
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

        // Active workspace background (red)
        let active_bg_brush = unsafe {
            render_target.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 0.6745,
                    g: 0.2431,
                    b: 0.1922,
                    a: 1.0,
                },
                None,
            )?
        };

        // Inactive workspace background (dim gray)
        let inactive_bg_brush = unsafe {
            render_target.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 0.2,
                    g: 0.2,
                    b: 0.2,
                    a: 0.6,
                },
                None,
            )?
        };

        // Text brush (white)
        let text_brush = unsafe {
            render_target.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                },
                None,
            )?
        };

        let dwrite_factory =
            unsafe { DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED)? };

        let text_format = unsafe {
            dwrite_factory.CreateTextFormat(
                w!("Segoe UI"),
                None,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                14.0,
                w!("en-us"),
            )?
        };

        unsafe {
            text_format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            text_format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        }

        // Initial workspaces
        let workspaces = vec![WorkspaceData {
            text: "Main".encode_utf16().collect(),
            active: true,
        }];

        let window_data = Box::new(WindowData {
            render_target,
            border_brush,
            active_bg_brush,
            inactive_bg_brush,
            text_brush,
            text_format,
            thickness: border_thickness,
            corner_radius,
            workspaces,
            rect_x: 0.0,
            rect_y: 0.0,
            rect_width: 0.0,
            rect_height: 0.0,
        });

        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(window_data) as isize);
        }

        unsafe {
            _ = InvalidateRect(Some(hwnd), None, false);
            _ = UpdateWindow(hwnd);
        }

        Ok(Self { hwnd })
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

                        // Clear to transparent
                        data.render_target.Clear(Some(&D2D1_COLOR_F {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }));

                        let mut rect = RECT::default();
                        let _ = GetClientRect(hwnd, &mut rect);
                        let screen_width = (rect.right - rect.left) as f32;
                        let _screen_height = (rect.bottom - rect.top) as f32;

                        // Draw the bordered rectangle
                        let half = data.thickness / 2.0;
                        let rounded_rect = D2D1_ROUNDED_RECT {
                            rect: D2D_RECT_F {
                                left: data.rect_x + half,
                                top: data.rect_y + half,
                                right: data.rect_x + data.rect_width - half,
                                bottom: data.rect_y + data.rect_height - half,
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

                        // Draw workspaces at top center
                        let workspace_height = 20.0;
                        let workspace_width = 80.0;
                        let workspace_gap = 3.0;
                        let total_width = data.workspaces.len() as f32
                            * (workspace_width + workspace_gap)
                            - workspace_gap;
                        let start_x = (screen_width - total_width) / 2.0;

                        for (i, workspace) in data.workspaces.iter().enumerate() {
                            let x = start_x + i as f32 * (workspace_width + workspace_gap);
                            let y = 5.0;

                            // Draw background
                            let bg_rect = D2D1_ROUNDED_RECT {
                                rect: D2D_RECT_F {
                                    left: x,
                                    top: y,
                                    right: x + workspace_width,
                                    bottom: y + workspace_height,
                                },
                                radiusX: 4.0,
                                radiusY: 4.0,
                            };

                            let bg_brush = if workspace.active {
                                &data.active_bg_brush
                            } else {
                                &data.inactive_bg_brush
                            };

                            data.render_target.FillRoundedRectangle(&bg_rect, bg_brush);

                            // Draw text
                            let text_rect = D2D_RECT_F {
                                left: x,
                                top: y,
                                right: x + workspace_width,
                                bottom: y + workspace_height,
                            };

                            data.render_target.DrawText(
                                &workspace.text,
                                &data.text_format,
                                &text_rect,
                                &data.text_brush,
                                D2D1_DRAW_TEXT_OPTIONS_NONE,
                                DWRITE_MEASURING_MODE_NATURAL,
                            );
                        }

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
            WM_UPDATE_WORKSPACES => {
                let ptr = wparam.0 as *mut Vec<Workspace>;
                if !ptr.is_null() {
                    unsafe {
                        let workspaces = Box::from_raw(ptr);
                        let data_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                        if data_ptr != 0 {
                            let data = &mut *(data_ptr as *mut WindowData);
                            data.workspaces = workspaces
                                .iter()
                                .map(|w| WorkspaceData {
                                    text: w.text.encode_utf16().collect(),
                                    active: w.active,
                                })
                                .collect();
                        }
                        _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_UPDATE_RECT_POS => {
                let x = wparam.0 as i32;
                let y = lparam.0 as i32;
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
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }
}
