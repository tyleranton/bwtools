use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

mod app;
mod api;
mod config;
mod cache;
mod history;
mod tui;
mod ui;
mod overlay;
mod replay;
mod detect;

use crate::app::App;
use crate::cache::CacheReader;
use crate::config::Config;
use crate::history::load_history;
use std::path::Path;
use crate::tui::{restore_terminal, setup_terminal, install_panic_hook};
use crate::ui::render;

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
    let cfg: Config = Default::default();
    let tick_rate = cfg.tick_rate;
    let mut last_tick = Instant::now();
    let mut last_refresh = Instant::now();
    app.debug_window_secs = cfg.debug_window_secs;

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
    app.screp_available = which::which(&cfg.screp_cmd).is_ok() && Path::new(&cfg.last_replay_path).exists();
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
                    if key.kind == KeyEventKind::Press {
                        if key.modifiers.contains(KeyModifiers::CONTROL) {
                            match key.code {
                                KeyCode::Char('d') => {
                                    app.view = match app.view {
                                        crate::app::View::Debug => crate::app::View::Main,
                                        _ => crate::app::View::Debug,
                                    };
                                    if matches!(app.view, crate::app::View::Debug) {
                                        app.debug_scroll = 0;
                                    }
                                }
                                KeyCode::Char('s') => {
                                    app.view = crate::app::View::Search;
                                    app.search_focus_gateway = false;
                                    app.search_cursor = app.search_name.chars().count();
                                }
                                KeyCode::Char('m') => { app.view = crate::app::View::Main; }
                                KeyCode::Char('q') => { app.should_quit = true; }
                                _ => {}
                            }
                        } else {
                            match key.code {
                                KeyCode::Esc => app.should_quit = true,
                                other => app.on_key(other),
                            }
                        }
                    }
                }
                Event::Mouse(me) => {
                    use crossterm::event::{MouseEventKind, MouseButton};
                    if let MouseEventKind::Down(MouseButton::Left) = me.kind {
                        let x = me.column;
                        let y = me.row;
                        match app.view {
                            crate::app::View::Main => {
                                if let Some(rect) = app.status_opponent_rect {
                                    if x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height {
                                        if let (Some(name), Some(gw)) = (&app.profile_name, app.gateway) {
                                            app.view = crate::app::View::Search;
                                            app.search_name = name.clone();
                                            app.search_gateway = gw;
                                            app.search_focus_gateway = false;
                                            app.search_cursor = app.search_name.chars().count();
                                            app.search_matches_scroll = 0;
                                            app.search_in_progress = true;
                                        }
                                    }
                                }
                                if let Some(rect) = app.main_opponent_toons_rect {
                                    if x >= rect.x && y >= rect.y && y < rect.y + rect.height {
                                        let idx = (y - rect.y) as usize;
                                        // idx 0: header (opponent); others: filtered list
                                        if idx == 0 {
                                            if let (Some(name), Some(gw)) = (&app.profile_name, app.gateway) {
                                                let rating = app
                                                    .opponent_toons_data
                                                    .iter()
                                                    .find(|(t, _, _)| t.eq_ignore_ascii_case(name))
                                                    .map(|(_, _, r)| *r);
                                                let race_opt = app.opponent_race.clone();
                                                let mut parts: Vec<String> = vec![
                                                    name.clone(),
                                                    crate::api::gateway_label(gw).to_string(),
                                                ];
                                                if let Some(race) = race_opt { parts.push(race); }
                                                if let Some(r) = rating { parts.push(r.to_string()); }
                                                let head_text = parts.join(" • ");
                                                let head_width = head_text.chars().count() as u16;
                                                if x < rect.x + head_width {
                                                    app.view = crate::app::View::Search;
                                                    app.search_name = name.clone();
                                                    app.search_gateway = gw;
                                                    app.search_focus_gateway = false;
                                                    app.search_cursor = app.search_name.chars().count();
                                                    app.search_matches_scroll = 0;
                                                    app.search_in_progress = true;
                                                }
                                            }
                                        } else {
                                            // Build filtered others to match UI order
                                            let others: Vec<(String, u16, u32)> = app
                                                .opponent_toons_data
                                                .iter()
                                                .filter(|(t, _, _)|
                                                    app.profile_name.as_ref().map(|n| !t.eq_ignore_ascii_case(n)).unwrap_or(true)
                                                )
                                                .cloned()
                                                .collect();
                                            let sel = idx - 1;
                                            if sel < others.len() {
                                                let display_text = format!(
                                                    "{} • {} • {}",
                                                    others[sel].0,
                                                    crate::api::gateway_label(others[sel].1),
                                                    others[sel].2
                                                );
                                                let text_width = display_text.chars().count() as u16;
                                                if x < rect.x + text_width {
                                                    let (toon, gw, _r) = others[sel].clone();
                                                    app.view = crate::app::View::Search;
                                                    app.search_name = toon;
                                                    app.search_gateway = gw;
                                                    app.search_focus_gateway = false;
                                                    app.search_cursor = app.search_name.chars().count();
                                                    app.search_matches_scroll = 0;
                                                    app.search_in_progress = true;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            crate::app::View::Search => {
                                if let Some(rect) = app.search_other_toons_rect {
                                    if x >= rect.x && y >= rect.y && y < rect.y + rect.height {
                                        let idx = (y - rect.y) as usize;
                                        if idx < app.search_other_toons_data.len() {
                                            let text_width = app.search_other_toons.get(idx).map(|s| s.chars().count() as u16).unwrap_or(0);
                                            if x < rect.x + text_width {
                                                let (toon, gw, _r) = app.search_other_toons_data[idx].clone();
                                                app.search_name = toon;
                                                app.search_gateway = gw;
                                                app.search_focus_gateway = false;
                                                app.search_cursor = app.search_name.chars().count();
                                                app.search_matches_scroll = 0;
                                                app.search_in_progress = true;
                                            }
                                        }
                                    }
                                }
                            }
                            crate::app::View::Debug => {}
                        }
                    }
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
                    if let (Some(api), Some(name), Some(gw)) = (&app.api, &app.self_profile_name, app.self_gateway) {
                        match api.get_toon_info(name, gw) {
                            Ok(info) => {
                                let mut out = String::new();
                                out.push_str(&format!("profiles ({}):\n", info.profiles.len()));
                                for (i, p) in info.profiles.iter().enumerate() {
                                    out.push_str(&format!(
                                        "{:>3}. title={}, toon={}, toon_guid={}, private={}\n",
                                        i + 1,
                                        p.title,
                                        p.toon,
                                        p.toon_guid,
                                        p.private
                                    ));
                                }
                                app.last_profile_text = Some(out);
                                app.self_profile_rating = api.compute_rating_for_name(&info, name);
                                
                                app.own_profiles = info.profiles.iter().map(|p| p.toon.clone()).collect();
                                // Fetch profile for stats
                                                if let Ok(profile) = api.get_scr_profile(name, gw) {
                                                    let (mr, lines, _results) = api.profile_stats_last100(&profile, name);
                                                    app.self_main_race = mr;
                                                    app.self_matchups = lines;
                                                }
                                app.last_rating_poll = Some(Instant::now());
                                app.profile_fetched = true;
                                        overlay::write_rating(&cfg, app);
                            }
                            Err(err) => {
                                app.last_profile_text = Some(format!("API error: {}", err));
                                app.last_rating_poll = Some(Instant::now());
                                app.profile_fetched = true;
                            }
                        }
                    }
                }

                if app.search_in_progress {
                    app.search_in_progress = false;
                    app.search_error = None;
                    app.search_rating = None;
                    app.search_other_toons.clear();
                    app.search_matches.clear();
                    app.search_matches_scroll = 0;
                    app.search_main_race = None;
                    app.search_matchups.clear();
                    if let (Some(api), true) = (&app.api, !app.search_name.trim().is_empty()) {
                        let name = app.search_name.trim().to_string();
                        let gw = app.search_gateway;
                        match api.get_toon_info(&name, gw) {
                            Ok(info) => {
                                let season = info.matchmaked_current_season;
                                let guid = info
                                    .profiles
                                    .iter()
                                    .find(|p| p.toon.eq_ignore_ascii_case(&name))
                                    .map(|p| p.toon_guid)
                                    .or_else(|| info
                                        .matchmaked_stats
                                        .iter()
                                        .find(|s| s.season_id == season && s.toon.eq_ignore_ascii_case(&name))
                                        .map(|s| s.toon_guid)
                                    );
                                app.search_rating = guid.and_then(|g| api.compute_rating_for_guid(&info, g));
                                let others = api.other_toons_with_ratings(&info, &name);
                                app.search_other_toons_data = others.clone();
                                app.search_other_toons = others
                                    .into_iter()
                                    .map(|(toon, gw_num, rating)| format!("{} • {} • {}", toon, crate::api::gateway_label(gw_num), rating))
                                    .collect();
                                // Only show matches if current season total games across buckets >= threshold
                                let eligible = guid.map(|g| {
                                    let season = info.matchmaked_current_season;
                                    info.matchmaked_stats
                                        .iter()
                                        .filter(|s| s.toon_guid == g && s.season_id == season)
                                        .fold(0u32, |acc, s| acc.saturating_add(s.wins + s.losses))
                                }).map(|n| n >= crate::api::RATING_MIN_GAMES).unwrap_or(false);
                                match api.get_scr_profile(&name, gw) {
                                    Ok(profile) => {
                                        // matches list only if eligible (>=5)
                                        if eligible { app.search_matches = api.match_summaries(&profile, &name); } else { app.search_matches.clear(); }
                                        // always compute stats from last 100 (ignore results in Search for now)
                                        let (mr, lines, _results) = api.profile_stats_last100(&profile, &name);
                                        app.search_main_race = mr;
                                        app.search_matchups = lines;
                                    }
                                    Err(e) => { app.search_error = Some(format!("profile error: {}", e)); }
                                }
                            }
                            Err(e) => { app.search_error = Some(e.to_string()); }
                        }
                    }
                }

                if app.is_ready() {
                    // If screp is not available, continue periodic rating poll as fallback
                    if !app.screp_available {
                        let due = app.last_rating_poll.map_or(true, |t| t.elapsed() >= cfg.rating_poll_interval);
                        if due {
                            if let (Some(api), Some(name), Some(gw)) = (&app.api, &app.self_profile_name, app.self_gateway) {
                                match api.get_toon_info(name, gw) {
                                    Ok(info) => {
                                        app.self_profile_rating = api.compute_rating_for_name(&info, name);
                                        app.last_rating_poll = Some(Instant::now());
                                        if let Ok(profile) = api.get_scr_profile(name, gw) {
                                            let (mr, lines, _results) = api.profile_stats_last100(&profile, name);
                                            app.self_main_race = mr;
                                            app.self_matchups = lines;
                                        }
                                        overlay::write_rating(&cfg, app);
                                    }
                                    Err(_) => {
                                        // Still update the timestamp to avoid hammering on repeated failures
                                        app.last_rating_poll = Some(Instant::now());
                                    }
                                }
                            }
                        }
                    }
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
            last_tick = Instant::now();
        }
    }

    Ok(())
}

fn main() -> io::Result<()> {
    install_panic_hook();
    let mut terminal = setup_terminal()?;
    let res = (|| {
        let mut app = App::new();
        run_app(&mut terminal, &mut app)
    })();
    let _ = restore_terminal(&mut terminal);
    res
}
