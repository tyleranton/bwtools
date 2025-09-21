use crossterm::event::KeyCode;

use crate::players::PlayerDirectory;

use super::App;

impl App {
    pub(super) fn handle_players_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Left => {
                if self.players.search_cursor > 0 {
                    self.players.search_cursor -= 1;
                }
                true
            }
            KeyCode::Right => {
                let len = self.players.search_query.chars().count();
                if self.players.search_cursor < len {
                    self.players.search_cursor += 1;
                }
                true
            }
            KeyCode::Home => {
                self.players.search_cursor = 0;
                true
            }
            KeyCode::End => {
                self.players.search_cursor = self.players.search_query.chars().count();
                true
            }
            KeyCode::Backspace => {
                if self.players.search_cursor > 0 {
                    let mut chars: Vec<char> = self.players.search_query.chars().collect();
                    let idx = self.players.search_cursor - 1;
                    if idx < chars.len() {
                        chars.remove(idx);
                        self.players.search_query = chars.into_iter().collect();
                        self.players.search_cursor -= 1;
                        self.update_player_filter();
                    }
                }
                true
            }
            KeyCode::Delete => {
                let mut chars: Vec<char> = self.players.search_query.chars().collect();
                if self.players.search_cursor < chars.len() {
                    chars.remove(self.players.search_cursor);
                    self.players.search_query = chars.into_iter().collect();
                    self.update_player_filter();
                }
                true
            }
            KeyCode::Char(c) => {
                let mut chars: Vec<char> = self.players.search_query.chars().collect();
                let idx = self.players.search_cursor.min(chars.len());
                chars.insert(idx, c);
                self.players.search_query = chars.into_iter().collect();
                self.players.search_cursor += 1;
                self.update_player_filter();
                true
            }
            _ => false,
        }
    }

    pub fn set_player_directory(&mut self, directory: PlayerDirectory) {
        self.players.directory = Some(directory);
        self.players.missing_data = false;
        self.players.scroll = 0;
        self.players.search_query.clear();
        self.players.search_cursor = 0;
        self.update_player_filter();
    }

    pub fn mark_player_directory_missing(&mut self) {
        self.players.directory = None;
        self.players.missing_data = true;
        self.players.scroll = 0;
        self.update_player_filter();
    }

    pub(crate) fn clamp_players_scroll(&mut self) {
        if self.players.filtered.is_empty() {
            self.players.scroll = 0;
            return;
        }
        let max_scroll = self
            .players
            .filtered
            .len()
            .saturating_sub(1)
            .min(u16::MAX as usize) as u16;
        if self.players.scroll > max_scroll {
            self.players.scroll = max_scroll;
        }
    }

    pub(crate) fn update_player_filter(&mut self) {
        let query = self.players.search_query.trim();
        self.players.filtered = self
            .players
            .directory
            .as_ref()
            .map(|dir| dir.filter(query))
            .unwrap_or_default();
        self.players.scroll = 0;
        self.clamp_players_scroll();
        self.clamp_player_search_cursor();
    }

    fn clamp_player_search_cursor(&mut self) {
        let len = self.players.search_query.chars().count();
        if self.players.search_cursor > len {
            self.players.search_cursor = len;
        }
    }
}
