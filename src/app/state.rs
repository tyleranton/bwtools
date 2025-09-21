use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;
use std::time::Instant;

use ratatui::layout::Rect;

use crate::api::ApiHandle;
use crate::history::OpponentRecord;
use crate::players::{PlayerDirectory, PlayerEntry};
use crate::replay_download::{ReplayDownloadRequest, ReplayDownloadSummary, ReplayStorage};

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
    pub screp_available: bool,
    pub last_replay_mtime: Option<std::time::SystemTime>,
    pub last_replay_processed_mtime: Option<std::time::SystemTime>,
    pub replay_changed_at: Option<Instant>,
    pub opponent_output_last_text: Option<String>,
    pub rating_retry_retries: u8,
    pub rating_retry_next_at: Option<Instant>,
    pub rating_retry_baseline: Option<u32>,
    pub replay_storage: Option<ReplayStorage>,
    pub replay: ReplayState,
    pub search: SearchState,
    pub status_opponent_rect: Option<Rect>,
    pub main_opponent_toons_rect: Option<Rect>,
    pub self_main_race: Option<String>,
    pub self_matchups: Vec<String>,
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

    pub fn is_ready(&self) -> bool {
        self.port.is_some() && self.self_profile_name.is_some()
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
