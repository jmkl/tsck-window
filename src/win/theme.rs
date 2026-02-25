use std::sync::OnceLock;

use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

pub static THEME: OnceLock<Theme> = OnceLock::new();

pub fn th() -> &'static Theme {
    THEME.get_or_init(|| Theme::default())
}
#[macro_export]
macro_rules! hex {
    ($col:expr) => {{
        let a = (($col >> 24) & 0xFF) as f32 / 255.0;
        windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
            r: (($col >> 16) & 0xFF) as f32 / 255.0,
            g: (($col >> 8) & 0xFF) as f32 / 255.0,
            b: ($col & 0xFF) as f32 / 255.0,
            a: if a > 0.0 { a } else { 1.0 },
        }
    }};
}

macro_rules! theme_generator {
    ($struct_name:ident,
      $( $name:ident => $value:expr, )* $(,)?) => {
      pub struct $struct_name{
        $(
        pub $name:u32,
        )*
      }
      impl Default for $struct_name{
        fn default()->Self{
          Self{ $($name:$value,)*}
        }
      }
      impl $struct_name{
        $(
          pub fn $name(&self)->D2D1_COLOR_F{
            hex!(self.$name)
          }
        )*
      }
    };
}

theme_generator!(Theme,
  base_100          => 0x1F2937,
  base_200          => 0x1C2431,
  base_300          => 0x191F2B,
  base_trans        => 0x55000000,
  base_content      => 0xF4F7FF,
  primary           => 0x7C3AED,
  primary_content   => 0xF1EFFF,
  dim_content       => 0x44F1EFFF,
  secondary         => 0xF43F5E,
  secondary_content => 0xFFE4E9,
  accent            => 0x2DD4BF,
  accent_content    => 0x0F3F3A,
  neutral           => 0x111827,
  neutral_content   => 0xE5E7EB,
  info              => 0x38BDF8,
  info_content      => 0x0C4A6E,
  success           => 0x22C55E,
  success_content   => 0x14532D,
  warning           => 0xFACC15,
  warning_content   => 0x78350F,
  error             => 0xEF4444,
  error_content     => 0x7F1D1D,
);

#[macro_export]
macro_rules! col {
    ($color:ident) => {{ crate::win::theme::th().$color() }};
}
