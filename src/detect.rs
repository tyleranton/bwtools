use crate::app::App;
use crate::cache::CacheReader;
use crate::config::Config;
use crate::history::{OpponentRecord, save_history};
use crate::overlay;

pub fn tick_detection(app: &mut App, cfg: &Config, r: &mut CacheReader) {
    // Port detection
    if app.port.is_none() {
        if let Ok(port_opt) = r.parse_for_port(cfg.scan_window_secs) {
            if port_opt.is_some() { app.port = port_opt; }
        }
    }

    // Self profile detection
    if app.port.is_some() && app.self_profile_name.is_none() {
        if let Ok(self_opt) = r.latest_self_profile(cfg.scan_window_secs) {
            if let Some((name, gw)) = self_opt {
                app.self_profile_name = Some(name);
                app.self_gateway = Some(gw);
            }
        }
    }

    // API init
    if let Some(p) = app.port {
        if app.api.is_none() || app.last_port_used != Some(p) {
            let base_url = format!("http://127.0.0.1:{p}");
            app.api = crate::api::ApiHandle::new(base_url).ok();
            app.last_port_used = Some(p);
        }
    }

    // Self profile switch detection (mmgameloading)
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
                        app.reset_opponent_state();
                        overlay::write_rating(cfg, app);
                    }
                }
            }
        }
    }

    // Opponent detection
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
}

