use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::app::{App, View};
use crate::cache::CacheReader;
use crate::config::Config;
use crate::detect::DetectionService;
use crate::error::AppError;
use crate::history::{FileHistorySource, HistoryService};
use crate::overlay::OverlayService;
use crate::players::PlayerDirectory;
use crate::profile::ProfileService;
use crate::replay::ReplayService;
use crate::replay_download::{ReplayDownloadRequest, ReplayStorage};
use crate::search::SearchService;
use crate::tui::{restore_terminal, setup_terminal};
use crate::ui::render;
use which::which;

pub struct AppRuntime {
    cfg: Config,
    app: App,
    terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
    tick_rate: Duration,
    last_tick: Instant,
    detection: DetectionEngine,
    history: Option<HistoryService<FileHistorySource>>,
}

impl AppRuntime {
    pub fn new(cfg: Config) -> Result<Self, AppError> {
        let terminal = setup_terminal().map_err(AppError::TerminalSetup)?;
        let app = App {
            debug_window_secs: cfg.debug_window_secs,
            ..Default::default()
        };

        let detection = DetectionEngine::new(cfg.cache_dir.clone(), cfg.refresh_interval);

        let mut runtime = Self {
            tick_rate: cfg.tick_rate,
            last_tick: Instant::now(),
            cfg,
            app,
            terminal,
            detection,
            history: None,
        };
        runtime.bootstrap()?;
        Ok(runtime)
    }

