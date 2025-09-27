use crossterm::event::KeyCode;

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

        self.handle_global_navigation_key(code);

        if matches!(self.view, View::Players) {
            self.clamp_players_scroll();
        }
    }

    fn handle_global_navigation_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Up => {
                if matches!(self.view, View::Debug) {
                    self.debug.scroll = self.debug.scroll.saturating_sub(1);
                } else if matches!(self.view, View::Players) {
                    self.players.scroll = self.players.scroll.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if matches!(self.view, View::Debug) {
                    self.debug.scroll = self.debug.scroll.saturating_add(1);
                } else if matches!(self.view, View::Players) {
                    let max_scroll = self
                        .players
                        .filtered
                        .len()
                        .saturating_sub(1)
                        .min(u16::MAX as usize) as u16;
                    self.players.scroll = self.players.scroll.saturating_add(1).min(max_scroll);
                }
            }
            KeyCode::PageUp => {
                if matches!(self.view, View::Debug) {
                    self.debug.scroll = self.debug.scroll.saturating_sub(10);
                } else if matches!(self.view, View::Players) {
                    self.players.scroll = self.players.scroll.saturating_sub(10);
                }
            }
            KeyCode::PageDown => {
                if matches!(self.view, View::Debug) {
                    self.debug.scroll = self.debug.scroll.saturating_add(10);
                } else if matches!(self.view, View::Players) {
                    let max_scroll = self
                        .players
                        .filtered
                        .len()
                        .saturating_sub(1)
                        .min(u16::MAX as usize) as u16;
                    self.players.scroll = self.players.scroll.saturating_add(10).min(max_scroll);
                }
            }
            KeyCode::Home => {
                if matches!(self.view, View::Debug) {
                    self.debug.scroll = 0;
                } else if matches!(self.view, View::Players) {
                    self.players.scroll = 0;
                }
            }
            KeyCode::End => {
                if matches!(self.view, View::Debug) {
                    self.debug.scroll = u16::MAX;
                } else if matches!(self.view, View::Players) {
                    let max_scroll = self
                        .players
                        .filtered
                        .len()
                        .saturating_sub(1)
                        .min(u16::MAX as usize) as u16;
                    self.players.scroll = max_scroll;
                }
            }
            KeyCode::Char('k') => {
                if matches!(self.view, View::Debug) {
                    self.debug.scroll = self.debug.scroll.saturating_sub(1);
                } else if matches!(self.view, View::Players) {
                    self.players.scroll = self.players.scroll.saturating_sub(1);
                }
            }
            KeyCode::Char('j') => {
                if matches!(self.view, View::Debug) {
                    self.debug.scroll = self.debug.scroll.saturating_add(1);
                } else if matches!(self.view, View::Players) {
                    let max_scroll = self
                        .players
                        .filtered
                        .len()
                        .saturating_sub(1)
                        .min(u16::MAX as usize) as u16;
                    self.players.scroll = self.players.scroll.saturating_add(1).min(max_scroll);
                }
            }
            _ => {}
        }
    }
}
