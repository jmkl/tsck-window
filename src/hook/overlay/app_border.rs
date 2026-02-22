use windows::Win32::{
    Foundation::*,
    Graphics::{
        Direct2D::{Common::*, *},
        Dwm::*,
        Dxgi::Common::*,
        Gdi::{InvalidateRect, ValidateRect},
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{Controls::MARGINS, WindowsAndMessaging::*},
};
use windows::core::*;

use crate::hook::win_api;

const WM_SET_FOCUS_BORDER: u32 = WM_USER + 30;
const WM_SET_TOPMOST_BORDER: u32 = WM_USER + 31;
const WM_REMOVE_TOPMOST_BORDER: u32 = WM_USER + 32;

#[derive(Clone, Debug)]
pub struct BorderInfo {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub color: u32,
    pub thickness: f32,
    pub radius: f32,
}

struct BorderOverlayData {
    render_target: ID2D1HwndRenderTarget,
    // current focus window border
    focus_border: Option<BorderInfo>,
    // always-on-top windows borders, keyed by hwnd
    topmost_borders: std::collections::HashMap<isize, BorderInfo>,
    // virtual screen origin for coordinate offset
    virt_x: i32,
    virt_y: i32,
}

pub struct BorderOverlay {
    hwnd: isize,
}

unsafe impl Send for BorderOverlay {}
unsafe impl Sync for BorderOverlay {}

impl BorderOverlay {
    pub fn new() -> anyhow::Result<Self> {
        let hinstance: HINSTANCE = unsafe { GetModuleHandleW(None)?.into() };

        // virtual screen covers all monitors
        let virt_x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let virt_y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let virt_w = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
        let virt_h = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };

        let class_name = w!("BorderOverlayD2D");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance,
            lpszClassName: class_name,
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
                w!(""),
                WS_POPUP | WS_VISIBLE,
                virt_x,
                virt_y,
                virt_w,
                virt_h,
                None,
                None,
                Some(hinstance),
                None,
            )?
        };

        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins)? };
        unsafe { SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)? };

        // D2D setup
        let d2d_factory: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)? };

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
                width: virt_w as u32,
                height: virt_h as u32,
            },
            presentOptions: D2D1_PRESENT_OPTIONS_IMMEDIATELY,
        };
        let render_target = unsafe { d2d_factory.CreateHwndRenderTarget(&props, &hwnd_props)? };

        let data = Box::new(BorderOverlayData {
            render_target,
            focus_border: None,
            topmost_borders: std::collections::HashMap::new(),
            virt_x,
            virt_y,
        });

        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(data) as isize);
            _ = InvalidateRect(Some(hwnd), None, false);
        }

        Ok(Self {
            hwnd: hwnd.0 as isize,
        })
    }

    fn hwnd(&self) -> HWND {
        HWND(self.hwnd as *mut _)
    }

    /// Set the focused window border — call from hook thread
    pub fn set_focus(&self, info: BorderInfo) {
        unsafe {
            let _ = PostMessageW(
                Some(self.hwnd()),
                WM_SET_FOCUS_BORDER,
                WPARAM(Box::into_raw(Box::new(info)) as usize),
                LPARAM(0),
            );
        }
    }

    /// Clear focus border
    pub fn clear_focus(&self) {
        unsafe {
            let _ = PostMessageW(Some(self.hwnd()), WM_SET_FOCUS_BORDER, WPARAM(0), LPARAM(0));
        }
    }

    /// Add/update an always-on-top border by hwnd key
    pub fn set_topmost(&self, hwnd_key: isize, info: BorderInfo) {
        unsafe {
            let payload = Box::new((hwnd_key, info));
            let _ = PostMessageW(
                Some(self.hwnd()),
                WM_SET_TOPMOST_BORDER,
                WPARAM(Box::into_raw(payload) as usize),
                LPARAM(0),
            );
        }
    }

    /// Remove an always-on-top border by hwnd key
    pub fn remove_topmost(&self, hwnd_key: isize) {
        unsafe {
            let _ = PostMessageW(
                Some(self.hwnd()),
                WM_REMOVE_TOPMOST_BORDER,
                WPARAM(hwnd_key as usize),
                LPARAM(0),
            );
        }
    }
}

