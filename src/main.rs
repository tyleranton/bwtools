use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

mod api;
mod app;
mod cache;
mod config;
mod detect;
mod history;
mod input;
mod overlay;
mod profile;
mod replay;
mod replay_download;
mod search;
mod tui;
mod ui;

use crate::app::App;
use crate::cache::CacheReader;
use crate::config::Config;
use crate::history::load_history;
use crate::replay_download::{ReplayDownloadRequest, ReplayStorage};
use crate::tui::{install_panic_hook, restore_terminal, setup_terminal};
use crate::ui::render;
use std::path::Path;

static TRACING_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();
static TRACING_INIT: OnceLock<()> = OnceLock::new();

fn init_logging() {
    TRACING_INIT.get_or_init(|| {
        let log_dir = Path::new("logs");
        if let Err(err) = std::fs::create_dir_all(log_dir) {
            eprintln!(
                "failed to create log directory {}: {err}",
                log_dir.display()
            );
            return;
        }

        let file_appender = tracing_appender::rolling::daily(log_dir, "bwtools.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        let _ = TRACING_GUARD.set(guard);

        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(non_blocking)
            .with_ansi(false)
            .init();
    });
}

fn maybe_start_replay_download(app: &mut App, cfg: &Config) {
    if !app.replay_should_start {
        return;
    }
    app.replay_should_start = false;
    if app.replay_in_progress {
        app.replay_last_error = Some("Replay download already running".to_string());
        return;
    }
    let toon = app.replay_toon_input.trim();
    if toon.is_empty() {
        app.replay_last_error = Some("Enter a profile name first".to_string());
        return;
    }
    let port = app.port.or(app.last_port_used).unwrap_or_default();
    if port == 0 {
        app.replay_last_error = Some("No API port detected".to_string());
        return;
    }
    let base_url = format!("http://127.0.0.1:{port}");
    let request = ReplayDownloadRequest {
        toon: toon.to_string(),
        gateway: app.replay_input_gateway,
        matchup: match app.replay_matchup_input.trim() {
            "" => None,
            other => Some(other.to_string()),
        },
        limit: app.replay_input_count.max(1) as usize,
        alias: match app.replay_alias_input.trim() {
            "" => None,
            other => Some(other.to_string()),
        },
    };

    if let Some(handle) = app.replay_job_handle.take() {
        let _ = handle.join();
    }
    app.replay_last_error = None;
    app.replay_last_summary = None;
    app.replay_last_request = Some(request.clone());

    let (handle, rx) = crate::replay_download::spawn_download_job(base_url, cfg.clone(), request);
    app.replay_job_rx = Some(rx);
    app.replay_job_handle = Some(handle);
    app.replay_in_progress = true;
}

#[allow(clippy::collapsible_if)]
fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
    init_logging();
    let cfg: Config = Default::default();
    let tick_rate = cfg.tick_rate;
    let mut last_tick = Instant::now();
    let mut last_refresh = Instant::now();
    app.debug_window_secs = cfg.debug_window_secs;

    let storage = ReplayStorage::new(cfg.replay_library_root.clone());
    if let Err(err) = storage.ensure_base_dirs() {
        tracing::error!(error = %err, "failed to ensure replay directories");
        app.last_profile_text = Some(format!("Replay dir error: {err}"));
    }
    app.replay_storage = Some(storage);

    // Load opponent history
    match load_history(&cfg.opponent_history_path) {
        Ok(hist) => app.opponent_history = hist,
        Err(err) => {
            tracing::error!(error = %err, "failed to load opponent history");
            app.opponent_history = Default::default();
            app.last_profile_text = Some(format!("History load error: {err}"));
        }
    }

    let mut reader = match CacheReader::new(cfg.cache_dir.clone()) {
        Ok(r) => Some(r),
        Err(e) => {
            tracing::error!(error = %e, "failed to open cache");
            app.last_profile_text = Some(format!("Cache error: {e}"));
            None
        }
    };

    // Initialize screp availability and baseline replay mtime
    app.screp_available =
        which::which(&cfg.screp_cmd).is_ok() && Path::new(&cfg.last_replay_path).exists();
    if let Ok(meta) = std::fs::metadata(&cfg.last_replay_path) {
        app.last_replay_mtime = meta.modified().ok();
        app.last_replay_processed_mtime = app.last_replay_mtime;
    }

    while !app.should_quit {
        terminal.draw(|f| render(f, app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    crate::input::handle_key_event(app, key);
                }
                Event::Mouse(me) => {
                    crate::input::handle_mouse_event(app, me);
                }
                _ => {}
            }
        }

        if last_tick.elapsed() >= tick_rate {
            if reader.is_none() {
                match CacheReader::new(cfg.cache_dir.clone()) {
                    Ok(r) => reader = Some(r),
                    Err(e) => {
                        tracing::error!(error = %e, "failed to reopen cache");
                        app.last_profile_text = Some(format!("Cache error: {e}"));
                    }
                }
            }

            let mut cache_panicked = false;
            if let Some(ref mut r) = reader {
                if last_refresh.elapsed() >= cfg.refresh_interval {
                    if let Err(e) = r.refresh() {
                        tracing::warn!(error = %e, "cache refresh failed");
                    }
                    last_refresh = Instant::now();
                }

                let detect_result = catch_unwind(AssertUnwindSafe(|| {
                    crate::detect::tick_detection(app, &cfg, r);
                }));

                if detect_result.is_err() {
                    cache_panicked = true;
                } else {
                    if app.is_ready() && !app.profile_fetched {
                        crate::profile::fetch_self_profile(app, &cfg);
                    }

                    if app.search_in_progress {
                        crate::search::run_search(app);
                    }
                    if app.is_ready() {
                        crate::profile::poll_self_rating(app, &cfg);
                        crate::replay::tick_replay_and_rating_retry(app, &cfg);
                    }
                    // Update opponent overlay text once per tick after potential updates
                    if let Err(err) = overlay::write_opponent(&cfg, app) {
                        tracing::error!(error = %err, "failed to update opponent overlay");
                        app.last_profile_text = Some(format!("Overlay error: {err}"));
                    }

                    if matches!(app.view, crate::app::View::Debug) {
                        match r.recent_keys(app.debug_window_secs, 20) {
                            Ok(list) => {
                                app.debug_recent = list
                                    .into_iter()
                                    .map(|(k, age)| format!("{age:>2}s • {}", k))
                                    .collect();
                            }
                            Err(err) => {
                                tracing::warn!(error = %err, "failed to list recent cache keys")
                            }
                        }
                    }
                }
            }

            if cache_panicked {
                tracing::error!("cache scan panicked; retrying shortly");
                app.last_profile_text = Some("Cache scan panicked; retrying shortly".to_string());
                reader = None;
                last_tick = Instant::now();
                continue;
            }
            maybe_start_replay_download(app, &cfg);
            app.poll_replay_job();
            last_tick = Instant::now();
        }
    }

    Ok(())
}

fn main() -> io::Result<()> {
    install_panic_hook();
    let mut terminal = setup_terminal()?;
    let res = {
        let mut app = App::default();
        run_app(&mut terminal, &mut app)
    };
    let _ = restore_terminal(&mut terminal);
    res
}
