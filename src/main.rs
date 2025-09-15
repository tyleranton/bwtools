use std::io;
use std::fs;
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

use crate::app::App;
use crate::cache::CacheReader;
use crate::config::Config;
use crate::history::{OpponentRecord, load_history, save_history};
use std::path::Path;
use std::process::Command;
use crate::tui::{restore_terminal, setup_terminal, install_panic_hook};
use crate::ui::render;

fn compute_self_rating(info: &bw_web_api_rs::models::aurora_profile::ScrToonInfo, profile_name: &str) -> Option<u32> {
    // Prefer matching by profile list (case-insensitive), else fall back to season stats by toon name
    let season = info.matchmaked_current_season;
    let toon_guid = info
        .profiles
        .iter()
        .find(|p| p.toon.eq_ignore_ascii_case(profile_name))
        .map(|p| p.toon_guid)
        .or_else(|| {
            info
                .matchmaked_stats
                .iter()
                .find(|s| s.season_id == season && s.toon.eq_ignore_ascii_case(profile_name))
                .map(|s| s.toon_guid)
        })?;

    let iter = info.matchmaked_stats.iter().filter(|s| s.toon_guid == toon_guid);
    if let Some(r) = iter.clone().filter(|s| s.season_id == season).max_by_key(|s| s.rating).map(|s| s.rating) {
        return Some(r);
    }
    iter.max_by_key(|s| s.rating).map(|s| s.rating)
}

fn write_rating_output(cfg: &Config, app: &mut App) {
    if !cfg.rating_output_enabled { return; }
    let text = match app.self_profile_rating { Some(r) => r.to_string(), None => "N/A".to_string() };
    if app.rating_output_last_text.as_deref() == Some(text.as_str()) { return; }
    if let Some(parent) = cfg.rating_output_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&cfg.rating_output_path, &text);
    app.rating_output_last_text = Some(text);
}

fn write_opponent_output(cfg: &Config, app: &mut App) {
    if !cfg.opponent_output_enabled { return; }
    let name = match &app.profile_name { Some(n) => n.clone(), None => { return; } };
    // Race from history if known
    let race = app
        .opponent_race
        .clone()
        .unwrap_or_else(|| "Unknown".to_string());
    // Rating from opponent_toons_data if present
    let rating_opt = app
        .opponent_toons_data
        .iter()
        .find(|(t, _, _)| t.eq_ignore_ascii_case(&name))
        .map(|(_, _, r)| *r);
    let rating_text = rating_opt.map(|r| r.to_string()).unwrap_or_else(|| "N/A".to_string());
    // Append W-L if we have any history
    let wl_text = app
        .opponent_history
        .get(&name.to_ascii_lowercase())
        .filter(|rec| rec.wins + rec.losses > 0)
        .map(|rec| format!(" • W-L {}-{}", rec.wins, rec.losses))
        .unwrap_or_default();
    let text = format!("{} • {} • {}{}", name, race, rating_text, wl_text);
    if app.opponent_output_last_text.as_deref() == Some(text.as_str()) { return; }
    if let Some(parent) = cfg.opponent_output_path.parent() { let _ = fs::create_dir_all(parent); }
    let _ = fs::write(&cfg.opponent_output_path, &text);
    app.opponent_output_last_text = Some(text);
}

fn parse_screp_overview(text: &str) -> (Option<String>, Vec<(u8, Option<String>, String)>) {
    // Returns (winner_team_label like "Team 1", players list of (team, race, name))
    let mut winner: Option<String> = None;
    let mut in_players = false;
    let mut players: Vec<(u8, Option<String>, String)> = Vec::new();
    for line in text.lines() {
        let l = line.trim_end();
        if l.to_ascii_lowercase().starts_with("winner") {
            if let Some((_, v)) = l.split_once(':') {
                winner = Some(v.trim().to_string());
            }
        }
        if l.starts_with("Team  R  APM") { in_players = true; continue; }
        if in_players {
            if l.trim().is_empty() { continue; }
            // Example: "  1   P    0    0   4  bwtest"
            let parts: Vec<&str> = l.split_whitespace().collect();
            if parts.len() >= 6 {
                let team = parts[0].parse::<u8>().unwrap_or(0);
                let r = parts[1];
                let race = match r.to_ascii_uppercase().as_str() { "P"=>Some("Protoss".to_string()), "T"=>Some("Terran".to_string()), "Z"=>Some("Zerg".to_string()), _=>None };
                let name = parts[5..].join(" ");
                players.push((team, race, name));
            }
        }
    }
    (winner, players)
}

