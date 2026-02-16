use parking_lot::Mutex;
use std::{sync::Arc, thread, time::Duration};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::{Common::*, *},
            Dwm::*,
            Gdi::{InvalidateRect, UpdateWindow, ValidateRect},
        },
        System::LibraryLoader::*,
        UI::{Controls::MARGINS, WindowsAndMessaging::*},
    },
    core::*,
};
macro_rules! hwnd {
    ($self:ident) => {
        HWND($self.hwnd as *mut std::ffi::c_void)
    };
}
// Custom messages
const WM_UPDATE_COLOR: u32 = WM_USER + 1;
const WM_UPDATE_THICKNESS: u32 = WM_USER + 2;
const WM_UPDATE_RADIUS: u32 = WM_USER + 3;
const PADDING: i32 = 0;

// Only store thread-safe data (HWND is safe to share)
struct BorderState {
    hwnd: isize,
}

pub struct BorderManager {
    pub state: Arc<Mutex<BorderState>>,
}

impl BorderManager {
    pub fn new() -> Self {
        let window = unsafe {
            TransparentBorderWindow::new(
                100,      // x
                100,      // y
                0,        // width
                0,        // height
                0xAC3E31, // red color
                1.0,      // border thickness
                4.0,      // corner radius
            )
            .unwrap()
        };

        let state = Arc::new(Mutex::new(BorderState {
            hwnd: window.hwnd().0 as isize,
        }));

        // Don't drop window - it needs to stay alive
        std::mem::forget(window);

        Self { state }
    }

    /// Start the message loop - call this in a dedicated thread
    pub fn run_message_loop(&self) {
        unsafe {
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    /// Update position (thread-safe)
    pub fn update_position(&self, x: i32, y: i32) -> anyhow::Result<()> {
        let state = self.state.lock();
        unsafe {
            SetWindowPos(hwnd!(state), Some(HWND_TOPMOST), x, y, 0, 0, SWP_NOACTIVATE)?;
        }
        Ok(())
    }
    pub fn update_sizepos_delay(
        &self,
        delay: u64,
        size: (i32, i32),
        position: (i32, i32),
    ) -> anyhow::Result<()> {
        let state = self.state.clone();
        thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(delay));
            let state = state.lock();
            let w = (size.0 - PADDING * 2).max(0);
            let h = (size.1 - PADDING * 2).max(0);
            unsafe {
                _ = SetWindowPos(
                    hwnd!(state),
                    Some(HWND_TOPMOST),
                    position.0 + PADDING,
                    position.1 + PADDING,
                    w,
                    h,
                    SWP_NOACTIVATE,
                );
            }
        });
        Ok(())
    }
    pub fn update_sizepos(&self, size: (i32, i32), position: (i32, i32)) -> anyhow::Result<()> {
        let state = self.state.lock();
        let w = (size.0 - PADDING * 2).max(0);
        let h = (size.1 - PADDING * 2).max(0);
        unsafe {
            _ = SetWindowPos(
                hwnd!(state),
                Some(HWND_TOPMOST),
                position.0 + PADDING,
                position.1 + PADDING,
                w,
                h,
                SWP_NOACTIVATE,
            );
        }
        Ok(())
    }

    /// Update size (thread-safe)
    pub fn update_size(&self, width: i32, height: i32) -> anyhow::Result<()> {
        let state = self.state.lock();
        unsafe {
            SetWindowPos(
                hwnd!(state),
                Some(HWND_TOPMOST),
                0,
                0,
                width,
                height,
                SWP_NOMOVE | SWP_NOACTIVATE,
            )?;
        }
        Ok(())
    }

    /// Update corner radius (thread-safe)
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

    /// Update color (thread-safe)
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

    /// Update thickness (thread-safe)
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

    /// Set visibility (thread-safe)
    pub fn set_visible(&self, visible: bool) -> anyhow::Result<()> {
        let state = self.state.lock();
        unsafe {
            _ = ShowWindow(hwnd!(state), if visible { SW_SHOW } else { SW_HIDE });
        }
        Ok(())
    }

    /// Get window handle (thread-safe)
    pub fn hwnd(&self) -> HWND {
        let state = self.state.lock();
        hwnd!(state)
    }

    /// Clone the manager for sharing across threads
    pub fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

impl Drop for BorderManager {
    fn drop(&mut self) {
        // Only destroy when the last reference is dropped
        if Arc::strong_count(&self.state) == 1 {
            let state = self.state.lock();
            unsafe {
                let _ = DestroyWindow(hwnd!(state));
            }
        }
    }
}

// Window data stored in GWLP_USERDATA - lives only in window thread
struct WindowData {
    render_target: ID2D1HwndRenderTarget,
    brush: ID2D1SolidColorBrush,
    thickness: f32,
    corner_radius: f32,
}

// Simple transparent window with rounded border
struct TransparentBorderWindow {
    hwnd: HWND,
}

impl TransparentBorderWindow {
    unsafe fn new(
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        border_color: u32,
        border_thickness: f32,
        corner_radius: f32,
    ) -> anyhow::Result<Self> {
        // Create D2D factory
        let d2d_factory: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)? };

        // Register window class
        let class_name = w!("ThreadSafeBorderWindow");
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

        // Create the window
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE
                    | WS_EX_TRANSPARENT,
                class_name,
                w!("Border"),
                WS_POPUP | WS_VISIBLE,
                x,
                y,
                width,
                height,
                None,
                None,
                Some(HINSTANCE(hinstance.0)),
                None,
            )?
        };

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
                width: width as u32,
                height: height as u32,
            },
            presentOptions: D2D1_PRESENT_OPTIONS_IMMEDIATELY,
        };

        let render_target = unsafe { d2d_factory.CreateHwndRenderTarget(&props, &hwnd_props)? };

        let color = D2D1_COLOR_F {
            r: ((border_color >> 16) & 0xFF) as f32 / 255.0,
            g: ((border_color >> 8) & 0xFF) as f32 / 255.0,
            b: (border_color & 0xFF) as f32 / 255.0,
            a: 1.0,
        };

        let brush = unsafe { render_target.CreateSolidColorBrush(&color, None)? };

        // Store window data
        let window_data = Box::new(WindowData {
            render_target,
            brush,
            thickness: border_thickness,
            corner_radius,
        });

        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(window_data) as isize);
        }

        // Initial paint
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

                        data.render_target.Clear(Some(&D2D1_COLOR_F {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }));

                        let mut rect = RECT::default();
                        let _ = GetClientRect(hwnd, &mut rect);

                        let width = (rect.right - rect.left) as f32;
                        let height = (rect.bottom - rect.top) as f32;
                        let half = data.thickness / 2.0;

                        let rounded_rect = D2D1_ROUNDED_RECT {
                            rect: D2D_RECT_F {
                                left: half,
                                top: half,
                                right: width - half,
                                bottom: height - half,
                            },
                            radiusX: data.corner_radius,
                            radiusY: data.corner_radius,
                        };

                        data.render_target.DrawRoundedRectangle(
                            &rounded_rect,
                            &data.brush,
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
                        data.brush.SetColor(&d2d_color);
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
