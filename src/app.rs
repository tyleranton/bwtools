use crate::api::ApiHandle;
use crate::history::OpponentRecord;
use crate::players::{PlayerDirectory, PlayerEntry};
use crate::replay_download::{ReplayDownloadRequest, ReplayDownloadSummary, ReplayStorage};
use crossterm::event::KeyCode;
use ratatui::layout::Rect;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::thread::JoinHandle;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Main,
    Debug,
    Search,
    Replays,
    Players,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayFocus {
    Toon,
    Alias,
    Gateway,
    Matchup,
    Count,
}

#[derive(Debug)]
pub struct SearchState {
    pub name: String,
    pub gateway: u16,
    pub focus_gateway: bool,
    pub in_progress: bool,
    pub rating: Option<u32>,
    pub other_toons: Vec<String>,
    pub other_toons_data: Vec<(String, u16, u32)>,
    pub matches: Vec<String>,
    pub error: Option<String>,
    pub matches_scroll: u16,
    pub cursor: usize,
    pub main_race: Option<String>,
    pub matchups: Vec<String>,
    pub other_toons_rect: Option<Rect>,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            name: String::new(),
            gateway: 10,
            focus_gateway: false,
            in_progress: false,
            rating: None,
            other_toons: Vec::new(),
            other_toons_data: Vec::new(),
            matches: Vec::new(),
            error: None,
            matches_scroll: 0,
            cursor: 0,
            main_race: None,
            matchups: Vec::new(),
            other_toons_rect: None,
        }
    }
}

#[derive(Debug)]
pub struct ReplayState {
    pub focus: ReplayFocus,
    pub toon_input: String,
    pub toon_cursor: usize,
    pub matchup_input: String,
    pub matchup_cursor: usize,
    pub alias_input: String,
    pub alias_cursor: usize,
    pub input_gateway: u16,
    pub input_count: u16,
    pub in_progress: bool,
    pub should_start: bool,
    pub last_summary: Option<ReplayDownloadSummary>,
    pub last_request: Option<ReplayDownloadRequest>,
    pub last_error: Option<String>,
    pub job_rx: Option<Receiver<ReplayDownloadSummary>>,
    pub job_handle: Option<JoinHandle<()>>,
}

