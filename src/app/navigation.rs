use crossterm::event::KeyCode;

use crate::interaction::Intent;

use super::{App, View};

impl App {
    pub fn on_key(&mut self, code: KeyCode) {
        match self.view {
            View::Replays => {
                self.handle_replay_key(code);
                return;
            }
            View::Main | View::Debug => {}
        }

        if let Some(intent) = global_intent(self.view, code) {
            intent.apply(self);
        }
    }
}

fn global_intent(view: View, code: KeyCode) -> Option<Intent> {
    match view {
        View::Debug => debug_intent(code),
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
