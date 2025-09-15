use crate::api::ApiHandle;
use crate::history::OpponentRecord;
use std::time::Instant;
use std::collections::{HashSet, HashMap};
use ratatui::layout::Rect;

pub enum View {
    Main,
    Debug,
    Search,
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
    
    // Search view state
    pub search_name: String,
    pub search_gateway: u16,
    pub search_focus_gateway: bool,
    pub search_in_progress: bool,
    pub search_rating: Option<u32>,
    pub search_other_toons: Vec<String>,
    pub search_other_toons_data: Vec<(String, u16, u32)>,
    pub search_matches: Vec<String>,
    pub search_error: Option<String>,
    pub search_matches_scroll: u16,
    pub search_cursor: usize,
    pub search_main_race: Option<String>,
    pub search_matchups: Vec<String>,
    // Clickable regions
    pub status_opponent_rect: Option<Rect>,
    pub main_opponent_toons_rect: Option<Rect>,
    pub search_other_toons_rect: Option<Rect>,
    // Self stats for main view
    pub self_main_race: Option<String>,
    pub self_matchups: Vec<String>,
}

impl App {
    pub fn new() -> Self {
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
            search_name: String::new(),
            search_gateway: 10,
            search_focus_gateway: false,
            search_in_progress: false,
            search_rating: None,
            search_other_toons: Vec::new(),
            search_other_toons_data: Vec::new(),
            search_matches: Vec::new(),
            search_error: None,
            search_matches_scroll: 0,
            search_cursor: 0,
            search_main_race: None,
            search_matchups: Vec::new(),
            status_opponent_rect: None,
            main_opponent_toons_rect: None,
            search_other_toons_rect: None,
            self_main_race: None,
            self_matchups: Vec::new(),
        }
    }

    pub fn reset_opponent_state(&mut self) {
        self.profile_name = None;
        self.gateway = None;
        self.opponent_toons.clear();
        self.opponent_toons_data.clear();
        self.last_opponent_identity = None;
        self.opponent_race = None;
        self.opponent_output_last_text = None;
    }

    pub fn on_key(&mut self, code: crossterm::event::KeyCode) {
        // If we are in Search view, handle input locally and do not trigger global hotkeys
        if matches!(self.view, View::Search) {
            match code {
                crossterm::event::KeyCode::Tab => {
                    self.search_focus_gateway = !self.search_focus_gateway;
                }
                crossterm::event::KeyCode::Left => {
                    if self.search_focus_gateway { self.gateway_prev(); }
                    else if self.search_cursor > 0 { self.search_cursor -= 1; }
                }
                crossterm::event::KeyCode::Right => {
                    if self.search_focus_gateway { self.gateway_next(); }
                    else {
                        let len = self.search_name.chars().count();
                        if self.search_cursor < len { self.search_cursor += 1; }
                    }
                }
                crossterm::event::KeyCode::Backspace => {
                    if !self.search_focus_gateway {
                        if self.search_cursor > 0 {
                            let before: String = self.search_name.chars().take(self.search_cursor - 1).collect();
                            let after: String = self.search_name.chars().skip(self.search_cursor).collect();
                            self.search_name = before + &after;
                            self.search_cursor -= 1;
                            self.clamp_search_cursor();
                        }
                    }
                }
                crossterm::event::KeyCode::Enter => {
                    self.search_in_progress = true;
                    self.search_error = None;
                    self.search_matches_scroll = 0;
                }
                crossterm::event::KeyCode::Home => {
                    if !self.search_focus_gateway { self.search_cursor = 0; }
                }
                crossterm::event::KeyCode::End => {
                    if !self.search_focus_gateway { self.search_cursor = self.search_name.chars().count(); }
                }
                crossterm::event::KeyCode::Delete => {
                    if !self.search_focus_gateway {
                        let len = self.search_name.chars().count();
                        if self.search_cursor < len {
                            let before: String = self.search_name.chars().take(self.search_cursor).collect();
                            let after: String = self.search_name.chars().skip(self.search_cursor + 1).collect();
                            self.search_name = before + &after;
                        }
                    }
                }
                crossterm::event::KeyCode::Char(c) => {
                    if !self.search_focus_gateway {
                        let len = self.search_name.chars().count();
                        if self.search_cursor >= len {
                            self.search_name.push(c);
                        } else {
                            let before: String = self.search_name.chars().take(self.search_cursor).collect();
                            let after: String = self.search_name.chars().skip(self.search_cursor).collect();
                            self.search_name = before + &c.to_string() + &after;
                        }
                        self.search_cursor += 1;
                    }
                }
                crossterm::event::KeyCode::Up => {
                    self.search_matches_scroll = self.search_matches_scroll.saturating_sub(1);
                }
                crossterm::event::KeyCode::Down => {
                    self.search_matches_scroll = self.search_matches_scroll.saturating_add(1);
                }
                crossterm::event::KeyCode::PageUp => {
                    self.search_matches_scroll = self.search_matches_scroll.saturating_sub(10);
                }
                crossterm::event::KeyCode::PageDown => {
                    self.search_matches_scroll = self.search_matches_scroll.saturating_add(10);
                }
                // Note: Home/End handled for name editing above; matches panel scroll uses Up/Down/Page keys
                _ => {}
            }
            return;
        }

        match code {
            // Note: Global view hotkeys are handled in main (Ctrl+D/S/M)
            crossterm::event::KeyCode::Up => {
                if matches!(self.view, View::Debug) {
                    self.debug_scroll = self.debug_scroll.saturating_sub(1);
                }
            }
            crossterm::event::KeyCode::Down => {
                if matches!(self.view, View::Debug) {
                    self.debug_scroll = self.debug_scroll.saturating_add(1);
                }
            }
            crossterm::event::KeyCode::PageUp => {
                if matches!(self.view, View::Debug) {
                    self.debug_scroll = self.debug_scroll.saturating_sub(10);
                }
            }
            crossterm::event::KeyCode::PageDown => {
                if matches!(self.view, View::Debug) {
                    self.debug_scroll = self.debug_scroll.saturating_add(10);
                }
            }
            crossterm::event::KeyCode::Home => {
                if matches!(self.view, View::Debug) {
                    self.debug_scroll = 0;
                }
            }
            crossterm::event::KeyCode::End => {
                if matches!(self.view, View::Debug) {
                    self.debug_scroll = u16::MAX;
                }
            }
            crossterm::event::KeyCode::Char('k') => {
                if matches!(self.view, View::Debug) {
                    self.debug_scroll = self.debug_scroll.saturating_sub(1);
                }
            }
            crossterm::event::KeyCode::Char('j') => {
                if matches!(self.view, View::Debug) {
                    self.debug_scroll = self.debug_scroll.saturating_add(1);
                }
            }
            _ => {}
        }
    }
}

impl App {
    pub fn is_ready(&self) -> bool {
        self.port.is_some() && self.self_profile_name.is_some()
    }

    pub fn gateway_next(&mut self) {
        self.search_gateway = match self.search_gateway { 10 => 11, 11 => 20, 20 => 30, 30 => 45, _ => 10 };
    }
    pub fn gateway_prev(&mut self) {
        self.search_gateway = match self.search_gateway { 11 => 10, 20 => 11, 30 => 20, 45 => 30, _ => 45 };
    }
    pub fn clamp_search_cursor(&mut self) {
        let len = self.search_name.chars().count();
        if self.search_cursor > len { self.search_cursor = len; }
    }
}
