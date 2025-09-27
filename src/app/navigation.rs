use crossterm::event::KeyCode;

use crate::interaction::Intent;

use super::{App, View};

impl App {
    #[allow(clippy::collapsible_if)]
    pub fn on_key(&mut self, code: KeyCode) {
        match self.view {
            View::Replays => {
                self.handle_replay_key(code);
                return;
            }
            View::Search => {
                self.handle_search_key(code);
                return;
            }
            View::Players => {
                if self.handle_players_key(code) {
                    return;
                }
            }
            View::Main | View::Debug => {}
        }

        if let Some(intent) = global_intent(self.view, code, self.players.filtered.len()) {
            intent.apply(self);
        }

        if matches!(self.view, View::Players) {
            self.clamp_players_scroll();
        }
    }
}

fn global_intent(view: View, code: KeyCode, players_len: usize) -> Option<Intent> {
    match view {
        View::Debug => debug_intent(code),
        View::Players => player_intent(code, players_len),
        _ => None,
    }
}

fn debug_intent(code: KeyCode) -> Option<Intent> {
    match code {
        KeyCode::Up | KeyCode::Char('k') => Some(Intent::AdjustDebugScroll { delta: -1 }),
        KeyCode::Down | KeyCode::Char('j') => Some(Intent::AdjustDebugScroll { delta: 1 }),
        KeyCode::PageUp => Some(Intent::AdjustDebugScroll { delta: -10 }),
        KeyCode::PageDown => Some(Intent::AdjustDebugScroll { delta: 10 }),
        KeyCode::Home => Some(Intent::SetDebugScroll { value: 0 }),
        KeyCode::End => Some(Intent::SetDebugScroll { value: i32::MAX }),
        _ => None,
    }
}

fn player_intent(code: KeyCode, len: usize) -> Option<Intent> {
    let max_scroll = len.saturating_sub(1).min(u16::MAX as usize) as u16;
    match code {
        KeyCode::Up | KeyCode::Char('k') => Some(Intent::AdjustPlayerScroll {
            delta: -1,
            max: max_scroll,
        }),
        KeyCode::Down | KeyCode::Char('j') => Some(Intent::AdjustPlayerScroll {
            delta: 1,
            max: max_scroll,
        }),
        KeyCode::PageUp => Some(Intent::AdjustPlayerScroll {
            delta: -10,
            max: max_scroll,
        }),
        KeyCode::PageDown => Some(Intent::AdjustPlayerScroll {
            delta: 10,
            max: max_scroll,
        }),
        KeyCode::Home => Some(Intent::SetPlayerScroll { value: 0 }),
        KeyCode::End => Some(Intent::SetPlayerScroll { value: max_scroll }),
        _ => None,
    }
}
