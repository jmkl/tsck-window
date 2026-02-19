use std::str::FromStr;

macro_rules! win_event_builder {
  ($event_name:ident , $( ($int_val:expr,  $str_val:expr, $enum_val:ident) ),* $(,)?) => {
        #[derive(Debug, Clone, Copy)]
        pub enum $event_name {
          $($enum_val),*
        }
        impl FromStr for $event_name{
          type Err = ();
          fn from_str(s:&str)->Result<Self,Self::Err>{
            match s{
              $( $str_val =>Ok(Self::$enum_val), )*
              _=>Err(())
            }
          }
        }
        impl $event_name{
          pub fn parse_event<'a>(id:u32)->&'a str{
            return match id {
              $($int_val => $str_val, )*
              _=>"Unknown"
            }
          }
        }

    };
}

win_event_builder! { WinEvent,
  (45055, "EVENT_AIA_END", AiaEnd),
  (40960, "EVENT_AIA_START", AiaStart),
  (16385, "EVENT_CONSOLE_CARET", ConsoleCaret),
  (16639, "EVENT_CONSOLE_END", ConsoleEnd),
  (16391, "EVENT_CONSOLE_END_APPLICATION", ConsoleEndApplication),
  (16389, "EVENT_CONSOLE_LAYOUT", ConsoleLayout),
  (16390, "EVENT_CONSOLE_START_APPLICATION", ConsoleStartApplication),
  (16386, "EVENT_CONSOLE_UPDATE_REGION", ConsoleUpdateRegion),
  (16388, "EVENT_CONSOLE_UPDATE_SCROLL", ConsoleUpdateScroll),
  (16387, "EVENT_CONSOLE_UPDATE_SIMPLE", ConsoleUpdateSimple),
  (32786, "EVENT_OBJECT_ACCELERATORCHANGE", ObjectAcceleratorchange),
  (32791, "EVENT_OBJECT_CLOAKED", ObjectCloaked),
  (32789, "EVENT_OBJECT_CONTENTSCROLLED", ObjectContentscrolled),
  (32768, "EVENT_OBJECT_CREATE", ObjectCreate),
  (32785, "EVENT_OBJECT_DEFACTIONCHANGE", ObjectDefactionchange),
  (32781, "EVENT_OBJECT_DESCRIPTIONCHANGE", ObjectDescriptionchange),
  (32769, "EVENT_OBJECT_DESTROY", ObjectDestroy),
  (32802, "EVENT_OBJECT_DRAGCANCEL", ObjectDragcancel),
  (32803, "EVENT_OBJECT_DRAGCOMPLETE", ObjectDragcomplete),
  (32806, "EVENT_OBJECT_DRAGDROPPED", ObjectDragdropped),
  (32804, "EVENT_OBJECT_DRAGENTER", ObjectDragenter),
  (32805, "EVENT_OBJECT_DRAGLEAVE", ObjectDragleave),
  (32801, "EVENT_OBJECT_DRAGSTART", ObjectDragstart),
  (33023, "EVENT_OBJECT_END", ObjectEnd),
  (32773, "EVENT_OBJECT_FOCUS", ObjectFocus),
  (32784, "EVENT_OBJECT_HELPCHANGE", ObjectHelpchange),
  (32771, "EVENT_OBJECT_HIDE", ObjectHide),
  (32800, "EVENT_OBJECT_HOSTEDOBJECTSINVALIDATED", ObjectHostedobjectsinvalidated),
  (32809, "EVENT_OBJECT_IME_CHANGE", ObjectImeChange),
  (32808, "EVENT_OBJECT_IME_HIDE", ObjectImeHide),
  (32807, "EVENT_OBJECT_IME_SHOW", ObjectImeShow),
  (32787, "EVENT_OBJECT_INVOKED", ObjectInvoked),
  (32793, "EVENT_OBJECT_LIVEREGIONCHANGED", ObjectLiveregionchanged),
  (32779, "EVENT_OBJECT_LOCATIONCHANGE", ObjectLocationchange),
  (32780, "EVENT_OBJECT_NAMECHANGE", ObjectNamechange),
  (32783, "EVENT_OBJECT_PARENTCHANGE", ObjectParentchange),
  (32772, "EVENT_OBJECT_REORDER", ObjectReorder),
  (32774, "EVENT_OBJECT_SELECTION", ObjectSelection),
  (32775, "EVENT_OBJECT_SELECTIONADD", ObjectSelectionadd),
  (32776, "EVENT_OBJECT_SELECTIONREMOVE", ObjectSelectionremove),
  (32777, "EVENT_OBJECT_SELECTIONWITHIN", ObjectSelectionwithin),
  (32770, "EVENT_OBJECT_SHOW", ObjectShow),
  (32778, "EVENT_OBJECT_STATECHANGE", ObjectStatechange),
  (32816, "EVENT_OBJECT_TEXTEDIT_CONVERSIONTARGETCHANGED", ObjectTexteditConversiontargetchanged),
  (32788, "EVENT_OBJECT_TEXTSELECTIONCHANGED", ObjectTextselectionchanged),
  (32792, "EVENT_OBJECT_UNCLOAKED", ObjectUncloaked),
  (32782, "EVENT_OBJECT_VALUECHANGE", ObjectValuechange),
  (511, "EVENT_OEM_DEFINED_END", OemDefinedEnd),
  (257, "EVENT_OEM_DEFINED_START", OemDefinedStart),
  (2, "EVENT_SYSTEM_ALERT", SystemAlert),
  (32790, "EVENT_SYSTEM_ARRANGMENTPREVIEW", SystemArrangmentpreview),
  (9, "EVENT_SYSTEM_CAPTUREEND", SystemCaptureend),
  (8, "EVENT_SYSTEM_CAPTURESTART", SystemCapturestart),
  (13, "EVENT_SYSTEM_CONTEXTHELPEND", SystemContexthelpend),
  (12, "EVENT_SYSTEM_CONTEXTHELPSTART", SystemContexthelpstart),
  (32, "EVENT_SYSTEM_DESKTOPSWITCH", SystemDesktopswitch),
  (17, "EVENT_SYSTEM_DIALOGEND", SystemDialogend),
  (16, "EVENT_SYSTEM_DIALOGSTART", SystemDialogstart),
  (15, "EVENT_SYSTEM_DRAGDROPEND", SystemDragdropend),
  (14, "EVENT_SYSTEM_DRAGDROPSTART", SystemDragdropstart),
  (255, "EVENT_SYSTEM_END", SystemEnd),
  (3, "EVENT_SYSTEM_FOREGROUND", SystemForeground),
  (41, "EVENT_SYSTEM_IME_KEY_NOTIFICATION", SystemImeKeyNotification),
  (5, "EVENT_SYSTEM_MENUEND", SystemMenuend),
  (7, "EVENT_SYSTEM_MENUPOPUPEND", SystemMenupopupend),
  (6, "EVENT_SYSTEM_MENUPOPUPSTART", SystemMenupopupstart),
  (4, "EVENT_SYSTEM_MENUSTART", SystemMenustart),
  (23, "EVENT_SYSTEM_MINIMIZEEND", SystemMinimizeend),
  (22, "EVENT_SYSTEM_MINIMIZESTART", SystemMinimizestart),
  (11, "EVENT_SYSTEM_MOVESIZEEND", SystemMovesizeend),
  (10, "EVENT_SYSTEM_MOVESIZESTART", SystemMovesizestart),
  (19, "EVENT_SYSTEM_SCROLLINGEND", SystemScrollingend),
  (18, "EVENT_SYSTEM_SCROLLINGSTART", SystemScrollingstart),
  (1, "EVENT_SYSTEM_SOUND", SystemSound),
  (21, "EVENT_SYSTEM_SWITCHEND", SystemSwitchend),
  (38, "EVENT_SYSTEM_SWITCHER_APPDROPPED", SystemSwitcherAppdropped),
  (36, "EVENT_SYSTEM_SWITCHER_APPGRABBED", SystemSwitcherAppgrabbed),
  (37, "EVENT_SYSTEM_SWITCHER_APPOVERTARGET", SystemSwitcherAppovertarget),
  (39, "EVENT_SYSTEM_SWITCHER_CANCELLED", SystemSwitcherCancelled),
  (20, "EVENT_SYSTEM_SWITCHSTART", SystemSwitchstart),
  (20223, "EVENT_UIA_EVENTID_END", UiaEventidEnd),
  (19968, "EVENT_UIA_EVENTID_START", UiaEventidStart),
  (30207, "EVENT_UIA_PROPID_END", UiaPropidEnd),
  (29952, "EVENT_UIA_PROPID_START", UiaPropidStart),
  (99999, "EVENT_DONE", Done),
}
