use crossterm::event::KeyCode;
use std::sync::mpsc::TryRecvError;

use super::{App, ReplayFocus};

impl App {
    pub(super) fn handle_replay_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Tab => {
                self.replay.focus = next_focus(self.replay.focus);
            }
            KeyCode::BackTab => {
                self.replay.focus = prev_focus(self.replay.focus);
            }
            KeyCode::Left => match self.replay.focus {
                ReplayFocus::Toon | ReplayFocus::Alias | ReplayFocus::Matchup => {
                    if let Some((text, cursor)) = self.replay_active_text_mut() {
                        move_cursor_left(text, cursor);
                    }
                }
                ReplayFocus::Gateway => {
                    self.replay_gateway_prev();
                }
                ReplayFocus::Count => {
                    self.replay_decrement_count();
                }
            },
            KeyCode::Right => match self.replay.focus {
                ReplayFocus::Toon | ReplayFocus::Alias | ReplayFocus::Matchup => {
                    if let Some((text, cursor)) = self.replay_active_text_mut() {
                        move_cursor_right(text, cursor);
                    }
                }
                ReplayFocus::Gateway => {
                    self.replay_gateway_next();
                }
                ReplayFocus::Count => {
                    self.replay_increment_count();
                }
            },
            KeyCode::Up => {
                if matches!(self.replay.focus, ReplayFocus::Count) {
                    self.replay_increment_count();
                }
            }
            KeyCode::Down => {
                if matches!(self.replay.focus, ReplayFocus::Count) {
                    self.replay_decrement_count();
                }
            }
            KeyCode::Home => match self.replay.focus {
                ReplayFocus::Toon | ReplayFocus::Alias | ReplayFocus::Matchup => {
                    if let Some((_text, cursor)) = self.replay_active_text_mut() {
                        *cursor = 0;
                    }
                }
                ReplayFocus::Gateway | ReplayFocus::Count => {}
            },
            KeyCode::End => match self.replay.focus {
                ReplayFocus::Toon | ReplayFocus::Alias | ReplayFocus::Matchup => {
                    if let Some((text, cursor)) = self.replay_active_text_mut() {
                        *cursor = text.chars().count();
                    }
                }
                ReplayFocus::Gateway | ReplayFocus::Count => {}
            },
            KeyCode::Backspace => match self.replay.focus {
                ReplayFocus::Toon | ReplayFocus::Alias | ReplayFocus::Matchup => {
                    if let Some((text, cursor)) = self.replay_active_text_mut() {
                        backspace_at_cursor(text, cursor);
                    }
                }
                ReplayFocus::Gateway | ReplayFocus::Count => {}
            },
            KeyCode::Delete => match self.replay.focus {
                ReplayFocus::Toon | ReplayFocus::Alias | ReplayFocus::Matchup => {
                    if let Some((text, cursor)) = self.replay_active_text_mut() {
                        delete_at_cursor(text, cursor);
                    }
                }
                ReplayFocus::Gateway | ReplayFocus::Count => {}
            },
            KeyCode::Char(c) => match self.replay.focus {
                ReplayFocus::Toon | ReplayFocus::Alias | ReplayFocus::Matchup => {
                    if let Some((text, cursor)) = self.replay_active_text_mut() {
                        insert_char_at_cursor(text, cursor, c);
                    }
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

    fn replay_active_text_mut(&mut self) -> Option<(&mut String, &mut usize)> {
        match self.replay.focus {
            ReplayFocus::Toon => Some((&mut self.replay.toon_input, &mut self.replay.toon_cursor)),
            ReplayFocus::Alias => {
                Some((&mut self.replay.alias_input, &mut self.replay.alias_cursor))
            }
            ReplayFocus::Matchup => Some((
                &mut self.replay.matchup_input,
                &mut self.replay.matchup_cursor,
            )),
            ReplayFocus::Gateway | ReplayFocus::Count => None,
        }
    }

    fn replay_increment_count(&mut self) {
        self.replay.input_count = self.replay.input_count.saturating_add(1);
    }

    fn replay_decrement_count(&mut self) {
        if self.replay.input_count > 1 {
            self.replay.input_count -= 1;
        }
    }

    pub(crate) fn replay_gateway_next(&mut self) {
        self.replay.input_gateway = crate::gateway::next_gateway(self.replay.input_gateway);
    }

    pub(crate) fn replay_gateway_prev(&mut self) {
        self.replay.input_gateway = crate::gateway::prev_gateway(self.replay.input_gateway);
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

fn next_focus(current: ReplayFocus) -> ReplayFocus {
    match current {
        ReplayFocus::Toon => ReplayFocus::Alias,
        ReplayFocus::Alias => ReplayFocus::Gateway,
        ReplayFocus::Gateway => ReplayFocus::Matchup,
        ReplayFocus::Matchup => ReplayFocus::Count,
        ReplayFocus::Count => ReplayFocus::Toon,
    }
}

fn prev_focus(current: ReplayFocus) -> ReplayFocus {
    match current {
        ReplayFocus::Toon => ReplayFocus::Count,
        ReplayFocus::Alias => ReplayFocus::Toon,
        ReplayFocus::Gateway => ReplayFocus::Alias,
        ReplayFocus::Matchup => ReplayFocus::Gateway,
        ReplayFocus::Count => ReplayFocus::Matchup,
    }
}

fn move_cursor_left(text: &str, cursor: &mut usize) {
    let len = text.chars().count();
    if *cursor > len {
        *cursor = len;
    }
    *cursor = cursor.saturating_sub(1);
}

fn move_cursor_right(text: &str, cursor: &mut usize) {
    let len = text.chars().count();
    if *cursor < len {
        *cursor += 1;
    }
}

fn insert_char_at_cursor(text: &mut String, cursor: &mut usize, c: char) {
    let chars: Vec<char> = text.chars().collect();
    let idx = (*cursor).min(chars.len());
    let mut updated = String::with_capacity(text.len() + c.len_utf8());
    updated.extend(chars[..idx].iter().copied());
    updated.push(c);
    updated.extend(chars[idx..].iter().copied());
    *text = updated;
    *cursor = idx + 1;
}

fn backspace_at_cursor(text: &mut String, cursor: &mut usize) {
    if *cursor == 0 {
        return;
    }

    let chars: Vec<char> = text.chars().collect();
    let idx = (*cursor).min(chars.len());
    if idx == 0 {
        *cursor = 0;
        return;
    }

    let mut updated = String::with_capacity(text.len());
    updated.extend(chars[..idx - 1].iter().copied());
    updated.extend(chars[idx..].iter().copied());
    *text = updated;
    *cursor = idx - 1;
}

fn delete_at_cursor(text: &mut String, cursor: &mut usize) {
    let chars: Vec<char> = text.chars().collect();
    let idx = (*cursor).min(chars.len());
    if idx >= chars.len() {
        return;
    }

    let mut updated = String::with_capacity(text.len());
    updated.extend(chars[..idx].iter().copied());
    updated.extend(chars[idx + 1..].iter().copied());
    *text = updated;
    *cursor = idx;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_gateway_wraps_in_both_directions() {
        assert_eq!(crate::gateway::prev_gateway(10), 45);
        assert_eq!(crate::gateway::next_gateway(45), 10);
        assert_eq!(crate::gateway::next_gateway(11), 20);
        assert_eq!(crate::gateway::prev_gateway(20), 11);
    }

    #[test]
    fn rotate_gateway_defaults_unknown_to_cycle_boundary() {
        assert_eq!(crate::gateway::next_gateway(999), 10);
        assert_eq!(crate::gateway::prev_gateway(999), 45);
    }

    #[test]
    fn insert_backspace_and_delete_work_with_unicode_safe_cursoring() {
        let mut text = String::from("ab");
        let mut cursor = 1usize;

        insert_char_at_cursor(&mut text, &mut cursor, 'X');
        assert_eq!(text, "aXb");
        assert_eq!(cursor, 2);

        backspace_at_cursor(&mut text, &mut cursor);
        assert_eq!(text, "ab");
        assert_eq!(cursor, 1);

        delete_at_cursor(&mut text, &mut cursor);
        assert_eq!(text, "a");
        assert_eq!(cursor, 1);
    }

    #[test]
    fn focus_navigation_cycles_correctly() {
        assert_eq!(next_focus(ReplayFocus::Count), ReplayFocus::Toon);
        assert_eq!(prev_focus(ReplayFocus::Toon), ReplayFocus::Count);
    }
}
