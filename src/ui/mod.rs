use ratatui::layout::{Constraint, Direction, Layout};

mod debug_view;
mod display;
mod footer;
mod main_view;
mod players_view;
mod replays_view;
mod search_view;
mod status;

use crate::app::{App, View};

pub fn render(frame: &mut ratatui::Frame, app: &mut App) {
    let size = frame.area();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(size);

    status::render_status(frame, layout[0], app);

    match app.view {
        View::Main => main_view::render_main(frame, layout[1], app),
        View::Debug => debug_view::render_debug(frame, layout[1], app),
        View::Search => search_view::render_search(frame, layout[1], app),
        View::Replays => replays_view::render_replays(frame, layout[1], app),
        View::Players => players_view::render_players(frame, layout[1], app),
    }

    footer::render_footer(frame, layout[2]);
}
