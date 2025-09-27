use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;
use std::time::Instant;

use ratatui::layout::Rect;

use crate::api::ApiHandle;
use crate::history::OpponentRecord;
use crate::players::{PlayerDirectory, PlayerEntry};
use crate::profile_history::MatchOutcome;
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

#[derive(Debug, Clone)]
pub struct DodgeCandidate {
    pub opponent: String,
    pub outcome: Option<MatchOutcome>,
    pub approx_timestamp: Option<u64>,
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

#[derive(Debug, Default)]
pub struct DebugState {
    pub window_secs: i64,
    pub recent: Vec<String>,
    pub port_text: Option<String>,
    pub scroll: u16,
}

#[derive(Default)]
pub struct DetectionState {
    pub port: Option<u16>,
    pub api: Option<ApiHandle>,
    pub last_port_used: Option<u16>,
    pub screp_available: bool,
}

#[derive(Debug, Default)]
pub struct RatingRetryState {
    pub retries: u8,
    pub next_at: Option<Instant>,
    pub baseline: Option<u32>,
}

#[derive(Debug, Default)]
pub struct SelfProfileState {
    pub name: Option<String>,
    pub gateway: Option<u16>,
    pub rating: Option<u32>,
    pub profile_fetched: bool,
    pub own_profiles: HashSet<String>,
    pub last_rating_poll: Option<Instant>,
    pub rating_retry: RatingRetryState,
    pub main_race: Option<String>,
    pub matchups: Vec<String>,
    pub self_dodged: u32,
    pub opponent_dodged: u32,
}

#[derive(Debug, Default)]
pub struct OpponentState {
    pub name: Option<String>,
    pub gateway: Option<u16>,
    pub toons: Vec<String>,
    pub toons_data: Vec<(String, u16, u32)>,
    pub last_identity: Option<(String, u16)>,
    pub race: Option<String>,
    pub matchups: Vec<String>,
    pub history: HashMap<String, OpponentRecord>,
}

#[derive(Debug, Default)]
pub struct OverlayState {
    pub rating_last_text: Option<String>,
    pub opponent_last_text: Option<String>,
}

#[derive(Default)]
pub struct ReplayWatchState {
    pub storage: Option<ReplayStorage>,
    pub last_mtime: Option<std::time::SystemTime>,
    pub last_processed_mtime: Option<std::time::SystemTime>,
    pub changed_at: Option<Instant>,
    pub last_dodge_candidate: Option<DodgeCandidate>,
}

impl std::fmt::Debug for DetectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DetectionState")
            .field("port", &self.port)
            .field("has_api", &self.api.is_some())
            .field("last_port_used", &self.last_port_used)
            .field("screp_available", &self.screp_available)
            .finish()
    }
}

impl std::fmt::Debug for ReplayWatchState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplayWatchState")
            .field("has_storage", &self.storage.is_some())
            .field("last_mtime", &self.last_mtime)
            .field("last_processed_mtime", &self.last_processed_mtime)
            .field("changed_at", &self.changed_at)
            .field("last_dodge_candidate", &self.last_dodge_candidate)
            .finish()
    }
}

#[derive(Debug, Default)]
pub struct LayoutState {
    pub status_opponent_rect: Option<Rect>,
    pub main_opponent_toons_rect: Option<Rect>,
}

#[derive(Debug, Default)]
pub struct StatusState {
    pub last_profile_text: Option<String>,
}

pub struct App {
    pub should_quit: bool,
    pub view: View,
    pub debug: DebugState,
    pub detection: DetectionState,
    pub self_profile: SelfProfileState,
    pub opponent: OpponentState,
    pub overlays: OverlayState,
    pub replay: ReplayState,
    pub replay_watch: ReplayWatchState,
    pub search: SearchState,
    pub players: PlayersState,
    pub layout: LayoutState,
    pub status: StatusState,
}

impl App {
    pub fn new(debug_window_secs: i64) -> Self {
        let mut app = Self::default();
        app.debug.window_secs = debug_window_secs;
        app
    }

    pub fn reset_opponent_state(&mut self) {
        self.opponent.name = None;
        self.opponent.gateway = None;
        self.opponent.toons.clear();
        self.opponent.toons_data.clear();
        self.opponent.last_identity = None;
        self.opponent.race = None;
        self.overlays.opponent_last_text = None;
        self.opponent.matchups.clear();
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
        self.detection.port.is_some() && self.self_profile.name.is_some()
    }
}

impl Default for App {
    fn default() -> Self {
        Self {
            should_quit: false,
            view: View::Main,
            debug: DebugState {
                window_secs: 10,
                ..Default::default()
            },
            detection: DetectionState::default(),
            self_profile: SelfProfileState::default(),
            opponent: OpponentState::default(),
            overlays: OverlayState::default(),
            replay: ReplayState::default(),
            replay_watch: ReplayWatchState::default(),
            search: SearchState::default(),
            players: PlayersState::default(),
            layout: LayoutState::default(),
            status: StatusState::default(),
        }
    }
}
