use crate::{hex, log_error, win::winapi::WindowsAPI};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::{Common::*, *},
            Dwm::*,
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            Gdi::{InvalidateRect, ValidateRect},
        },
        System::LibraryLoader::*,
        UI::{Controls::MARGINS, WindowsAndMessaging::*},
    },
    core::*,
};

const WM_SET_FOCUS_BORDER: u32 = WM_USER + 30;
const WM_SET_TOPMOST_BORDER: u32 = WM_USER + 31;

#[derive(Clone, Debug)]
pub struct BorderInfo {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub color: u32,
    pub thickness: f32,
    pub radius: f32,
    pub target: isize,
    pub blacklist: Vec<String>,
}

struct BorderOverlayData {
    render_target: ID2D1HwndRenderTarget,
    border_info: Option<BorderInfo>,
    topmost_border_info: Vec<BorderInfo>,
    virt_x: i32,
    virt_y: i32,
}

pub struct BorderOverlay {
    hwnd: isize,
    class_name: String,
}

unsafe impl Send for BorderOverlay {}
unsafe impl Sync for BorderOverlay {}
impl Clone for BorderOverlay {
    fn clone(&self) -> Self {
        Self {
            hwnd: self.hwnd,
            class_name: self.class_name.clone(),
        }
    }
}
impl BorderOverlay {
    pub fn new(id: &str) -> anyhow::Result<Self> {
        let hinstance: HINSTANCE = unsafe { GetModuleHandleW(None)?.into() };
        let class_name_str = format!("Border-{}\0", id);
        let class_name = class_name_str.encode_utf16().collect::<Vec<_>>();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        unsafe { RegisterClassExW(&wc) };

        let virt_x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let virt_y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let virt_w = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
        let virt_h = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };

        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED
                    // | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE
                    | WS_EX_TRANSPARENT,
                PCWSTR(class_name.as_ptr()),
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
            border_info: None,
            topmost_border_info: Vec::new(),
            virt_x,
            virt_y,
        });

        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(data) as isize);
        }

        Ok(Self {
            hwnd: hwnd.0 as isize,
            class_name: class_name_str,
        })
    }

    pub fn hwnd(&self) -> HWND {
        HWND(self.hwnd as *mut _)
    }
    pub fn get_class_name(&self) -> String {
        log_error!("get_class_name", &self.class_name);
        self.class_name.clone()
    }

    pub fn set_focus(&self, info: BorderInfo) {
        unsafe {
            let payload = Box::new(info);
            let _ = PostMessageW(
                Some(self.hwnd()),
                WM_SET_FOCUS_BORDER,
                WPARAM(Box::into_raw(payload) as usize),
                LPARAM(1), // 1 = has focus
            );
        }
    }

    pub fn clear_focus(&self) {
        unsafe {
            let _ = PostMessageW(
                Some(self.hwnd()),
                WM_SET_FOCUS_BORDER,
                WPARAM(0),
                LPARAM(0), // 0 = clear
            );
        }
    }
    pub fn set_top_most(&self, info: Vec<BorderInfo>) {
        unsafe {
            let payload = Box::new(info);
            let _ = PostMessageW(
                Some(self.hwnd()),
                WM_SET_TOPMOST_BORDER,
                WPARAM(Box::into_raw(payload) as usize),
                LPARAM(1),
            );
        }
    }

    pub fn clear_top_most(&self) {
        unsafe {
            let _ = PostMessageW(
                Some(self.hwnd()),
                WM_SET_TOPMOST_BORDER,
                WPARAM(0),
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
                if data_ptr == 0 {
                    return LRESULT(0);
                }
                let data = &mut *(data_ptr as *mut BorderOverlayData);

                if lparam.0 == 0 || wparam.0 == 0 {
                    data.border_info = None;
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER | SWP_HIDEWINDOW,
                    );
                } else {
                    let info = *Box::from_raw(wparam.0 as *mut BorderInfo);
                    let t = info.thickness as i32;

                    let wx = info.x - t;
                    let wy = info.y - t;
                    let ww = info.width + t * 2;
                    let wh = info.height + t * 2;
                    // let topwindow = WindowsAPI::top_window(&info.blacklist);
                    _ = SetWindowPos(
                        hwnd,
                        None,
                        wx,
                        wy,
                        ww,
                        wh,
                        SWP_NOACTIVATE | SWP_NOREDRAW | SWP_SHOWWINDOW | SWP_NOSENDCHANGING,
                    );
                    // let _ = SetWindowPos(
                    //     hwnd,
                    //     topwindow,
                    //     0,
                    //     0,
                    //     0,
                    //     0,
                    //     SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    // );
                    let _ = data.render_target.Resize(&D2D_SIZE_U {
                        width: ww as u32,
                        height: wh as u32,
                    });

                    data.border_info = Some(info);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }

            WM_SET_TOPMOST_BORDER => {
                let data_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                if data_ptr != 0 && wparam.0 != 0 {
                    let data = &mut *(data_ptr as *mut BorderOverlayData);
                    let info = *Box::from_raw(wparam.0 as *mut Vec<BorderInfo>);
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_TOPMOST),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                    data.topmost_border_info.clear();
                    data.topmost_border_info = info;
                    _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }

            WM_PAINT => {
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                if ptr != 0 {
                    let data = &*(ptr as *const BorderOverlayData);

                    data.render_target.BeginDraw();
                    data.render_target.Clear(Some(&D2D1_COLOR_F {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 0.0,
                    }));

                    // Render focus border first (if exists)
                    if let Some(ref info) = data.border_info {
                        let t = info.thickness;
                        let half = t / 2.0;
                        let rounded_rect = D2D1_ROUNDED_RECT {
                            rect: D2D_RECT_F {
                                left: half + t,
                                top: half + t,
                                right: info.width as f32 + t - half,
                                bottom: info.height as f32 + t - half,
                            },
                            radiusX: info.radius,
                            radiusY: info.radius,
                        };
                        if let Ok(brush) = data
                            .render_target
                            .CreateSolidColorBrush(&hex!(info.color), None)
                        {
                            data.render_target
                                .DrawRoundedRectangle(&rounded_rect, &brush, t, None);
                        }
                    }
                    if !data.topmost_border_info.is_empty() {
                        if let Ok(factory) = data.render_target.GetFactory() {
                            let factory: ID2D1Factory = factory.cast().unwrap();

                            let virt_x = data.virt_x;
                            let virt_y = data.virt_y;

                            for i in (0..data.topmost_border_info.len()).rev() {
                                let info = &data.topmost_border_info[i];
                                let clip_against: Vec<&BorderInfo> =
                                    data.topmost_border_info[0..i].iter().collect();

                                draw_border_clipped(
                                    &data.render_target,
                                    &factory,
                                    info,
                                    &clip_against,
                                    virt_x,
                                    virt_y,
                                );
                            }
                        }
                    }

                    let _ = data.render_target.EndDraw(None, None);
                }
                let _ = ValidateRect(Some(hwnd), None);
                LRESULT(0)
            }

            WM_SIZE => {
                let data_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                if data_ptr != 0 {
                    let data = &*(data_ptr as *const BorderOverlayData);
                    let w = (lparam.0 & 0xFFFF) as u32;
                    let h = ((lparam.0 >> 16) & 0xFFFF) as u32;
                    if w > 0 && h > 0 {
                        let _ = data.render_target.Resize(&D2D_SIZE_U {
                            width: w,
                            height: h,
                        });
                    }
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
fn draw_border_clipped(
    rt: &ID2D1HwndRenderTarget,
    factory: &ID2D1Factory,
    info: &BorderInfo,
    clip_against: &[&BorderInfo],
    virt_x: i32,
    virt_y: i32,
) {
    let color = D2D1_COLOR_F {
        r: ((info.color >> 16) & 0xFF) as f32 / 255.0,
        g: ((info.color >> 8) & 0xFF) as f32 / 255.0,
        b: (info.color & 0xFF) as f32 / 255.0,
        a: 1.0,
    };
    let Ok(brush) = (unsafe { rt.CreateSolidColorBrush(&color, None) }) else {
        return;
    };

    let x = (info.x - virt_x) as f32;
    let y = (info.y - virt_y) as f32;
    let w = info.width as f32;
    let h = info.height as f32;
    let t = info.thickness;
    let half = t / 2.0;

    // Build ring (outer - inner) using the exact same coordinates
    // Outer edge (outside of the stroke)
    let Ok(outer) = (unsafe {
        factory.CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: x - half,
                top: y - half,
                right: x + w + t,
                bottom: y + h + t,
            },
            radiusX: info.radius + half,
            radiusY: info.radius + half,
        })
    }) else {
        return;
    };

    // Inner edge (inside of the stroke)
    let Ok(inner) = (unsafe {
        factory.CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: x + half,
                top: y + half,
                right: x + w - half,
                bottom: y + h - half,
            },
            radiusX: (info.radius - half).max(0.0),
            radiusY: (info.radius - half).max(0.0),
        })
    }) else {
        return;
    };

    let ring_geos: [Option<ID2D1Geometry>; 2] = [Some(outer.into()), Some(inner.into())];
    let Ok(ring) = (unsafe { factory.CreateGeometryGroup(D2D1_FILL_MODE_ALTERNATE, &ring_geos) })
    else {
        return;
    };

    // Successively subtract each clip geometry
    let mut current: ID2D1Geometry = ring.into();
    for clip in clip_against {
        let cx = (clip.x - virt_x) as f32;
        let cy = (clip.y - virt_y) as f32;
        let cw = clip.width as f32;
        let ch = clip.height as f32;
        let ct = clip.thickness;
        let chalf = ct / 2.0;

        // Clip the entire area - using the outer bounds of the clip border
        let Ok(clip_geo) = (unsafe {
            factory.CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left: cx + ct - chalf,
                    top: cy + ct - chalf,
                    right: cx + cw + ct,
                    bottom: cy + ch + ct,
                },
                radiusX: clip.radius + chalf,
                radiusY: clip.radius + chalf,
            })
        }) else {
            continue;
        };

        let Ok(path) = (unsafe { factory.CreatePathGeometry() }) else {
            continue;
        };
        let Ok(sink) = (unsafe { path.Open() }) else {
            continue;
        };

        let clip_geo: ID2D1Geometry = clip_geo.into();
        unsafe {
            if current
                .CombineWithGeometry(&clip_geo, D2D1_COMBINE_MODE_EXCLUDE, None, 0.25, &sink)
                .is_err()
            {
                let _ = sink.Close();
                continue;
            }
            let _ = sink.Close();
        }
        current = path.into();
    }

    unsafe { rt.FillGeometry(&current, &brush, None) };
}
