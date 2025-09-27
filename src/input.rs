use crate::app::App;
use crate::interaction::Intent;
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

#[allow(clippy::collapsible_if)]
pub fn handle_key_event(app: &mut App, key: KeyEvent) {
    if key.kind != KeyEventKind::Press {
        return;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('d') => Intent::ToggleDebug.apply(app),
            KeyCode::Char('s') => Intent::ShowSearch.apply(app),
            KeyCode::Char('m') => Intent::ShowMain.apply(app),
            KeyCode::Char('r') => Intent::ShowReplays.apply(app),
            KeyCode::Char('p') => Intent::ShowPlayers.apply(app),
            KeyCode::Char('q') => Intent::Quit.apply(app),
            _ => {}
        }
    } else {
        match key.code {
            KeyCode::Esc => Intent::Quit.apply(app),
            other => app.on_key(other),
        }
    }
}

#[allow(clippy::collapsible_if)]
pub fn handle_mouse_event(app: &mut App, me: MouseEvent) {
    if let MouseEventKind::Down(MouseButton::Left) = me.kind {
        let x = me.column;
        let y = me.row;
        match app.view {
            crate::app::View::Main => {
                if let Some(intent) = crate::ui::main_view::intent_at(app, x, y) {
                    intent.apply(app);
                }
            }
            crate::app::View::Search => {
                if let Some(intent) = crate::ui::search_view::intent_at(app, x, y) {
                    intent.apply(app);
                }
            }
            crate::app::View::Debug => {}
            crate::app::View::Replays => {}
            crate::app::View::Players => {}
        }
    }
}
