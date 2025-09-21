use crossterm::event::KeyCode;

use super::App;

impl App {
    pub(super) fn handle_search_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Tab => {
                self.search.focus_gateway = !self.search.focus_gateway;
            }
            KeyCode::Left => {
                if self.search.focus_gateway {
                    self.gateway_prev();
                } else if self.search.cursor > 0 {
                    self.search.cursor -= 1;
                }
            }
            KeyCode::Right => {
                if self.search.focus_gateway {
                    self.gateway_next();
                } else {
                    let len = self.search.name.chars().count();
                    if self.search.cursor < len {
                        self.search.cursor += 1;
                    }
                }
            }
            KeyCode::Backspace => {
                if !self.search.focus_gateway && self.search.cursor > 0 {
                    let before: String = self
                        .search
                        .name
                        .chars()
                        .take(self.search.cursor - 1)
                        .collect();
                    let after: String = self.search.name.chars().skip(self.search.cursor).collect();
                    self.search.name = before + &after;
                    self.search.cursor -= 1;
                    self.clamp_search_cursor();
                }
            }
            KeyCode::Delete => {
                if !self.search.focus_gateway {
                    let len = self.search.name.chars().count();
                    if self.search.cursor < len {
                        let before: String =
                            self.search.name.chars().take(self.search.cursor).collect();
                        let after: String = self
                            .search
                            .name
                            .chars()
                            .skip(self.search.cursor + 1)
                            .collect();
                        self.search.name = before + &after;
                    }
                }
            }
            KeyCode::Enter => {
                self.search.in_progress = true;
                self.search.error = None;
                self.search.matches_scroll = 0;
            }
            KeyCode::Home => {
                if !self.search.focus_gateway {
                    self.search.cursor = 0;
                }
            }
            KeyCode::End => {
                if !self.search.focus_gateway {
                    self.search.cursor = self.search.name.chars().count();
                }
            }
            KeyCode::Char(c) => {
                if !self.search.focus_gateway {
                    let len = self.search.name.chars().count();
                    if self.search.cursor >= len {
                        self.search.name.push(c);
                    } else {
                        let before: String =
                            self.search.name.chars().take(self.search.cursor).collect();
                        let after: String =
                            self.search.name.chars().skip(self.search.cursor).collect();
                        self.search.name = before + &c.to_string() + &after;
                    }
                    self.search.cursor += 1;
                }
            }
            KeyCode::Up => {
                self.search.matches_scroll = self.search.matches_scroll.saturating_sub(1);
            }
            KeyCode::Down => {
                self.search.matches_scroll = self.search.matches_scroll.saturating_add(1);
            }
            KeyCode::PageUp => {
                self.search.matches_scroll = self.search.matches_scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.search.matches_scroll = self.search.matches_scroll.saturating_add(10);
            }
            _ => {}
        }
    }

    pub(crate) fn gateway_next(&mut self) {
        self.search.gateway = match self.search.gateway {
            10 => 11,
            11 => 20,
            20 => 30,
            30 => 45,
            _ => 10,
        };
    }

    pub(crate) fn gateway_prev(&mut self) {
        self.search.gateway = match self.search.gateway {
            11 => 10,
            20 => 11,
            30 => 20,
            45 => 30,
            _ => 45,
        };
    }

    pub(crate) fn clamp_search_cursor(&mut self) {
        let len = self.search.name.chars().count();
        if self.search.cursor > len {
            self.search.cursor = len;
        }
    }
}