impl Drop for BorderOverlay {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd());
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_SET_FOCUS_BORDER => {
                let data_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                if data_ptr != 0 {
                    let data = &mut *(data_ptr as *mut BorderOverlayData);
                    if wparam.0 == 0 {
                        data.focus_border = None;
                    } else {
                        let info = Box::from_raw(wparam.0 as *mut BorderInfo);
                        data.focus_border = Some(*info);
                    }
                    _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }

            WM_SET_TOPMOST_BORDER => {
                let data_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                if data_ptr != 0 && wparam.0 != 0 {
                    let data = &mut *(data_ptr as *mut BorderOverlayData);
                    let payload = Box::from_raw(wparam.0 as *mut (isize, BorderInfo));
                    let (key, info) = *payload;
                    data.topmost_borders.insert(key, info);
                    _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }

            WM_REMOVE_TOPMOST_BORDER => {
                let data_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                if data_ptr != 0 {
                    let data = &mut *(data_ptr as *mut BorderOverlayData);
                    data.topmost_borders.remove(&(wparam.0 as isize));
                    _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }

            WM_PAINT => {
                let data_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                if data_ptr != 0 {
                    let data = &*(data_ptr as *const BorderOverlayData);
                    data.render_target.BeginDraw();
                    data.render_target.Clear(Some(&D2D1_COLOR_F {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 0.0,
                    }));

                    // draw focus border
                    if let Some(ref info) = data.focus_border {
                        draw_border(&data.render_target, info, data.virt_x, data.virt_y);
                    }

                    // draw all topmost borders
                    for info in data.topmost_borders.values() {
                        draw_border(&data.render_target, info, data.virt_x, data.virt_y);
                    }

                    let _ = data.render_target.EndDraw(None, None);
                }
                _ = ValidateRect(Some(hwnd), None);
                LRESULT(0)
            }

            WM_SIZE => {
                let data_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                if data_ptr != 0 {
                    let data = &*(data_ptr as *const BorderOverlayData);
                    let w = (lparam.0 & 0xFFFF) as u32;
                    let h = ((lparam.0 >> 16) & 0xFFFF) as u32;
                    let _ = data.render_target.Resize(&D2D_SIZE_U {
                        width: w,
                        height: h,
                    });
                }
                LRESULT(0)
            }

            WM_DESTROY => {
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                if ptr != 0 {
                    let _ = Box::from_raw(ptr as *mut BorderOverlayData);
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                LRESULT(0)
            }

            WM_ERASEBKGND => LRESULT(1),
            WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn draw_border(rt: &ID2D1HwndRenderTarget, info: &BorderInfo, virt_x: i32, virt_y: i32) {
    let color = D2D1_COLOR_F {
        r: ((info.color >> 16) & 0xFF) as f32 / 255.0,
        g: ((info.color >> 8) & 0xFF) as f32 / 255.0,
        b: (info.color & 0xFF) as f32 / 255.0,
        a: 1.0,
    };
    let Ok(brush) = (unsafe { rt.CreateSolidColorBrush(&color, None) }) else {
        return;
    };

    let half = info.thickness / 2.0;
    // offset screen coords to overlay-local coords
    let x = (info.x - virt_x) as f32;
    let y = (info.y - virt_y) as f32;
    let w = info.width as f32;
    let h = info.height as f32;

    let rounded = D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left: x + half,
            top: y + half,
            right: x + w - half,
            bottom: y + h - half,
        },
        radiusX: info.radius,
        radiusY: info.radius,
    };
    unsafe { rt.DrawRoundedRectangle(&rounded, &brush, info.thickness, None) };
}
