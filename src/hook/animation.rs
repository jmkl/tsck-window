#![allow(unused)]
use crate::hook::{
    app_info::{AppPosition, AppSize},
    win_api::{self, APP_WINDOW_PADDING},
};
use crate::hwnd;
use std::time::{Duration, Instant};
use windows::Win32::{
    Foundation::HWND,
    Graphics::Dwm::{DWMWA_TRANSITIONS_FORCEDISABLED, DwmSetWindowAttribute},
    UI::WindowsAndMessaging::{
        BeginDeferWindowPos, DeferWindowPos, EndDeferWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
    },
};

#[derive(Debug, Clone, PartialEq)]
pub enum AnimationEasing {
    EaseInSine,
    EaseOutSine,
    EaseInOutSine,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
    EaseInCubic,
    EaseOutCubic,
    EaseInOutCubic,
    EaseInQuart,
    EaseOutQuart,
    EaseInOutQuart,
    EaseInQuint,
    EaseOutQuint,
    EaseInOutQuint,
    EaseInExpo,
    EaseOutExpo,
    EaseInOutExpo,
    EaseInCirc,
    EaseOutCirc,
    EaseInOutCirc,
    EaseOutBack,
    EaseInOutBack,
    EaseOutElastic,
    EaseOutBounce,
    EaseInBounce,
}
impl AnimationEasing {
    pub fn evaluate(&self, t: f64) -> f64 {
        match self {
            AnimationEasing::EaseInSine => 1.0 - (t * std::f64::consts::FRAC_PI_2).cos(),
            AnimationEasing::EaseOutSine => (t * std::f64::consts::FRAC_PI_2).sin(),
            AnimationEasing::EaseInOutSine => -((t * std::f64::consts::PI).cos() - 1.0) / 2.0,
            AnimationEasing::EaseInQuad => t * t,
            AnimationEasing::EaseOutQuad => 1.0 - (1.0 - t) * (1.0 - t),
            AnimationEasing::EaseInOutQuad => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
            AnimationEasing::EaseInCubic => t * t * t,
            AnimationEasing::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
            AnimationEasing::EaseInOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            AnimationEasing::EaseInQuart => t * t * t * t,
            AnimationEasing::EaseOutQuart => 1.0 - (1.0 - t).powi(4),
            AnimationEasing::EaseInOutQuart => {
                if t < 0.5 {
                    8.0 * t * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(4) / 2.0
                }
            }
            AnimationEasing::EaseInQuint => t * t * t * t * t,
            AnimationEasing::EaseOutQuint => 1.0 - (1.0 - t).powi(5),
            AnimationEasing::EaseInOutQuint => {
                if t < 0.5 {
                    16.0 * t * t * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(5) / 2.0
                }
            }
            AnimationEasing::EaseInExpo => {
                if t == 0.0 {
                    0.0
                } else {
                    2.0f64.powf(10.0 * t - 10.0)
                }
            }
            AnimationEasing::EaseOutExpo => {
                if t == 1.0 {
                    1.0
                } else {
                    1.0 - 2.0f64.powf(-10.0 * t)
                }
            }
            AnimationEasing::EaseInOutExpo => {
                if t == 0.0 {
                    0.0
                } else if t == 1.0 {
                    1.0
                } else if t < 0.5 {
                    2.0f64.powf(20.0 * t - 10.0) / 2.0
                } else {
                    (2.0 - 2.0f64.powf(-20.0 * t + 10.0)) / 2.0
                }
            }
            AnimationEasing::EaseInCirc => 1.0 - (1.0 - t * t).sqrt(),
            AnimationEasing::EaseOutCirc => (1.0 - (t - 1.0).powi(2)).sqrt(),
            AnimationEasing::EaseInOutCirc => {
                if t < 0.5 {
                    (1.0 - (1.0 - (2.0 * t).powi(2)).sqrt()) / 2.0
                } else {
                    ((1.0 - (-2.0 * t + 2.0).powi(2)).sqrt() + 1.0) / 2.0
                }
            }
            AnimationEasing::EaseOutBack => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
            }
            AnimationEasing::EaseInOutBack => {
                let c1 = 1.70158;
                let c2 = c1 * 1.525;
                if t < 0.5 {
                    ((2.0 * t).powi(2) * ((c2 + 1.0) * 2.0 * t - c2)) / 2.0
                } else {
                    ((2.0 * t - 2.0).powi(2) * ((c2 + 1.0) * (t * 2.0 - 2.0) + c2) + 2.0) / 2.0
                }
            }
            AnimationEasing::EaseOutElastic => {
                const C4: f64 = (2.0 * std::f64::consts::PI) / 3.0;

                if t == 0.0 {
                    0.0
                } else if t == 1.0 {
                    1.0
                } else {
                    (2.0f64.powf(-10.0 * t) * ((t * 10.0 - 0.75) * C4).sin()) + 1.0
                }
            }
            AnimationEasing::EaseOutBounce => {
                let n1 = 7.5625;
                let d1 = 2.75;

                if t < 1.0 / d1 {
                    n1 * t * t
                } else if t < 2.0 / d1 {
                    let t = t - 1.5 / d1;
                    n1 * t * t + 0.75
                } else if t < 2.5 / d1 {
                    let t = t - 2.25 / d1;
                    n1 * t * t + 0.9375
                } else {
                    let t = t - 2.625 / d1;
                    n1 * t * t + 0.984375
                }
            }
            AnimationEasing::EaseInBounce => 1.0 - AnimationEasing::EaseOutBounce.evaluate(1.0 - t),
        }
    }
}
pub fn map_value(start: (i32, i32), end: (i32, i32), eased_t: f64) -> (i32, i32) {
    let new_x = start.0 as f64 + (end.0 - start.0) as f64 * eased_t;
    let new_y = start.1 as f64 + (end.1 - start.1) as f64 * eased_t;
    (new_x as i32, new_y as i32)
}

pub fn animate_window(
    hwnd: isize,
    pos: AppPosition,
    to_pos: AppPosition,
    size: AppSize,
    to_size: AppSize,
    easing: AnimationEasing,
) {
    std::thread::spawn(move || {
        let hwnd = hwnd!(hwnd);
        let start_time = Instant::now();
        let duration = Duration::from_millis(150);

        loop {
            let elapsed = start_time.elapsed();
            if elapsed >= duration {
                break;
            }

            let t = elapsed.as_secs_f64() / duration.as_secs_f64();
            let eased_t = easing.evaluate(t.min(1.0));

            let new_pos = map_value((pos.x, pos.y), (to_pos.x, to_pos.y), eased_t);
            let new_size = map_value(
                (size.width as i32, size.height as i32),
                (to_size.width, to_size.height),
                eased_t,
            );

            unsafe {
                let hdwp = BeginDeferWindowPos(1);
                if let Ok(hdwp) = hdwp {
                    let _ = DeferWindowPos(
                        hdwp,
                        hwnd,
                        None,
                        new_pos.0 + APP_WINDOW_PADDING,
                        new_pos.1 + APP_WINDOW_PADDING,
                        (new_size.0 - APP_WINDOW_PADDING * 2).max(0),
                        (new_size.1 - APP_WINDOW_PADDING * 2).max(0),
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                    EndDeferWindowPos(hdwp);
                }
            }

            std::thread::sleep(Duration::from_millis(1000 / 60));
        }

        win_api::set_app_size_position(
            hwnd,
            to_pos.x,
            to_pos.y,
            to_size.width,
            to_size.height,
            true,
        );
    });
}
