use crate::api::ApiHandle;
use std::time::Instant;
use std::collections::HashSet;

pub enum View {
    Main,
    Debug,
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
        }
    }

    pub fn on_key(&mut self, code: crossterm::event::KeyCode) {
        match code {
            crossterm::event::KeyCode::Char('d') => {
                self.view = match self.view {
                    View::Main => View::Debug,
                    View::Debug => View::Main,
                };
                if matches!(self.view, View::Debug) {
                    self.debug_scroll = 0;
                }
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
