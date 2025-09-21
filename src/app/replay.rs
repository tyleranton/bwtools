use crossterm::event::KeyCode;
use std::sync::mpsc::TryRecvError;

use super::{App, ReplayFocus};

impl App {
    pub(super) fn handle_replay_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Tab => {
                self.replay.focus = match self.replay.focus {
                    ReplayFocus::Toon => ReplayFocus::Alias,
                    ReplayFocus::Alias => ReplayFocus::Gateway,
                    ReplayFocus::Gateway => ReplayFocus::Matchup,
                    ReplayFocus::Matchup => ReplayFocus::Count,
                    ReplayFocus::Count => ReplayFocus::Toon,
                };
            }
            KeyCode::BackTab => {
                self.replay.focus = match self.replay.focus {
                    ReplayFocus::Toon => ReplayFocus::Count,
                    ReplayFocus::Alias => ReplayFocus::Toon,
                    ReplayFocus::Gateway => ReplayFocus::Alias,
                    ReplayFocus::Matchup => ReplayFocus::Gateway,
                    ReplayFocus::Count => ReplayFocus::Matchup,
                };
            }
            KeyCode::Left => match self.replay.focus {
                ReplayFocus::Toon => {
                    if self.replay.toon_cursor > 0 {
                        self.replay.toon_cursor -= 1;
                    }
                }
                ReplayFocus::Alias => {
                    if self.replay.alias_cursor > 0 {
                        self.replay.alias_cursor -= 1;
                    }
                }
                ReplayFocus::Gateway => {
                    self.replay_gateway_prev();
                }
                ReplayFocus::Matchup => {
                    if self.replay.matchup_cursor > 0 {
                        self.replay.matchup_cursor -= 1;
                    }
                }
                ReplayFocus::Count => {
                    if self.replay.input_count > 1 {
                        self.replay.input_count -= 1;
                    }
                }
            },
            KeyCode::Right => match self.replay.focus {
                ReplayFocus::Toon => {
                    let len = self.replay.toon_input.chars().count();
                    if self.replay.toon_cursor < len {
                        self.replay.toon_cursor += 1;
                    }
                }
                ReplayFocus::Alias => {
                    let len = self.replay.alias_input.chars().count();
                    if self.replay.alias_cursor < len {
                        self.replay.alias_cursor += 1;
                    }
                }
                ReplayFocus::Gateway => {
                    self.replay_gateway_next();
                }
                ReplayFocus::Matchup => {
                    let len = self.replay.matchup_input.chars().count();
                    if self.replay.matchup_cursor < len {
                        self.replay.matchup_cursor += 1;
                    }
                }
                ReplayFocus::Count => {
                    self.replay.input_count = self.replay.input_count.saturating_add(1);
                }
            },
            KeyCode::Up => {
                if matches!(self.replay.focus, ReplayFocus::Count) {
                    self.replay.input_count = self.replay.input_count.saturating_add(1);
                }
            }
            KeyCode::Down => {
                if matches!(self.replay.focus, ReplayFocus::Count) && self.replay.input_count > 1 {
                    self.replay.input_count -= 1;
                }
            }
            KeyCode::Home => match self.replay.focus {
                ReplayFocus::Toon => {
                    self.replay.toon_cursor = 0;
                }
                ReplayFocus::Alias => {
                    self.replay.alias_cursor = 0;
                }
                ReplayFocus::Matchup => {
                    self.replay.matchup_cursor = 0;
                }
                ReplayFocus::Gateway | ReplayFocus::Count => {}
            },
            KeyCode::End => match self.replay.focus {
                ReplayFocus::Toon => {
                    self.replay.toon_cursor = self.replay.toon_input.chars().count();
                }
                ReplayFocus::Alias => {
                    self.replay.alias_cursor = self.replay.alias_input.chars().count();
                }
                ReplayFocus::Matchup => {
                    self.replay.matchup_cursor = self.replay.matchup_input.chars().count();
                }
                ReplayFocus::Gateway | ReplayFocus::Count => {}
            },
            KeyCode::Backspace => match self.replay.focus {
                ReplayFocus::Toon => {
                    if self.replay.toon_cursor > 0 {
                        let before: String = self
                            .replay
                            .toon_input
                            .chars()
                            .take(self.replay.toon_cursor - 1)
                            .collect();
                        let after: String = self
                            .replay
                            .toon_input
                            .chars()
                            .skip(self.replay.toon_cursor)
                            .collect();
                        self.replay.toon_input = before + &after;
                        self.replay.toon_cursor -= 1;
                    }
                }
                ReplayFocus::Alias => {
                    if self.replay.alias_cursor > 0 {
                        let before: String = self
                            .replay
                            .alias_input
                            .chars()
                            .take(self.replay.alias_cursor - 1)
                            .collect();
                        let after: String = self
                            .replay
                            .alias_input
                            .chars()
                            .skip(self.replay.alias_cursor)
                            .collect();
                        self.replay.alias_input = before + &after;
                        self.replay.alias_cursor -= 1;
                    }
                }
                ReplayFocus::Matchup => {
                    if self.replay.matchup_cursor > 0 {
                        let before: String = self
                            .replay
                            .matchup_input
                            .chars()
                            .take(self.replay.matchup_cursor - 1)
                            .collect();
                        let after: String = self
                            .replay
                            .matchup_input
                            .chars()
                            .skip(self.replay.matchup_cursor)
                            .collect();
                        self.replay.matchup_input = before + &after;
                        self.replay.matchup_cursor -= 1;
                    }
                }
                ReplayFocus::Gateway | ReplayFocus::Count => {}
            },
            KeyCode::Delete => match self.replay.focus {
                ReplayFocus::Toon => {
                    let len = self.replay.toon_input.chars().count();
                    if self.replay.toon_cursor < len {
                        let before: String = self
                            .replay
                            .toon_input
                            .chars()
                            .take(self.replay.toon_cursor)
                            .collect();
                        let after: String = self
                            .replay
                            .toon_input
                            .chars()
                            .skip(self.replay.toon_cursor + 1)
                            .collect();
                        self.replay.toon_input = before + &after;
                    }
                }
                ReplayFocus::Alias => {
                    let len = self.replay.alias_input.chars().count();
                    if self.replay.alias_cursor < len {
                        let before: String = self
                            .replay
                            .alias_input
                            .chars()
                            .take(self.replay.alias_cursor)
                            .collect();
                        let after: String = self
                            .replay
                            .alias_input
                            .chars()
                            .skip(self.replay.alias_cursor + 1)
                            .collect();
                        self.replay.alias_input = before + &after;
                    }
                }
                ReplayFocus::Matchup => {
                    let len = self.replay.matchup_input.chars().count();
                    if self.replay.matchup_cursor < len {
                        let before: String = self
                            .replay
                            .matchup_input
                            .chars()
                            .take(self.replay.matchup_cursor)
                            .collect();
                        let after: String = self
                            .replay
                            .matchup_input
                            .chars()
                            .skip(self.replay.matchup_cursor + 1)
                            .collect();
                        self.replay.matchup_input = before + &after;
                    }
                }
                ReplayFocus::Gateway | ReplayFocus::Count => {}
            },
            KeyCode::Char(c) => match self.replay.focus {
                ReplayFocus::Toon => {
                    let len = self.replay.toon_input.chars().count();
                    if self.replay.toon_cursor >= len {
                        self.replay.toon_input.push(c);
                    } else {
                        let before: String = self
                            .replay
                            .toon_input
                            .chars()
                            .take(self.replay.toon_cursor)
                            .collect();
                        let after: String = self
                            .replay
                            .toon_input
                            .chars()
                            .skip(self.replay.toon_cursor)
                            .collect();
                        self.replay.toon_input = before + &c.to_string() + &after;
                    }
                    self.replay.toon_cursor += 1;
                }
                ReplayFocus::Alias => {
                    let len = self.replay.alias_input.chars().count();
                    if self.replay.alias_cursor >= len {
                        self.replay.alias_input.push(c);
                    } else {
                        let before: String = self
                            .replay
                            .alias_input
                            .chars()
                            .take(self.replay.alias_cursor)
                            .collect();
                        let after: String = self
                            .replay
                            .alias_input
                            .chars()
                            .skip(self.replay.alias_cursor)
                            .collect();
                        self.replay.alias_input = before + &c.to_string() + &after;
                    }
                    self.replay.alias_cursor += 1;
                }
                ReplayFocus::Matchup => {
                    let len = self.replay.matchup_input.chars().count();
                    if self.replay.matchup_cursor >= len {
                        self.replay.matchup_input.push(c);
                    } else {
                        let before: String = self
                            .replay
                            .matchup_input
                            .chars()
                            .take(self.replay.matchup_cursor)
                            .collect();
                        let after: String = self
                            .replay
                            .matchup_input
                            .chars()
                            .skip(self.replay.matchup_cursor)
                            .collect();
                        self.replay.matchup_input = before + &c.to_string() + &after;
                    }
                    self.replay.matchup_cursor += 1;
                }
                ReplayFocus::Count => {
                    if c.is_ascii_digit() {
                        let digit = c.to_digit(10).unwrap_or(0) as u16;
                        self.replay.input_count = digit.max(1);
                    }
                }
                ReplayFocus::Gateway => {}
            },
            KeyCode::Enter => {
                self.replay.should_start = true;
            }
            _ => {}
        }
    }

    pub(crate) fn replay_gateway_next(&mut self) {
        self.replay.input_gateway = match self.replay.input_gateway {
            10 => 11,
            11 => 20,
            20 => 30,
            30 => 45,
            _ => 10,
        };
    }

    pub(crate) fn replay_gateway_prev(&mut self) {
        self.replay.input_gateway = match self.replay.input_gateway {
            11 => 10,
            20 => 11,
            30 => 20,
            45 => 30,
            _ => 45,
        };
    }

    pub(crate) fn clamp_replay_cursors(&mut self) {
        let len = self.replay.toon_input.chars().count();
        if self.replay.toon_cursor > len {
            self.replay.toon_cursor = len;
        }
        let len_a = self.replay.alias_input.chars().count();
        if self.replay.alias_cursor > len_a {
            self.replay.alias_cursor = len_a;
        }
        let len_m = self.replay.matchup_input.chars().count();
        if self.replay.matchup_cursor > len_m {
            self.replay.matchup_cursor = len_m;
        }
        if self.replay.input_count == 0 {
            self.replay.input_count = 1;
        }
    }

    pub fn poll_replay_job(&mut self) {
        let mut clear = false;
        if let Some(rx) = self.replay.job_rx.as_ref() {
            match rx.try_recv() {
                Ok(summary) => {
                    self.replay.in_progress = false;
                    self.replay.last_summary = Some(summary);
                    clear = true;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.replay.in_progress = false;
                    self.replay.last_error = Some("Replay job channel disconnected".to_string());
                    clear = true;
                }
            }
        }
        if clear {
            self.replay.job_rx = None;
            if let Some(handle) = self.replay.job_handle.take() {
                let _ = handle.join();
            }
        }
        self.clamp_replay_cursors();
    }
}
