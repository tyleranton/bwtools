use crate::app::{App, View};

#[derive(Debug, Clone)]
pub enum Intent {
    Quit,
    ToggleDebug,
    ShowSearch,
    ShowMain,
    ShowReplays,
    ShowPlayers,
    BeginSearch { name: String, gateway: u16 },
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
            Intent::ShowSearch => {
                app.view = View::Search;
                app.search.focus_gateway = false;
                app.search.cursor = app.search.name.chars().count();
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
            Intent::ShowPlayers => {
                app.view = View::Players;
                app.players.scroll = 0;
                app.players.search_cursor = app.players.search_query.chars().count();
            }
            Intent::BeginSearch { name, gateway } => {
                app.begin_search(name, gateway);
            }
        }
    }
}
