use crate::app::{App, View};

#[derive(Debug, Clone)]
pub enum Intent {
    Quit,
    ToggleDebug,
    ShowMain,
    ShowReplays,
    AdjustDebugScroll { delta: i32 },
    SetDebugScroll { value: i32 },
}

impl Intent {
    pub fn apply(self, app: &mut App) {
        match self {
            Intent::Quit => {
                app.should_quit = true;
            }
            Intent::ToggleDebug => {
                app.view = match app.view {
                    View::Debug => View::Main,
                    _ => View::Debug,
                };
                if app.view == View::Debug {
                    app.debug.scroll = 0;
                }
            }
            Intent::ShowMain => {
                app.view = View::Main;
            }
            Intent::ShowReplays => {
                app.view = View::Replays;
                if app.replay.toon_input.is_empty()
                    && let Some(name) = &app.self_profile.name
                {
                    app.replay.toon_input = name.clone();
                    app.replay.toon_cursor = app.replay.toon_input.chars().count();
                }
                if let Some(gw) = app.self_profile.gateway {
                    app.replay.input_gateway = gw;
                }
                app.replay.focus = crate::app::ReplayFocus::Toon;
                app.replay.last_error = None;
            }
            Intent::AdjustDebugScroll { delta } => {
                if app.view == View::Debug {
                    let current = app.debug.scroll as i32;
                    let next = (current + delta).max(0) as u16;
                    app.debug.scroll = next;
                }
            }
            Intent::SetDebugScroll { value } => {
                if app.view == View::Debug {
                    if value == i32::MAX {
                        app.debug.scroll = u16::MAX;
                    } else {
                        app.debug.scroll = value.max(0) as u16;
                    }
                }
            }
        }
    }
}