impl Default for ReplayState {
    fn default() -> Self {
        Self {
            focus: ReplayFocus::Toon,
            toon_input: String::new(),
            toon_cursor: 0,
            matchup_input: String::new(),
            matchup_cursor: 0,
            alias_input: String::new(),
            alias_cursor: 0,
            input_gateway: 10,
            input_count: 5,
            in_progress: false,
            should_start: false,
            last_summary: None,
            last_request: None,
            last_error: None,
            job_rx: None,
            job_handle: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct PlayersState {
    pub directory: Option<PlayerDirectory>,
    pub scroll: u16,
    pub filtered: Vec<PlayerEntry>,
    pub search_query: String,
    pub search_cursor: usize,
    pub missing_data: bool,
}

pub struct App {
    pub should_quit: bool,
    pub port: Option<u16>,
    pub profile_name: Option<String>,
    pub gateway: Option<u16>,
    pub self_profile_name: Option<String>,
    pub self_gateway: Option<u16>,
    pub self_profile_rating: Option<u32>,
    pub debug_recent: Vec<String>,
    pub view: View,
    pub debug_window_secs: i64,
    pub api: Option<ApiHandle>,
    pub last_port_used: Option<u16>,
    pub profile_fetched: bool,
    pub last_profile_text: Option<String>,
    pub debug_scroll: u16,
    pub opponent_toons: Vec<String>,
    pub opponent_toons_data: Vec<(String, u16, u32)>,
    pub last_opponent_identity: Option<(String, u16)>,
    pub opponent_race: Option<String>,
    pub own_profiles: HashSet<String>,
    pub last_rating_poll: Option<Instant>,
    pub rating_output_last_text: Option<String>,
    pub opponent_history: HashMap<String, OpponentRecord>,
    // Replay watcher state
    pub screp_available: bool,
    pub last_replay_mtime: Option<std::time::SystemTime>,
    pub last_replay_processed_mtime: Option<std::time::SystemTime>,
    pub replay_changed_at: Option<Instant>,
    pub opponent_output_last_text: Option<String>,
    // Post-replay rating retry state
    pub rating_retry_retries: u8,
    pub rating_retry_next_at: Option<Instant>,
    pub rating_retry_baseline: Option<u32>,
    pub replay_storage: Option<ReplayStorage>,
    pub replay: ReplayState,
    pub search: SearchState,
    // Clickable regions
    pub status_opponent_rect: Option<Rect>,
    pub main_opponent_toons_rect: Option<Rect>,
    // Self stats for main view
    pub self_main_race: Option<String>,
    pub self_matchups: Vec<String>,
    // Player directory view
    pub players: PlayersState,
}

impl App {
    pub fn reset_opponent_state(&mut self) {
        self.profile_name = None;
        self.gateway = None;
        self.opponent_toons.clear();
        self.opponent_toons_data.clear();
        self.last_opponent_identity = None;
        self.opponent_race = None;
        self.opponent_output_last_text = None;
    }

    pub fn begin_search(&mut self, name: String, gateway: u16) {
        self.view = View::Search;
        self.search.name = name;
        self.search.gateway = gateway;
        self.search.focus_gateway = false;
        self.search.cursor = self.search.name.chars().count();
        self.search.matches_scroll = 0;
        self.search.in_progress = true;
    }

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

    fn handle_replay_key(&mut self, code: KeyCode) {
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

    fn handle_search_key(&mut self, code: KeyCode) {
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

    fn handle_players_key(&mut self, code: KeyCode) -> bool {
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

    fn handle_global_navigation_key(&mut self, code: KeyCode) {
        match code {
            // Note: Global view hotkeys are handled in main (Ctrl+D/S/M)
            KeyCode::Up => {
                if matches!(self.view, View::Debug) {
                    self.debug_scroll = self.debug_scroll.saturating_sub(1);
                } else if matches!(self.view, View::Players) {
                    self.players.scroll = self.players.scroll.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if matches!(self.view, View::Debug) {
                    self.debug_scroll = self.debug_scroll.saturating_add(1);
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
                    self.debug_scroll = self.debug_scroll.saturating_sub(10);
                } else if matches!(self.view, View::Players) {
                    self.players.scroll = self.players.scroll.saturating_sub(10);
                }
            }
            KeyCode::PageDown => {
                if matches!(self.view, View::Debug) {
                    self.debug_scroll = self.debug_scroll.saturating_add(10);
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
                    self.debug_scroll = 0;
                } else if matches!(self.view, View::Players) {
                    self.players.scroll = 0;
                }
            }
            KeyCode::End => {
                if matches!(self.view, View::Debug) {
                    self.debug_scroll = u16::MAX;
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
                    self.debug_scroll = self.debug_scroll.saturating_sub(1);
                } else if matches!(self.view, View::Players) {
                    self.players.scroll = self.players.scroll.saturating_sub(1);
                }
            }
            KeyCode::Char('j') => {
                if matches!(self.view, View::Debug) {
                    self.debug_scroll = self.debug_scroll.saturating_add(1);
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

impl Default for App {
    fn default() -> Self {
        Self {
            should_quit: false,
            port: None,
            profile_name: None,
            gateway: None,
            self_profile_name: None,
            self_gateway: None,
            self_profile_rating: None,
            debug_recent: Vec::new(),
            view: View::Main,
            debug_window_secs: 10,
            api: None,
            last_port_used: None,
            profile_fetched: false,
            last_profile_text: None,
            debug_scroll: 0,
            opponent_toons: Vec::new(),
            opponent_toons_data: Vec::new(),
            last_opponent_identity: None,
            opponent_race: None,
            own_profiles: HashSet::new(),
            last_rating_poll: None,
            rating_output_last_text: None,
            opponent_history: HashMap::new(),
            screp_available: false,
            last_replay_mtime: None,
            last_replay_processed_mtime: None,
            replay_changed_at: None,
            opponent_output_last_text: None,
            rating_retry_retries: 0,
            rating_retry_next_at: None,
            rating_retry_baseline: None,
            replay_storage: None,
            replay: ReplayState::default(),
            search: SearchState::default(),
            status_opponent_rect: None,
            main_opponent_toons_rect: None,
            self_main_race: None,
            self_matchups: Vec::new(),
            players: PlayersState::default(),
        }
    }
}

impl App {
    pub fn is_ready(&self) -> bool {
        self.port.is_some() && self.self_profile_name.is_some()
    }

    pub fn gateway_next(&mut self) {
        self.search.gateway = match self.search.gateway {
            10 => 11,
            11 => 20,
            20 => 30,
            30 => 45,
            _ => 10,
        };
    }
    pub fn gateway_prev(&mut self) {
        self.search.gateway = match self.search.gateway {
            11 => 10,
            20 => 11,
            30 => 20,
            45 => 30,
            _ => 45,
        };
    }
    pub fn clamp_search_cursor(&mut self) {
        let len = self.search.name.chars().count();
        if self.search.cursor > len {
            self.search.cursor = len;
        }
    }

    fn clamp_player_search_cursor(&mut self) {
        let len = self.players.search_query.chars().count();
        if self.players.search_cursor > len {
            self.players.search_cursor = len;
        }
    }

    fn clamp_players_scroll(&mut self) {
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

    pub fn update_player_filter(&mut self) {
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

    pub fn replay_gateway_next(&mut self) {
        self.replay.input_gateway = match self.replay.input_gateway {
            10 => 11,
            11 => 20,
            20 => 30,
            30 => 45,
            _ => 10,
        };
    }

    pub fn replay_gateway_prev(&mut self) {
        self.replay.input_gateway = match self.replay.input_gateway {
            11 => 10,
            20 => 11,
            30 => 20,
            45 => 30,
            _ => 45,
        };
    }

    pub fn clamp_replay_cursors(&mut self) {
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
