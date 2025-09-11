use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode};
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
    // Find the profile matching the name to get its toon_guid
    let toon_guid = info.profiles.iter().find(|p| p.toon == profile_name).map(|p| p.toon_guid)?;
    // Among matchmaked_stats, pick the entry with the same toon_guid and highest rating
    info.matchmaked_stats
        .iter()
        .filter(|s| s.toon_guid == toon_guid)
        .max_by_key(|s| s.rating)
        .map(|s| s.rating)
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
    let cfg = Config::default();
    let tick_rate = cfg.tick_rate;
    let mut last_tick = Instant::now();
    let mut last_refresh = Instant::now();
    // Initialize app values derived from config
    app.debug_window_secs = cfg.debug_window_secs;

    let mut reader = CacheReader::new(cfg.cache_dir.clone()).ok();

    while !app.should_quit {
        terminal.draw(|f| render(f, app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                    other => app.on_key(other),
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            if let Some(ref mut r) = reader {
                // Refresh cache less frequently
                if last_refresh.elapsed() >= cfg.refresh_interval {
                    let _ = r.refresh();
                    last_refresh = Instant::now();
                }

                // Only scan for port until found
                if app.port.is_none() {
                    if let Ok(port_opt) = r.parse_for_port(cfg.scan_window_secs) {
                        if port_opt.is_some() { app.port = port_opt; }
                    }
                }

                // Auto-detect self profile after connection via scr_tooninfo
                if app.port.is_some() && app.self_profile_name.is_none() {
                    if let Ok(self_opt) = r.latest_self_profile(cfg.scan_window_secs) {
                        if let Some((name, gw)) = self_opt {
                            app.self_profile_name = Some(name);
                            app.self_gateway = Some(gw);
                        }
                    }
                }

                // Initialize API client once we are ready and port changed
                if app.is_ready() {
                    if let Some(p) = app.port {
                        if app.api.is_none() || app.last_port_used != Some(p) {
                            let base_url = format!("http://127.0.0.1:{p}");
                            app.api = crate::api::ApiHandle::new(base_url).ok();
                            app.last_port_used = Some(p);
                        }
                    }
                }

                // If we see an mmgameloading entry that matches one of our profiles, update self (profile switched)
                if app.is_ready() {
                    if let Ok(self_mm_opt) = r.latest_mmgameloading_profile(cfg.scan_window_secs) {
                        if let Some((mm_name, mm_gw)) = self_mm_opt {
                            if app.own_profiles.contains(&mm_name) {
                                if app.self_profile_name.as_deref() != Some(&mm_name) || app.self_gateway != Some(mm_gw) {
                                    app.self_profile_name = Some(mm_name);
                                    app.self_gateway = Some(mm_gw);
                                    // Reset derived data so it refreshes for the new profile
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

                // Opponent (mmgameloading), after we're ready (port + self known)
                if app.is_ready() {
                    if let Ok(profile_opt) = r.latest_opponent_profile(app.self_profile_name.as_deref(), cfg.scan_window_secs) {
                        if let Some((name, gw)) = profile_opt {
                            // Ignore any of our own profiles
                            if !app.own_profiles.contains(&name) {
                                app.profile_name = Some(name);
                                app.gateway = Some(gw);
                                // If new opponent or gateway changed, fetch their toons summary
                                if let (Some(api), Some(opp_name), Some(opp_gw)) = (&app.api, &app.profile_name, app.gateway) {
                                    let identity = (opp_name.clone(), opp_gw);
                                    if app.last_opponent_identity.as_ref() != Some(&identity) {
                                        if let Ok(list) = api.opponent_toons_summary(opp_name, opp_gw) {
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

                // After self profile detected, fetch toon info once and compute rating
                if app.is_ready() && !app.profile_fetched {
                    if let (Some(api), Some(name), Some(gw)) = (&app.api, &app.self_profile_name, app.self_gateway) {
                        match api.get_toon_info(name, gw) {
                            Ok(info) => {
                                // Format profiles summary for debug
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
                                // Compute and store self rating
                                app.self_profile_rating = compute_self_rating(&info, name);
                                // Track all own toons to ignore for opponent detection
                                app.own_profiles = info.profiles.iter().map(|p| p.toon.clone()).collect();
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

                // Periodically poll for updated rating (every rating_poll_interval)
                if app.is_ready() {
                    let due = app.last_rating_poll.map_or(true, |t| t.elapsed() >= cfg.rating_poll_interval);
                    if due {
                        if let (Some(api), Some(name), Some(gw)) = (&app.api, &app.self_profile_name, app.self_gateway) {
                            match api.get_toon_info(name, gw) {
                                Ok(info) => {
                                    app.self_profile_rating = compute_self_rating(&info, name);
                                    app.last_rating_poll = Some(Instant::now());
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