fn system_time_secs(st: std::time::SystemTime) -> Option<u64> {
    st.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
}

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
                                    app.opponent_race = None;
                                    write_rating_output(&cfg, app);
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
                                        // Reset derived opponent fields when identity changes
                                        app.opponent_race = None;
                                        if let Ok(list) = api.opponent_toons_summary(opp_name, opp_gw) {
                                        app.opponent_toons_data = list.clone();
                                        app.opponent_toons = list
                                            .into_iter()
                                            .map(|(toon, gw_num, rating)| format!("{} • {} • {}", toon, crate::api::gateway_label(gw_num), rating))
                                            .collect();
                                            app.last_opponent_identity = Some(identity);
                                        }
                                        // Also fetch opponent profile to compute their main race like we do for self
                                        if let Ok(profile) = api.get_scr_profile(opp_name, opp_gw) {
                                            let (mr, _lines, _results) = api.profile_stats_last100(&profile, opp_name);
                                            app.opponent_race = mr;
                                        }
                                        // Update opponent history with latest rating snapshot
                                        if let Ok(info) = api.get_toon_info(opp_name, opp_gw) {
                                            // Find guid for opp_name (case-insensitive), or fallback via season stats
                                            let season = info.matchmaked_current_season;
                                            let guid = info
                                                .profiles
                                                .iter()
                                                .find(|p| p.toon.eq_ignore_ascii_case(opp_name))
                                                .map(|p| p.toon_guid)
                                                .or_else(|| info
                                                    .matchmaked_stats
                                                    .iter()
                                                    .find(|s| s.season_id == season && s.toon.eq_ignore_ascii_case(opp_name))
                                                    .map(|s| s.toon_guid)
                                                );
                                            let rating = guid.and_then(|g| api.compute_rating_for_guid(&info, g));
                                            let key = opp_name.to_ascii_lowercase();
                                            let is_new = !app.opponent_history.contains_key(&key);
                                            let entry = app.opponent_history.entry(key.clone()).or_insert_with(|| OpponentRecord {
                                                name: opp_name.clone(),
                                                gateway: opp_gw,
                                                race: None,
                                                current_rating: None,
                                                previous_rating: None,
                                                wins: 0,
                                                losses: 0,
                                                last_match_ts: None,
                                            });
                                            entry.name = opp_name.clone();
                                            entry.gateway = opp_gw;
                                            entry.previous_rating = entry.current_rating;
                                            entry.current_rating = rating;
                                            // If we have no prior W/L data for this opponent, scan our match history to backfill
                                            if is_new || (entry.wins + entry.losses == 0) {
                                                if let (Some(self_name), Some(self_gw)) = (&app.self_profile_name, app.self_gateway) {
                                                    if let Ok(profile) = api.get_scr_profile(self_name, self_gw) {
                                                        let mut wins = 0u32;
                                                        let mut losses = 0u32;
                                                        let mut last_ts: u64 = 0;
                                                        let mut last_race: Option<String> = None;
                                                        for g in profile.game_results.iter() {
                                                            let players: Vec<&bw_web_api_rs::models::common::Player> = g
                                                                .players
                                                                .iter()
                                                                .filter(|p| p.attributes.r#type == "player" && !p.toon.trim().is_empty())
                                                                .collect();
                                                            if players.len() != 2 { continue; }
                                                            let mi = if players[0].toon.eq_ignore_ascii_case(self_name) { 0 } else if players[1].toon.eq_ignore_ascii_case(self_name) { 1 } else { continue };
                                                            let oi = 1 - mi;
                                                            if !players[oi].toon.eq_ignore_ascii_case(opp_name) { continue; }
                                                            let ts = g.create_time.parse::<u64>().unwrap_or(0);
                                                            if players[mi].result.eq_ignore_ascii_case("win") { wins = wins.saturating_add(1); } else if players[mi].result.eq_ignore_ascii_case("loss") { losses = losses.saturating_add(1); }
                                                            if ts > last_ts {
                                                                last_ts = ts;
                                                                last_race = players[oi].attributes.race.clone();
                                                            }
                                                        }
                                                        entry.wins = wins;
                                                        entry.losses = losses;
                                                        entry.last_match_ts = if last_ts > 0 { Some(last_ts) } else { None };
                                                        if entry.race.is_none() {
                                                            entry.race = last_race.map(|s| match s.to_lowercase().as_str() { "protoss"=>"Protoss".to_string(), "terran"=>"Terran".to_string(), "zerg"=>"Zerg".to_string(), _=>s });
                                                        }
                                                    }
                                                }
                                            }
                                            save_history(&cfg.opponent_history_path, &app.opponent_history);
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
                                write_rating_output(&cfg, app);
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
                                    .map(|(toon, gw_num, rating)| format!("{} • {} • {}", toon, crate::api::gateway_label(gw_num), rating))
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
                    // If screp is not available, continue periodic rating poll as fallback
                    if !app.screp_available {
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
                                        write_rating_output(&cfg, app);
                                    }
                                    Err(_) => {
                                        // Still update the timestamp to avoid hammering on repeated failures
                                        app.last_rating_poll = Some(Instant::now());
                                    }
                                }
                            }
                        }
                    }
                    // If screp is available, watch LastReplay.rep and process on change
                    if app.screp_available {
                        if let Ok(meta) = std::fs::metadata(&cfg.last_replay_path) {
                            if let Ok(mtime) = meta.modified() {
                                let changed = app.last_replay_processed_mtime.map_or(true, |p| mtime > p);
                                if changed {
                                    app.last_replay_mtime = Some(mtime);
                                    if app.replay_changed_at.is_none() {
                                        app.replay_changed_at = Some(Instant::now());
                                    }
                                }
                            }
                        }
                        if let Some(start) = app.replay_changed_at {
                            if start.elapsed() >= cfg.replay_settle {
                                // Run screp -overview
                                let output = Command::new(&cfg.screp_cmd)
                                    .arg("-overview")
                                    .arg(&cfg.last_replay_path)
                                    .output();
                                match output {
                                    Ok(out) if out.status.success() => {
                                        let text = String::from_utf8_lossy(&out.stdout).to_string();
                                        let (winner, players) = parse_screp_overview(&text);
                                        // Find self and opponent in players
                                        if let (Some(self_name), Some(wlab)) = (&app.self_profile_name, winner) {
                                            let mut self_team: Option<u8> = None;
                                            let mut opp: Option<(u8, String)> = None;
                                            // restrict to first two players (1v1 typical), else pick the one not self
                                            for (team, _race, name) in players.iter() {
                                                if name.eq_ignore_ascii_case(self_name) {
                                                    self_team = Some(*team);
                                                }
                                            }
                                            if let Some(st) = self_team {
                                                // opponent is anyone in players with team != st; choose first
                                                for (team, _race, name) in players.iter() {
                                                    if *team != st { opp = Some((*team, name.clone())); break; }
                                                }
                                                if let Some((_ot, opp_name)) = opp {
                                                    // Determine win/loss
                                                    let win = wlab.to_ascii_lowercase().contains(&format!("team {}", st).to_ascii_lowercase());
                                                    let key = opp_name.to_ascii_lowercase();
                                                    let entry = app.opponent_history.entry(key.clone()).or_insert_with(|| OpponentRecord {
                                                        name: opp_name.clone(), gateway: app.gateway.unwrap_or(0), race: None, current_rating: None, previous_rating: None, wins: 0, losses: 0, last_match_ts: None,
                                                    });
                                                    if let Some(mt) = app.last_replay_mtime { entry.last_match_ts = system_time_secs(mt); }
                                                    if win { entry.wins = entry.wins.saturating_add(1); } else { entry.losses = entry.losses.saturating_add(1); }
                                                    // Fill race if unknown from screp
                                                    if entry.race.is_none() {
                                                        // find opp row again to get race
                                                        if let Some((_, race, _)) = players.iter().find(|(t, _, n)| *t != st && n.eq_ignore_ascii_case(&opp_name)) {
                                                            entry.race = race.clone();
                                                        }
                                                    }
                                                    save_history(&cfg.opponent_history_path, &app.opponent_history);
                                                    
                                                    // Refresh rating once per replay
                                                    if let (Some(api), Some(name), Some(gw)) = (&app.api, &app.self_profile_name, app.self_gateway) {
                                                        if let Ok(info) = api.get_toon_info(name, gw) {
                                                            app.self_profile_rating = compute_self_rating(&info, name);
                                                            // Update overlay file with new rating
                                                            write_rating_output(&cfg, app);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        // Mark processed
                                        app.last_replay_processed_mtime = app.last_replay_mtime;
                                    }
                                    _ => {
                                        // If screp fails repeatedly, we could disable screp_available; keep enabled but show error in debug
                                        app.last_profile_text = Some("screp failed to parse LastReplay".to_string());
                                    }
                                }
                                app.replay_changed_at = None;
                            }
                        }
                    }
                }
                // Update opponent overlay text once per tick after potential updates
                write_opponent_output(&cfg, app);

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
