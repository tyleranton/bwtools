use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::app::{App, View};
use crate::cache::CacheReader;
use crate::config::Config;
use crate::detect::DetectionService;
use crate::error::AppError;
use crate::history::{FileHistorySource, HistoryService};
use crate::interaction::Intent;
use crate::overlay::OverlayService;
use crate::profile::ProfileService;
use crate::profile_history::ProfileHistoryService;
use crate::replay::ReplayService;
use crate::replay_download::{ReplayDownloadRequest, ReplayStorage};
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
    profile_history: ProfileHistoryService,
}

impl AppRuntime {
    pub fn new(cfg: Config) -> Result<Self, AppError> {
        let terminal = setup_terminal().map_err(AppError::TerminalSetup)?;
        let app = App::new(cfg.debug_window_secs);

        let detection = DetectionEngine::new(cfg.cache_dir.clone(), cfg.refresh_interval);

        let profile_history = match ProfileHistoryService::new(cfg.profile_history_path.clone()) {
            Ok(service) => service,
            Err(err) => {
                tracing::error!(error = %err, "failed to load profile history; starting empty");
                ProfileHistoryService::empty(cfg.profile_history_path.clone())
            }
        };

        let mut runtime = Self {
            tick_rate: cfg.tick_rate,
            last_tick: Instant::now(),
            cfg,
            app,
            terminal,
            detection,
            history: None,
            profile_history,
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

            if event::poll(timeout).map_err(AppError::TerminalRender)?
                && let Event::Key(key) = event::read().map_err(AppError::TerminalRender)?
            {
                handle_key_event(&mut self.app, key);
            }

            if self.last_tick.elapsed() >= self.tick_rate {
                if let Err(err) =
                    self.detection
                        .tick(&mut self.app, &self.cfg, self.history.as_ref())
                {
                    tracing::error!(error = %err, "detection tick failed");
                }

                if self.app.is_ready()
                    && !self.app.self_profile.profile_fetched
                    && let Err(err) = ProfileService::fetch_self_profile(
                        &mut self.app,
                        &self.cfg,
                        Some(&mut self.profile_history),
                    )
                {
                    tracing::error!(error = %err, "fetch self profile failed");
                    self.app.status.last_profile_text = some_text("Profile error", &err);
                }
                if self.app.is_ready()
                    && let Err(err) = ProfileService::poll_self_rating(
                        &mut self.app,
                        &self.cfg,
                        Some(&mut self.profile_history),
                    )
                {
                    tracing::error!(error = %err, "poll self rating failed");
                    self.app.status.last_profile_text = some_text("Profile error", &err);
                }

                if let Err(err) = self.handle_pending_replay_download() {
                    tracing::error!(error = %err, "replay download start failed");
                }
                self.app.poll_replay_job();

                if let Err(err) = ReplayService::tick(
                    &mut self.app,
                    &self.cfg,
                    self.history.as_ref(),
                    &mut self.profile_history,
                ) {
                    tracing::error!(error = %err, "replay service tick failed");
                    self.app.status.last_profile_text = some_text("Replay error", &err);
                }

                if let Err(err) = OverlayService::write_opponent(&self.cfg, &mut self.app) {
                    tracing::error!(error = %err, "failed to update opponent overlay");
                    self.app.status.last_profile_text = some_text("Overlay error", &err);
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
            self.app.status.last_profile_text = some_text("Replay dir error", &err);
        }
        self.app.replay_watch.storage = Some(storage);

        let history = HistoryService::new(FileHistorySource::new(
            self.cfg.opponent_history_path.clone(),
        ));
        match history.load() {
            Ok(hist) => self.app.opponent.history = hist,
            Err(err) => {
                tracing::error!(error = %err, "failed to load opponent history");
                self.app.opponent.history = Default::default();
                self.app.status.last_profile_text = some_text("History load error", &err);
            }
        }
        self.history = Some(history);

        self.app.detection.screp_available =
            which(&self.cfg.screp_cmd).is_ok() && Path::new(&self.cfg.last_replay_path).exists();
        if let Ok(meta) = std::fs::metadata(&self.cfg.last_replay_path) {
            self.app.replay_watch.last_mtime = meta.modified().ok();
            self.app.replay_watch.last_processed_mtime = self.app.replay_watch.last_mtime;
        }

        Ok(())
    }

    fn handle_pending_replay_download(&mut self) -> Result<(), AppError> {
        if !self.app.replay.should_start {
            return Ok(());
        }
        self.app.replay.should_start = false;
        if self.app.replay.in_progress {
            self.app.replay.last_error = Some("Replay download already running".to_string());
            return Ok(());
        }
        let toon = self.app.replay.toon_input.trim();
        if toon.is_empty() {
            self.app.replay.last_error = Some("Enter a profile name first".to_string());
            return Ok(());
        }
        let port = self
            .app
            .detection
            .port
            .or(self.app.detection.last_port_used)
            .unwrap_or_default();
        if port == 0 {
            self.app.replay.last_error = Some("No API port detected".to_string());
            return Ok(());
        }
        let base_url = format!("http://127.0.0.1:{port}");
        let request = ReplayDownloadRequest {
            toon: toon.to_string(),
            gateway: self.app.replay.input_gateway,
            matchup: match self.app.replay.matchup_input.trim() {
                "" => None,
                other => Some(other.to_string()),
            },
            limit: self.app.replay.input_count.max(1) as usize,
            alias: match self.app.replay.alias_input.trim() {
                "" => None,
                other => Some(other.to_string()),
            },
        };

        if let Some(handle) = self.app.replay.job_handle.take() {
            let _ = handle.join();
        }
        self.app.replay.last_error = None;
        self.app.replay.last_summary = None;
        self.app.replay.last_request = Some(request.clone());

        let cfg = self.cfg.clone();
        let (handle, rx) = crate::replay_download::spawn_download_job(base_url, cfg, request);
        self.app.replay.job_rx = Some(rx);
        self.app.replay.job_handle = Some(handle);
        self.app.replay.in_progress = true;
        Ok(())
    }
}

fn some_text<E>(prefix: &str, err: &E) -> Option<String>
where
    E: std::fmt::Display + std::fmt::Debug,
{
    let rendered = crate::error::render_error_message(err);
    Some(format!("{prefix}: {rendered}"))
}

fn handle_key_event(app: &mut App, key: KeyEvent) {
    if key.kind != KeyEventKind::Press {
        return;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('d') => Intent::ToggleDebug.apply(app),
            KeyCode::Char('m') => Intent::ShowMain.apply(app),
            KeyCode::Char('r') => Intent::ShowReplays.apply(app),
            KeyCode::Char('q') => Intent::Quit.apply(app),
            _ => {}
        }
    } else {
        match key.code {
            KeyCode::Esc => Intent::Quit.apply(app),
            other => app.on_key(other),
        }
    }
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
                match reader.recent_keys(app.debug.window_secs, 20) {
                    Ok(list) => {
                        app.debug.recent = list
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
