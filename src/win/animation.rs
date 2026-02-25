#![allow(unused)]
use crate::win::winapi::{self, AppData, AppRect, WindowsAPI};
use std::time::{Duration, Instant};
use windows::Win32::{
    Foundation::HWND,
    Graphics::Dwm::{DWMWA_TRANSITIONS_FORCEDISABLED, DwmSetWindowAttribute},
    UI::WindowsAndMessaging::{
        BeginDeferWindowPos, DeferWindowPos, EndDeferWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
    },
};
#[derive(Clone, Debug, PartialEq)]
pub struct CubicBezier {
    pub p1x: f64,
    pub p1y: f64,
    pub p2x: f64,
    pub p2y: f64,
}

impl CubicBezier {
    pub fn new(p1x: f64, p1y: f64, p2x: f64, p2y: f64) -> Self {
        Self { p1x, p1y, p2x, p2y }
    }

    // Preset equivalents to CSS easings
    pub fn ease() -> Self {
        Self::new(0.25, 0.1, 0.25, 1.0)
    }
    pub fn ease_in() -> Self {
        Self::new(0.42, 0.0, 1.0, 1.0)
    }
    pub fn ease_out() -> Self {
        Self::new(0.0, 0.0, 0.58, 1.0)
    }
    pub fn ease_in_out() -> Self {
        Self::new(0.42, 0.0, 0.58, 1.0)
    }
    pub fn spring() -> Self {
        Self::new(0.34, 1.56, 0.64, 1.0)
    }
    pub fn bounce() -> Self {
        Self::new(0.34, 1.8, 0.64, 1.0)
    }

    // Sample the X component of the bezier curve at parameter t
    fn sample_x(&self, t: f64) -> f64 {
        let mt = 1.0 - t;
        3.0 * mt * mt * t * self.p1x + 3.0 * mt * t * t * self.p2x + t * t * t
    }

    // Sample the Y component (the eased value)
    fn sample_y(&self, t: f64) -> f64 {
        let mt = 1.0 - t;
        3.0 * mt * mt * t * self.p1y + 3.0 * mt * t * t * self.p2y + t * t * t
    }

    // Derivative of X — used for Newton's method
    fn sample_x_derivative(&self, t: f64) -> f64 {
        let mt = 1.0 - t;
        3.0 * (mt * mt * self.p1x + 2.0 * mt * t * (self.p2x - self.p1x) + t * t * (1.0 - self.p2x))
    }

    // Given input x (0..1), find the bezier parameter t via Newton's method
    // then sample Y at that t
    pub fn evaluate(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }

        // Newton-Raphson to find t where sample_x(t) == x
        let mut t = x; // initial guess
        for _ in 0..8 {
            let x_err = self.sample_x(t) - x;
            if x_err.abs() < 1e-7 {
                break;
            }
            let dx = self.sample_x_derivative(t);
            if dx.abs() < 1e-6 {
                break;
            }
            t -= x_err / dx;
        }

        self.sample_y(t)
    }
}

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
    CubicBezier(CubicBezier),
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
            AnimationEasing::CubicBezier(b) => b.evaluate(t),
        }
    }
}
pub fn map_value(start: &AppRect, end: &AppRect, eased_t: f64) -> AppRect {
    let new_x = start.l as f64 + (end.l - start.l) as f64 * eased_t;
    let new_y = start.t as f64 + (end.t - start.t) as f64 * eased_t;
    let new_width = start.width as f64 + (end.width - start.width) as f64 * eased_t;
    let new_height = start.height as f64 + (end.height - start.height) as f64 * eased_t;

    AppRect {
        l: new_x as i32,
        t: new_y as i32,
        r: (new_x + new_width) as i32,
        b: (new_y + new_height) as i32,
        width: new_width as i32,
        height: new_height as i32,
    }
}

pub fn animate_window(hwnd: isize, rect: &AppRect, to_rect: &AppRect) {
    let easing = AnimationEasing::CubicBezier(CubicBezier::bounce());
    let rect = rect.clone();
    let to_rect = to_rect.clone();
    std::thread::spawn(move || {
        let hwnd_raw = crate::h!(hwnd);
        let duration = Duration::from_millis(150);
        let start_time = Instant::now();

        loop {
            let elapsed = start_time.elapsed();
            let t = (elapsed.as_secs_f64() / duration.as_secs_f64()).min(1.0);
            let eased_t = easing.evaluate(t);

            let new_rect = map_value(&rect, &to_rect, eased_t);
            WindowsAPI::transform_to(hwnd, &new_rect);

            if t >= 1.0 {
                break;
            }

            let frame_duration = Duration::from_micros(16_667);
            let next_frame = start_time
                + Duration::from_micros(
                    (start_time.elapsed().as_micros() as u64 / 16_667 + 1) * 16_667,
                );
            let now = Instant::now();
            if next_frame > now {
                std::thread::sleep(next_frame - now);
            }
        }
        WindowsAPI::transform_to(hwnd, &to_rect);
    });
}
