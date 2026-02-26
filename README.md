window manager inspired by `powertoys`, `komorebi`, `niri`, and many more i could think of.
the code mostly wrote by LLM. so...

# BUG
- [ ] floating app messup up the cycle index

# TODO
- [x] add/close window should reorder layout
- [x] cycle size ratio should respect is the app on the screen or not
- [ ] add function to increase and decrease window width by pixel in no floating mode
- [ ] add function to move cursor into another monitor
- [ ] implement move app to another monitor
- [ ] better window re-arrange handling
- [ ] make widget setup can be order tru config
  currently we have :
    - workspace indicator
    - clock
    - cpu
    - ram
    - network
    - active app name
    - active app title
- [ ] make them widget clickable?
- [ ] update all command function to respect statusbar height


# AVAILABLE KEYS
```rust
"a"                         => Ok(TKey::A),
"b"                         => Ok(TKey::B),
"c"                         => Ok(TKey::C),
"d"                         => Ok(TKey::D),
"e"                         => Ok(TKey::E),
"f"                         => Ok(TKey::F),
"g"                         => Ok(TKey::G),
"h"                         => Ok(TKey::H),
"i"                         => Ok(TKey::I),
"j"                         => Ok(TKey::J),
"k"                         => Ok(TKey::K),
"l"                         => Ok(TKey::L),
"m"                         => Ok(TKey::M),
"n"                         => Ok(TKey::N),
"o"                         => Ok(TKey::O),
"p"                         => Ok(TKey::P),
"q"                         => Ok(TKey::Q),
"r"                         => Ok(TKey::R),
"s"                         => Ok(TKey::S),
"t"                         => Ok(TKey::T),
"u"                         => Ok(TKey::U),
"v"                         => Ok(TKey::V),
"w"                         => Ok(TKey::W),
"x"                         => Ok(TKey::X),
"y"                         => Ok(TKey::Y),
"z"                         => Ok(TKey::Z),
"0"                         => Ok(TKey::Num0),
"1"                         => Ok(TKey::Num1),
"2"                         => Ok(TKey::Num2),
"3"                         => Ok(TKey::Num3),
"4"                         => Ok(TKey::Num4),
"5"                         => Ok(TKey::Num5),
"6"                         => Ok(TKey::Num6),
"7"                         => Ok(TKey::Num7),
"8"                         => Ok(TKey::Num8),
"9"                         => Ok(TKey::Num9),
"kp0"                       => Ok(TKey::Kp0),
"kp1"                       => Ok(TKey::Kp1),
"kp2"                       => Ok(TKey::Kp2),
"kp3"                       => Ok(TKey::Kp3),
"kp4"                       => Ok(TKey::Kp4),
"kp5"                       => Ok(TKey::Kp5),
"kp6"                       => Ok(TKey::Kp6),
"kp7"                       => Ok(TKey::Kp7),
"kp8"                       => Ok(TKey::Kp8),
"kp9"                       => Ok(TKey::Kp9),
"kpreturn"                  => Ok(TKey::KpReturn),
"kpminus"                   => Ok(TKey::KpMinus),
"kpplus"                    => Ok(TKey::KpPlus),
"kpmultiply"                => Ok(TKey::KpMultiply),
"kpdivide"                  => Ok(TKey::KpDivide),
"kpdelete"                  => Ok(TKey::KpDelete),
"f1"                        => Ok(TKey::F1),
"f2"                        => Ok(TKey::F2),
"f3"                        => Ok(TKey::F3),
"f4"                        => Ok(TKey::F4),
"f5"                        => Ok(TKey::F5),
"f6"                        => Ok(TKey::F6),
"f7"                        => Ok(TKey::F7),
"f8"                        => Ok(TKey::F8),
"f9"                        => Ok(TKey::F9),
"f10"                       => Ok(TKey::F10),
"f11"                       => Ok(TKey::F11),
"f12"                       => Ok(TKey::F12),
"f13"                       => Ok(TKey::F13),
"f14"                       => Ok(TKey::F14),
"f15"                       => Ok(TKey::F15),
"f16"                       => Ok(TKey::F16),
"f17"                       => Ok(TKey::F17),
"f18"                       => Ok(TKey::F18),
"f19"                       => Ok(TKey::F19),
"f20"                       => Ok(TKey::F20),
"f21"                       => Ok(TKey::F21),
"f22"                       => Ok(TKey::F22),
"f23"                       => Ok(TKey::F23),
"f24"                       => Ok(TKey::F24),
"return" | "enter"          => Ok(TKey::Return),
"space"                     => Ok(TKey::Space),
"escape" | "esc"            => Ok(TKey::Escape),
"tab"                       => Ok(TKey::Tab),
"backspace"                 => Ok(TKey::Backspace),
"delete" | "del"            => Ok(TKey::Delete),
"insert"                    => Ok(TKey::Insert),
"up"                        => Ok(TKey::UpArrow),
"down"                      => Ok(TKey::DownArrow),
"left"                      => Ok(TKey::LeftArrow),
"right"                     => Ok(TKey::RightArrow),
"home"                      => Ok(TKey::Home),
"end"                       => Ok(TKey::End),
"pageup"                    => Ok(TKey::PageUp),
"pagedown"                  => Ok(TKey::PageDown),
"capslock"                  => Ok(TKey::CapsLock),
"numlock"                   => Ok(TKey::NumLock),
"scrolllock"                => Ok(TKey::ScrollLock),
"pause"                     => Ok(TKey::Pause),
"printscreen"               => Ok(TKey::PrintScreen),
"backquote"                 => Ok(TKey::BackQuote),
"minus"                     => Ok(TKey::Minus),
"equal"                     => Ok(TKey::Equal),
"leftbracket"               => Ok(TKey::LeftBracket),
"rightbracket"              => Ok(TKey::RightBracket),
"backslash"                 => Ok(TKey::BackSlash),
"intlbackslash"             => Ok(TKey::IntlBackslash),
"semicolon"                 => Ok(TKey::SemiColon),
"quote"                     => Ok(TKey::Quote),
"comma"                     => Ok(TKey::Comma),
"dot"                       => Ok(TKey::Dot),
"slash"                     => Ok(TKey::Slash),
"volumeup"                  => Ok(TKey::VolumeUp),
"volumedown"                => Ok(TKey::VolumeDown),
"volumemute"                => Ok(TKey::VolumeMute),
"brightnessup"              => Ok(TKey::BrightnessUp),
"brightnessdown"            => Ok(TKey::BrightnessDown),
"previoustrack"             => Ok(TKey::PreviousTrack),
"playpause"                 => Ok(TKey::PlayPause),
"playcd"                    => Ok(TKey::PlayCd),
"nexttrack"                 => Ok(TKey::NextTrack),
"function"                  => Ok(TKey::Function)
```