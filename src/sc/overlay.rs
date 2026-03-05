use anyhow::{Result, anyhow};
use std::{ffi::c_void, sync::OnceLock};
use windows::{
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::{
            Direct2D::{
                Common::{
                    D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F,
                    D2D1_PIXEL_FORMAT,
                },
                D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
                D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_IMMEDIATELY,
                D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
                D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1CreateFactory, ID2D1Factory,
                ID2D1HwndRenderTarget,
            },
            Dwm::DwmExtendFrameIntoClientArea,
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            Gdi::{InvalidateRect, ValidateRect},
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::MARGINS,
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetSystemMetrics,
                GetWindowLongPtrW, HTTRANSPARENT, LWA_ALPHA, PostMessageW, RegisterClassExW,
                SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
                SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOREDRAW, SWP_NOSENDCHANGING,
                SWP_NOZORDER, SWP_SHOWWINDOW, SetLayeredWindowAttributes, SetWindowLongPtrW,
                SetWindowPos, WM_DESTROY, WM_ERASEBKGND, WM_NCHITTEST, WM_PAINT, WM_SIZE, WM_USER,
                WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
                WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
            },
        },
    },
    core::{Error, PCWSTR, w},
};

pub const WM_UPDATE_OVERLAY: u32 = WM_USER + 1;

pub struct FrameOverlay {
    hwnd: isize,
}
impl FrameOverlay {
    pub fn new() -> Result<Self> {
        let hinstance: HINSTANCE = unsafe { GetModuleHandleW(None)?.into() };
        let class_name = format!("Frame-Overlay\0");
        let class_name_ptr = class_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(window_proc),
            hInstance: hinstance,
            lpszClassName: PCWSTR(class_name_ptr.as_ptr()),
            ..Default::default()
        };
        let atom = unsafe { RegisterClassExW(&wc) };
        if atom == 0 {
            panic!("RegisterClassExW failed: {:?}", Error::from_thread());
        }

        let (virt_x, virt_y, virt_w, virt_h) = unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        };
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE
                    | WS_EX_TRANSPARENT,
                PCWSTR(class_name_ptr.as_ptr()),
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
        println!("WINDOW CREATED {}", hwnd.0 as isize);

        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins)? };
        unsafe { SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)? };
        let render_target = create_render_target(hwnd, virt_w, virt_h)?;
        let data = Box::new(BorderPainterData {
            render_target,
            border_info: None,
            virt_x,
            virt_y,
        });

        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(data) as isize);
        }

        Ok(Self {
            hwnd: hwnd.0 as isize,
        })
    }
}

unsafe extern "system" fn window_proc(
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
                let data = &mut *(data_ptr as *mut BorderPainterData);

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

                    // let wx = info.x - t;
                    // let wy = info.y - t;
                    // let ww = info.width + t * 2;
                    // let wh = info.height + t * 2;
                    let wx = info.x - t;
                    let wy = info.y - t;
                    let ww = info.width + t * 2;
                    let wh = info.height + t * 2;
                    _ = SetWindowPos(
                        hwnd,
                        None,
                        wx,
                        wy,
                        ww,
                        wh,
                        SWP_NOACTIVATE | SWP_NOREDRAW | SWP_SHOWWINDOW | SWP_NOSENDCHANGING,
                    );

                    let _ = data.render_target.Resize(&D2D_SIZE_U {
                        width: ww as u32,
                        height: wh as u32,
                    });

                    data.border_info = Some(info);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }

            WM_PAINT => {
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                if ptr != 0 {
                    let data = &*(ptr as *const BorderPainterData);

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
                        let (left, top, right, bottom) = if info.top_most {
                            (t, t, info.width as f32 + t, info.height as f32 + t)
                        } else {
                            (
                                half + t,
                                half + t,
                                info.width as f32 + t - half,
                                info.height as f32 + t - half,
                            )
                        };
                        let rounded_rect = D2D1_ROUNDED_RECT {
                            rect: D2D_RECT_F {
                                left,
                                top,
                                right,
                                bottom,
                            },
                            radiusX: info.radius,
                            radiusY: info.radius,
                        };
                        if let Ok(brush) =
                            data.render_target.CreateSolidColorBrush(&info.color, None)
                        {
                            data.render_target
                                .DrawRoundedRectangle(&rounded_rect, &brush, t, None);
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
                    let data = &*(data_ptr as *const BorderPainterData);
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
                    let _ = Box::from_raw(ptr as *mut BorderPainterData);
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
