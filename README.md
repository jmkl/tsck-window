window manager inspired by `powertoys`, `komorebi`, `niri`, and many more i could think of.
the code mostly wrote by LLM. so...

# TODO
- [ ] implement move app to another monitor
- [ ] better window re-arrange handling
- [ ] make widget setup can be order tru config
  currently we have :
    - [ ] workspace indicator
    - [ ] clock
    - [ ] cpu
    - [ ] ram
    - [ ] network
    - [ ] active app name
    - [ ] active app title
- [ ] make them widget clickable?
- [ ] update all command function to respect statusbar height

## **CONFIG**
```toml
# this is for W:CycleAppHeight and W:CycleAppWidth
size_factor = [1.0, 0.75, 0.666666, 0.5, 0.333333, 0.25]
# this is for W: CycleAppOnGrid
workspace_grid = [
      { width: 1.0, height: 1.0, x: 0.0,  y  : 0.0   },
      { width: 0.5, height: 1.0, x: 0.0,  y  : 0.0   },
      { width: 0.5, height: 1.0, x: 0.5,  y  : 0.0   },
      { width: 0.5, height: 0.5, x: 0.0,  y  : 0.0   },
      { width: 0.5, height: 0.5, x: 0.5,  y  : 0.0   },
      { width: 0.5, height: 0.5, x: 0.0,  y  : 0.5   },
      { width: 0.5, height: 0.5, x: 0.5,  y  : 0.5   },
      { width: 0.8, height: 1.0, x: 0.2,  y  : 0.0   },
      { width: 0.5, height: 0.5, x: 0.25, y  : 0.25  },

]
# this is for W:CycleAppHeight and W:CycleAppWidth
size_factor = [1.0, 0.75, 0.666666, 0.5, 0.333333, 0.25]
# this is for W: CycleAppOnGrid
workspace_grid = [
      { width: 1.0, height: 1.0, x: 0.0,  y  : 0.0   },
      { width: 0.5, height: 1.0, x: 0.0,  y  : 0.0   },
      { width: 0.5, height: 1.0, x: 0.5,  y  : 0.0   },
      { width: 0.5, height: 0.5, x: 0.0,  y  : 0.0   },
      { width: 0.5, height: 0.5, x: 0.5,  y  : 0.0   },
      { width: 0.5, height: 0.5, x: 0.0,  y  : 0.5   },
      { width: 0.5, height: 0.5, x: 0.5,  y  : 0.5   },
      { width: 0.8, height: 1.0, x: 0.2,  y  : 0.0   },
      { width: 0.5, height: 0.5, x: 0.25, y  : 0.25  },

]
workspaces = [
  "Work",
  "Browsing",
  "Files",
]
blacklist = [
    "tsck.exe",
    "TextInputHost.exe",
    "msedgewebview2.exe",
    "Microsoft.CmdPal.UI.exe",
    "StartMenuExperienceHost.exe",
    "SearchHost.exe"
]
move_inc = 50
size_inc = 50

hotkeys = {
  C-S-right         : W::MoveActiveApp(Right),
  C-S-A-pagedown    : W::MoveToWorkspace(Next),
  C-S-A-pageup      : W::MoveToWorkspace(Prev),
  C-S-pagedown      : W::GoToWorkspace(Next),
  C-S-pageup        : W::GoToWorkspace(Prev),
  C-S-down          : W::MoveActiveApp(Down),
  C-S-left          : W::MoveActiveApp(Left),
  C-S-up            : W::MoveActiveApp(Up),
  C-S-A-right       : W::ResizeActiveApp(Right),
  C-S-A-down        : W::ResizeActiveApp(Down),
  C-S-A-left        : W::ResizeActiveApp(Left),
  C-S-A-up          : W::ResizeActiveApp(Up),
  C-S-d             : W::Debug,
  C-S-t             : W::ToggleTopMost,
  C-S-p             : W::CycleAppOnGrid,
  C-S-k             : W::CycleAppHeight(Next),
  C-S-j             : W::CycleAppHeight(Prev),
  C-S-h             : W::CycleAppWidth(Prev),
  C-S-l             : W::CycleAppWidth(Next),
  C-S-c             : W::CycleColumn,
  C-S-w             : W::CloseActiveApp,
  C-S-comma         : W::CycleActiveApp(Prev),
  C-S-dot           : W::CycleActiveApp(Next),
}
```
## **HOTKEYS**
| Hotkey | Command | Description |
|--------|---------|-------------|
| C-S-right        |  W::MoveActiveApp(Right)     | move active app to right [inc]px  |
| C-S-A-pagedown   |  W::MoveToWorkspace(Next)    | move app to next workspace |
| C-S-A-pageup     |  W::MoveToWorkspace(Prev)    | move app to prev workspace |
| C-S-pagedown     |  W::GoToWorkspace(Next)  | activate next workspace |
| C-S-pageup       |  W::GoToWorkspace(Prev)  | activate previous workspace |
| C-S-down         |  W::MoveActiveApp(Down)      | move active app to down [inc]px  |
| C-S-left         |  W::MoveActiveApp(Left)      | move active app to left [inc]px  |
| C-S-up           |  W::MoveActiveApp(Up)        | move active app to up [inc]px  |
| C-S-A-right      |  W::ResizeActiveApp(Right)   | increate width |
| C-S-A-down       |  W::ResizeActiveApp(Down)    | increase height |
| C-S-A-left       |  W::ResizeActiveApp(Left)    | decrease width |
| C-S-A-up         |  W::ResizeActiveApp(Up)      | decreate height |
| C-S-d            |  W::Debug                    | debugging purpose |
| C-S-p            |  W::CycleAppOnGrid           | cycle app size with serial of size an position |
| C-S-k            |  W::CycleAppHeight(Next)     | cycle app height within the collection op scale 0.0 - 1.0 |
| C-S-j            |  W::CycleAppHeight(Prev)     | sda |
| C-S-h            |  W::CycleAppWidth(Prev)      | sda |
| C-S-l            |  W::CycleAppWidth(Next)      | sda |
| C-S-c            |  W::CycleColumn              | move app to left/right within the grid |
| C-S-w            |  W::CloseActiveApp           | close active app |
| C-S-comma        |  W::CycleActiveApp(Prev)     | cycle active app within the workspace |
| C-S-dot          |  W::CycleActiveApp(Next)     |  sda |