use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

mod app;
mod api;
mod config;
mod cache;
mod tui;
mod ui;

use crate::app::App;
use crate::cache::CacheReader;
use crate::config::Config;
use crate::tui::{restore_terminal, setup_terminal};
use crate::ui::render;

fn compute_self_rating(info: &bw_web_api_rs::models::aurora_profile::ScrToonInfo, profile_name: &str) -> Option<u32> {
    let toon_guid = info.profiles.iter().find(|p| p.toon == profile_name).map(|p| p.toon_guid)?;
    let season = info.matchmaked_current_season;
    let iter = info.matchmaked_stats.iter().filter(|s| s.toon_guid == toon_guid);
    if let Some(r) = iter.clone().filter(|s| s.season_id == season).max_by_key(|s| s.rating).map(|s| s.rating) {
        return Some(r);
    }
    iter.max_by_key(|s| s.rating).map(|s| s.rating)
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
    let cfg: Config = Default::default();
    let tick_rate = cfg.tick_rate;
    let mut last_tick = Instant::now();
    let mut last_refresh = Instant::now();
    app.debug_window_secs = cfg.debug_window_secs;

    let mut reader = CacheReader::new(cfg.cache_dir.clone()).ok();

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
                                                let head_text = match rating {
                                                    Some(r) => format!("{} • {} • Rating {}", name, crate::api::gateway_label(gw), r),
                                                    None => format!("{} • {}", name, crate::api::gateway_label(gw)),
                                                };
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
                                                    "{} • {} • Rating {}",
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
                    let _ = r.refresh();
                    last_refresh = Instant::now();
                }

                if app.port.is_none() {
                    if let Ok(port_opt) = r.parse_for_port(cfg.scan_window_secs) {
                        if port_opt.is_some() { app.port = port_opt; }
                    }
                }

                if app.port.is_some() && app.self_profile_name.is_none() {
                    if let Ok(self_opt) = r.latest_self_profile(cfg.scan_window_secs) {
                        if let Some((name, gw)) = self_opt {
                            app.self_profile_name = Some(name);
                            app.self_gateway = Some(gw);
                        }
                    }
                }

                if let Some(p) = app.port {
                    if app.api.is_none() || app.last_port_used != Some(p) {
                        let base_url = format!("http://127.0.0.1:{p}");
                        app.api = crate::api::ApiHandle::new(base_url).ok();
                        app.last_port_used = Some(p);
                    }
                }

                if app.is_ready() {
                    if let Ok(self_mm_opt) = r.latest_mmgameloading_profile(cfg.scan_window_secs) {
                        if let Some((mm_name, mm_gw)) = self_mm_opt {
                            if app.own_profiles.contains(&mm_name) {
                                if app.self_profile_name.as_deref() != Some(&mm_name) || app.self_gateway != Some(mm_gw) {
                                    app.self_profile_name = Some(mm_name);
                                    app.self_gateway = Some(mm_gw);
                                    app.self_profile_rating = None;
                                    app.profile_fetched = false;
                                    app.last_profile_text = None;
                                    app.last_rating_poll = None;
                                    app.last_opponent_identity = None;
                                    app.opponent_toons.clear();
                                }
                            }
                        }
                    }
                }

                if app.is_ready() {
                    if let Ok(profile_opt) = r.latest_opponent_profile(app.self_profile_name.as_deref(), cfg.scan_window_secs) {
                        if let Some((name, gw)) = profile_opt {
                            if !app.own_profiles.contains(&name) {
                                app.profile_name = Some(name);
                                app.gateway = Some(gw);
                                if let (Some(api), Some(opp_name), Some(opp_gw)) = (&app.api, &app.profile_name, app.gateway) {
                                    let identity = (opp_name.clone(), opp_gw);
                                    if app.last_opponent_identity.as_ref() != Some(&identity) {
                                        if let Ok(list) = api.opponent_toons_summary(opp_name, opp_gw) {
                                        app.opponent_toons_data = list.clone();
                                        app.opponent_toons = list
                                            .into_iter()
                                            .map(|(toon, gw_num, rating)| format!("{} • {} • Rating {}", toon, crate::api::gateway_label(gw_num), rating))
                                            .collect();
                                            app.last_opponent_identity = Some(identity);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

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
                                app.self_profile_rating = compute_self_rating(&info, name);
                                
                                app.own_profiles = info.profiles.iter().map(|p| p.toon.clone()).collect();
                                // Fetch profile for stats
                                if let Ok(profile) = api.get_scr_profile(name, gw) {
                                    let (mr, lines, _results) = api.profile_stats_last100(&profile, name);
                                    app.self_main_race = mr;
                                    app.self_matchups = lines;
                                }
                                app.last_rating_poll = Some(Instant::now());
                                app.profile_fetched = true;
                            }
                            Err(err) => {
                                app.last_profile_text = Some(format!("API error: {}", err));
                                app.last_rating_poll = Some(Instant::now());
                                app.profile_fetched = true;
                            }
                        }
                    }
                }

                // Search execution (blocking in tick, acceptable for now)
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
                                    .map(|(toon, gw_num, rating)| format!("{} • {} • Rating {}", toon, crate::api::gateway_label(gw_num), rating))
                                    .collect();
                                // Only show matches if current season total games across buckets >= 5
                                let eligible = guid.map(|g| {
                                    let season = info.matchmaked_current_season;
                                    info.matchmaked_stats
                                        .iter()
                                        .filter(|s| s.toon_guid == g && s.season_id == season)
                                        .fold(0u32, |acc, s| acc.saturating_add(s.wins + s.losses))
                                }).map(|n| n >= 5).unwrap_or(false);
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
                    let due = app.last_rating_poll.map_or(true, |t| t.elapsed() >= cfg.rating_poll_interval);
                    if due {
                        if let (Some(api), Some(name), Some(gw)) = (&app.api, &app.self_profile_name, app.self_gateway) {
                            match api.get_toon_info(name, gw) {
                                Ok(info) => {
                                    app.self_profile_rating = compute_self_rating(&info, name);
                                    app.last_rating_poll = Some(Instant::now());
                                    if let Ok(profile) = api.get_scr_profile(name, gw) {
                                        let (mr, lines, _results) = api.profile_stats_last100(&profile, name);
                                        app.self_main_race = mr;
                                        app.self_matchups = lines;
                                    }
                                }
                                Err(_) => {
                                    // Still update the timestamp to avoid hammering on repeated failures
                                    app.last_rating_poll = Some(Instant::now());
                                }
                            }
                        }
                    }
                }

                if let Ok(list) = r.recent_keys(app.debug_window_secs, 20) {
                    app.debug_recent = list
                        .into_iter()
                        .map(|(k, age)| format!("{age:>2}s • {}", k))
                        .collect();
                }
            }
            last_tick = Instant::now();
        }
    }

    Ok(())
}

fn main() -> io::Result<()> {
    let mut terminal = setup_terminal()?;
    let res = (|| {
        let mut app = App::new();
        run_app(&mut terminal, &mut app)
    })();
    let _ = restore_terminal(&mut terminal);
    res
}
