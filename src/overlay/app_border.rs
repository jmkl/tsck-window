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
    factory: ID2D1Factory,
    render_target: ID2D1HwndRenderTarget,
    // (hwnd_key, border_info) so we can skip it in topmost rendering
    focus_border: Option<(isize, BorderInfo)>,
    // always-on-top windows borders, keyed by hwnd
    topmost_borders: std::collections::HashMap<isize, BorderInfo>,
    // virtual screen origin for coordinate offset
    virt_x: i32,
    virt_y: i32,
}

pub struct BorderOverlay {
    hwnd: isize,
    data: *mut BorderOverlayData,
}

unsafe impl Send for BorderOverlay {}
unsafe impl Sync for BorderOverlay {}

impl BorderOverlay {
    pub fn new() -> anyhow::Result<Self> {
        let hinstance: HINSTANCE = unsafe { GetModuleHandleW(None)?.into() };

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
            factory: d2d_factory,
            render_target,
            focus_border: None,
            topmost_borders: std::collections::HashMap::new(),
            virt_x,
            virt_y,
        });
        let data_ptr = Box::into_raw(data);

        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, data_ptr as isize);
            _ = InvalidateRect(Some(hwnd), None, false);
        }

        Ok(Self {
            hwnd: hwnd.0 as isize,
            data: data_ptr,
        })
    }

    pub fn hwnd(&self) -> HWND {
        HWND(self.hwnd as *mut _)
    }

    /// Set the focused window border with its hwnd key
    pub fn set_focus(&self, hwnd_key: isize, info: BorderInfo) {
        unsafe {
            let payload = Box::new((hwnd_key, info));
            let _ = PostMessageW(
                Some(self.hwnd()),
                WM_SET_FOCUS_BORDER,
                WPARAM(Box::into_raw(payload) as usize),
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
                        let payload = Box::from_raw(wparam.0 as *mut (isize, BorderInfo));
                        data.focus_border = Some(*payload);
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

                    let focused_key = data.focus_border.as_ref().map(|(k, _)| *k);
                    let focus_is_topmost = focused_key
                        .map(|k| data.topmost_borders.contains_key(&k))
                        .unwrap_or(false);

                    let focused_topmost: Option<&BorderInfo> =
                        focused_key.and_then(|k| data.topmost_borders.get(&k));

                    // non-focused topmosts in a stable order
                    let non_focused_topmosts: Vec<&BorderInfo> = data
                        .topmost_borders
                        .iter()
                        .filter(|(k, _)| Some(**k) != focused_key)
                        .map(|(_, v)| v)
                        .collect();

                    let all_topmosts: Vec<&BorderInfo> = non_focused_topmosts
                        .iter()
                        .chain(focused_topmost.iter())
                        .copied()
                        .collect();

                    // 1. non-topmost focus → clipped by ALL topmost
                    if !focus_is_topmost {
                        if let Some((_, ref info)) = data.focus_border {
                            draw_border_clipped(
                                &data.render_target,
                                &data.factory,
                                info,
                                &all_topmosts,
                                data.virt_x,
                                data.virt_y,
                            );
                        }
                    }

                    // 2. non-focused topmosts → each clipped by all topmosts drawn AFTER it
                    //    (clip each other by draw order) + clipped by focused topmost
                    for i in 0..non_focused_topmosts.len() {
                        let info = non_focused_topmosts[i];

                        // everything after it in the list + focused topmost
                        let mut clip: Vec<&BorderInfo> = non_focused_topmosts[i + 1..].to_vec();
                        if let Some(ft) = focused_topmost {
                            clip.push(ft);
                        }

                        draw_border_clipped(
                            &data.render_target,
                            &data.factory,
                            info,
                            &clip,
                            data.virt_x,
                            data.virt_y,
                        );
                    }

                    // 3. focused topmost → always full, drawn last
                    if let Some(info) = focused_topmost {
                        draw_border_clipped(
                            &data.render_target,
                            &data.factory,
                            info,
                            &[],
                            data.virt_x,
                            data.virt_y,
                        );
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

    // build ring (outer - inner, even-odd)
    let Ok(outer) = (unsafe {
        factory.CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            },
            radiusX: info.radius,
            radiusY: info.radius,
        })
    }) else {
        return;
    };

    let Ok(inner) = (unsafe {
        factory.CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: x + t,
                top: y + t,
                right: x + w - t,
                bottom: y + h - t,
            },
            radiusX: (info.radius - t).max(0.0),
            radiusY: (info.radius - t).max(0.0),
        })
    }) else {
        return;
    };

    let ring_geos: [Option<ID2D1Geometry>; 2] = [Some(outer.into()), Some(inner.into())];
    let Ok(ring) = (unsafe { factory.CreateGeometryGroup(D2D1_FILL_MODE_ALTERNATE, &ring_geos) })
    else {
        return;
    };

    // successively subtract each clip geometry
    let mut current: ID2D1Geometry = ring.into();

    for clip in clip_against {
        let cx = (clip.x - virt_x) as f32;
        let cy = (clip.y - virt_y) as f32;
        let cw = clip.width as f32;
        let ch = clip.height as f32;

        let Ok(clip_geo) = (unsafe {
            factory.CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left: cx,
                    top: cy,
                    right: cx + cw,
                    bottom: cy + ch,
                },
                radiusX: clip.radius,
                radiusY: clip.radius,
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
                continue;
            }
            let _ = sink.Close();
        }
        current = path.into();
    }

    unsafe { rt.FillGeometry(&current, &brush, None) };
}

