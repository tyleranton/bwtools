use crate::api::ApiHandle;
use std::time::Instant;
use std::collections::HashSet;

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
    pub last_opponent_identity: Option<(String, u16)>,
    pub own_profiles: HashSet<String>,
    pub last_rating_poll: Option<Instant>,
    // Search view state
    pub search_name: String,
    pub search_gateway: u16,
    pub search_focus_gateway: bool,
    pub search_in_progress: bool,
    pub search_rating: Option<u32>,
    pub search_other_toons: Vec<String>,
    pub search_matches: Vec<String>,
    pub search_error: Option<String>,
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
            last_opponent_identity: None,
            own_profiles: HashSet::new(),
            last_rating_poll: None,
            search_name: String::new(),
            search_gateway: 10,
            search_focus_gateway: false,
            search_in_progress: false,
            search_rating: None,
            search_other_toons: Vec::new(),
            search_matches: Vec::new(),
            search_error: None,
        }
    }

    pub fn on_key(&mut self, code: crossterm::event::KeyCode) {
        // If we are in Search view, handle input locally and do not trigger global hotkeys
        if matches!(self.view, View::Search) {
            match code {
                crossterm::event::KeyCode::Tab => {
                    self.search_focus_gateway = !self.search_focus_gateway;
                }
                crossterm::event::KeyCode::Left => {
                    if self.search_focus_gateway {
                        let gw = self.search_gateway;
                        self.search_gateway = match gw { 11 => 10, 20 => 11, 30 => 20, 45 => 30, _ => 45 };
                    }
                }
                crossterm::event::KeyCode::Right => {
                    if self.search_focus_gateway {
                        let gw = self.search_gateway;
                        self.search_gateway = match gw { 10 => 11, 11 => 20, 20 => 30, 30 => 45, _ => 10 };
                    }
                }
                crossterm::event::KeyCode::Backspace => {
                    if !self.search_focus_gateway { self.search_name.pop(); }
                }
                crossterm::event::KeyCode::Enter => {
                    self.search_in_progress = true;
                    self.search_error = None;
                }
                crossterm::event::KeyCode::Char(c) => {
                    if !self.search_focus_gateway { self.search_name.push(c); }
                }
                _ => {}
            }
            return;
        }

        match code {
            crossterm::event::KeyCode::Char('d') => {
                self.view = match self.view {
                    View::Main => View::Debug,
                    View::Debug => View::Main,
                    View::Search => View::Debug,
                };
                if matches!(self.view, View::Debug) {
                    self.debug_scroll = 0;
                }
            }
            crossterm::event::KeyCode::Char('s') => {
                self.view = View::Search;
            }
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
}