    pub fn run(&mut self) -> Result<(), AppError> {
        while !self.app.should_quit {
            self.terminal
                .draw(|f| render(f, &mut self.app))
                .map_err(AppError::TerminalRender)?;

            let timeout = self
                .tick_rate
                .checked_sub(self.last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            if event::poll(timeout).map_err(AppError::TerminalRender)? {
                match event::read().map_err(AppError::TerminalRender)? {
                    Event::Key(key) => {
                        crate::input::handle_key_event(&mut self.app, key);
                    }
                    Event::Mouse(me) => {
                        crate::input::handle_mouse_event(&mut self.app, me);
                    }
                    _ => {}
                }
            }

            if self.last_tick.elapsed() >= self.tick_rate {
                if let Err(err) =
                    self.detection
                        .tick(&mut self.app, &self.cfg, self.history.as_ref())
                {
                    tracing::error!(error = %err, "detection tick failed");
                }

                if self.app.is_ready()
                    && !self.app.profile_fetched
                    && let Err(err) = ProfileService::fetch_self_profile(&mut self.app, &self.cfg)
                {
                    tracing::error!(error = %err, "fetch self profile failed");
                    self.app.last_profile_text = some_text("Profile error", &err);
                }
                if self.app.search_in_progress
                    && let Err(err) = SearchService::run(&mut self.app)
                {
                    tracing::warn!(error = %err, "search run failed");
                }
                if self.app.is_ready()
                    && let Err(err) = ProfileService::poll_self_rating(&mut self.app, &self.cfg)
                {
                    tracing::error!(error = %err, "poll self rating failed");
                    self.app.last_profile_text = some_text("Profile error", &err);
                }

                if let Err(err) = self.handle_pending_replay_download() {
                    tracing::error!(error = %err, "replay download start failed");
                }
                self.app.poll_replay_job();

                if let Err(err) =
                    ReplayService::tick(&mut self.app, &self.cfg, self.history.as_ref())
                {
                    tracing::error!(error = %err, "replay service tick failed");
                    self.app.last_profile_text = some_text("Replay error", &err);
                }

                if let Err(err) = OverlayService::write_opponent(&self.cfg, &mut self.app) {
                    tracing::error!(error = %err, "failed to update opponent overlay");
                    self.app.last_profile_text = some_text("Overlay error", &err);
                }

                self.last_tick = Instant::now();
            }
        }

        Ok(())
    }

    pub fn shutdown(&mut self) -> Result<(), AppError> {
        restore_terminal(&mut self.terminal).map_err(AppError::TerminalRestore)
    }

    fn bootstrap(&mut self) -> Result<(), AppError> {
        let storage = ReplayStorage::new(self.cfg.replay_library_root.clone());
        if let Err(err) = storage.ensure_base_dirs() {
            tracing::error!(error = %err, "failed to ensure replay directories");
            self.app.last_profile_text = Some(format!("Replay dir error: {err}"));
        }
        self.app.replay_storage = Some(storage);

        let history = HistoryService::new(FileHistorySource::new(
            self.cfg.opponent_history_path.clone(),
        ));
        match history.load() {
            Ok(hist) => self.app.opponent_history = hist,
            Err(err) => {
                tracing::error!(error = %err, "failed to load opponent history");
                self.app.opponent_history = Default::default();
                self.app.last_profile_text = Some(format!("History load error: {err}"));
            }
        }
        self.history = Some(history);

        match PlayerDirectory::load() {
            Ok(directory) => self.app.set_player_directory(directory),
            Err(err) => tracing::error!(error = %err, "failed to load player list"),
        }

        self.app.screp_available =
            which(&self.cfg.screp_cmd).is_ok() && Path::new(&self.cfg.last_replay_path).exists();
        if let Ok(meta) = std::fs::metadata(&self.cfg.last_replay_path) {
            self.app.last_replay_mtime = meta.modified().ok();
            self.app.last_replay_processed_mtime = self.app.last_replay_mtime;
        }

        Ok(())
    }

    fn handle_pending_replay_download(&mut self) -> Result<(), AppError> {
        if !self.app.replay_should_start {
            return Ok(());
        }
        self.app.replay_should_start = false;
        if self.app.replay_in_progress {
            self.app.replay_last_error = Some("Replay download already running".to_string());
            return Ok(());
        }
        let toon = self.app.replay_toon_input.trim();
        if toon.is_empty() {
            self.app.replay_last_error = Some("Enter a profile name first".to_string());
            return Ok(());
        }
        let port = self
            .app
            .port
            .or(self.app.last_port_used)
            .unwrap_or_default();
        if port == 0 {
            self.app.replay_last_error = Some("No API port detected".to_string());
            return Ok(());
        }
        let base_url = format!("http://127.0.0.1:{port}");
        let request = ReplayDownloadRequest {
            toon: toon.to_string(),
            gateway: self.app.replay_input_gateway,
            matchup: match self.app.replay_matchup_input.trim() {
                "" => None,
                other => Some(other.to_string()),
            },
            limit: self.app.replay_input_count.max(1) as usize,
            alias: match self.app.replay_alias_input.trim() {
                "" => None,
                other => Some(other.to_string()),
            },
        };

        if let Some(handle) = self.app.replay_job_handle.take() {
            let _ = handle.join();
        }
        self.app.replay_last_error = None;
        self.app.replay_last_summary = None;
        self.app.replay_last_request = Some(request.clone());

        let cfg = self.cfg.clone();
        let (handle, rx) = crate::replay_download::spawn_download_job(base_url, cfg, request);
        self.app.replay_job_rx = Some(rx);
        self.app.replay_job_handle = Some(handle);
        self.app.replay_in_progress = true;
        Ok(())
    }
}

fn some_text(prefix: &str, err: &dyn std::fmt::Display) -> Option<String> {
    Some(format!("{prefix}: {err}"))
}

struct DetectionEngine {
    reader: Option<CacheReader>,
    cache_dir: std::path::PathBuf,
    refresh_interval: Duration,
    last_refresh: Instant,
}

impl DetectionEngine {
    fn new(cache_dir: std::path::PathBuf, refresh_interval: Duration) -> Self {
        Self {
            reader: None,
            cache_dir,
            refresh_interval,
            last_refresh: Instant::now(),
        }
    }

    fn tick(
        &mut self,
        app: &mut App,
        cfg: &Config,
        history: Option<&HistoryService<FileHistorySource>>,
    ) -> Result<(), AppError> {
        if self.reader.is_none() {
            match CacheReader::new(self.cache_dir.clone()) {
                Ok(reader) => self.reader = Some(reader),
                Err(err) => {
                    return Err(AppError::runtime("open chrome cache", err));
                }
            }
        }

        if let Some(reader) = self.reader.as_mut() {
            if self.last_refresh.elapsed() >= self.refresh_interval {
                if let Err(err) = reader.refresh() {
                    return Err(AppError::runtime("refresh chrome cache", err));
                }
                self.last_refresh = Instant::now();
            }

            DetectionService::tick(app, cfg, reader, history)
                .map_err(|err| AppError::runtime("detection tick", err))?;

            if app.view == View::Debug {
                match reader.recent_keys(app.debug_window_secs, 20) {
                    Ok(list) => {
                        app.debug_recent = list
                            .into_iter()
                            .map(|(k, age)| format!("{age:>2}s • {}", k))
                            .collect();
                    }
                    Err(err) => {
                        return Err(AppError::runtime("list recent cache keys", err));
                    }
                }
            }
        }

        Ok(())
    }
}