// keep original draw_border for the focus window (stroke only, simple)
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
fn draw_all_borders(
    rt: &ID2D1HwndRenderTarget,
    factory: &ID2D1Factory,
    borders: &[&BorderInfo],
    virt_x: i32,
    virt_y: i32,
) {
    // build outer geometry for each border (the full rect, not just stroke)
    let mut outer_geos: Vec<ID2D1RoundedRectangleGeometry> = borders
        .iter()
        .map(|info| {
            let x = (info.x - virt_x) as f32;
            let y = (info.y - virt_y) as f32;
            let w = info.width as f32;
            let h = info.height as f32;
            unsafe {
                factory
                    .CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: x,
                            top: y,
                            right: x + w,
                            bottom: y + h,
                        },
                        radiusX: info.radius,
                        radiusY: info.radius,
                    })
                    .unwrap()
            }
        })
        .collect();

    for (i, info) in borders.iter().enumerate() {
        let color = D2D1_COLOR_F {
            r: ((info.color >> 16) & 0xFF) as f32 / 255.0,
            g: ((info.color >> 8) & 0xFF) as f32 / 255.0,
            b: (info.color & 0xFF) as f32 / 255.0,
            a: 1.0,
        };
        let Ok(brush) = (unsafe { rt.CreateSolidColorBrush(&color, None) }) else {
            continue;
        };

        let x = (info.x - virt_x) as f32;
        let y = (info.y - virt_y) as f32;
        let w = info.width as f32;
        let h = info.height as f32;
        let t = info.thickness;

        // outer rect geometry
        let outer = unsafe {
            factory
                .CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: x,
                        top: y,
                        right: x + w,
                        bottom: y + h,
                    },
                    radiusX: info.radius,
                    radiusY: info.radius,
                })
                .unwrap()
        };

        // inner rect geometry (the hole)
        let inner = unsafe {
            factory
                .CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: x + t,
                        top: y + t,
                        right: x + w - t,
                        bottom: y + h - t,
                    },
                    radiusX: (info.radius - t).max(0.0),
                    radiusY: (info.radius - t).max(0.0),
                })
                .unwrap()
        };

        // ring = outer XOR inner (even-odd)
        let ring_geos: [Option<ID2D1Geometry>; 2] = [Some(outer.into()), Some(inner.into())];
        let Ok(ring) =
            (unsafe { factory.CreateGeometryGroup(D2D1_FILL_MODE_ALTERNATE, &ring_geos) })
        else {
            continue;
        };

        // now clip this ring against all OTHER borders' outer rects using CombineGeometry
        // subtract every other border's outer rect from this ring
        let mut clipped: ID2D1Geometry = ring.into();

        for (j, other_outer) in outer_geos.iter().enumerate() {
            if i == j {
                continue;
            }

            let Ok(combined) = (unsafe {
                let path: ID2D1PathGeometry = factory.CreatePathGeometry().unwrap();
                let sink = path.Open().unwrap();
                let other_geo: ID2D1Geometry = other_outer.clone().into();
                clipped
                    .CombineWithGeometry(&other_geo, D2D1_COMBINE_MODE_EXCLUDE, None, 0.25, &sink)
                    .unwrap();
                sink.Close().unwrap();
                Ok::<_, ()>(path)
            }) else {
                continue;
            };

            clipped = combined.into();
        }

        unsafe { rt.FillGeometry(&clipped, &brush, None) };
    }
}
