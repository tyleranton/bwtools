use std::io;
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
    let cfg: Config = Default::default();
    let tick_rate = cfg.tick_rate;
    let mut last_tick = Instant::now();
    let mut last_refresh = Instant::now();
    app.debug_window_secs = cfg.debug_window_secs;

    let storage = ReplayStorage::new(cfg.replay_library_root.clone());
    if let Err(err) = storage.ensure_base_dirs() {
        app.last_profile_text = Some(format!("Replay dir error: {}", err));
    }
    app.replay_storage = Some(storage);

    // Load opponent history
    app.opponent_history = load_history(&cfg.opponent_history_path);

    let mut reader = match CacheReader::new(cfg.cache_dir.clone()) {
        Ok(r) => Some(r),
        Err(e) => {
            app.last_profile_text = Some(format!("Cache error: {}", e));
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
            if let Some(ref mut r) = reader {
                if last_refresh.elapsed() >= cfg.refresh_interval {
                    if let Err(e) = r.refresh() {
                        app.last_profile_text = Some(format!("Cache refresh error: {}", e));
                    }
                    last_refresh = Instant::now();
                }

                crate::detect::tick_detection(app, &cfg, r);

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
                overlay::write_opponent(&cfg, app);

                if matches!(app.view, crate::app::View::Debug) {
                    if let Ok(list) = r.recent_keys(app.debug_window_secs, 20) {
                        app.debug_recent = list
                            .into_iter()
                            .map(|(k, age)| format!("{age:>2}s • {}", k))
                            .collect();
                    }
                }
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
