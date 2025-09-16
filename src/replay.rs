#![allow(clippy::collapsible_if, clippy::type_complexity)]
use std::time::SystemTime;

pub fn parse_screp_overview(text: &str) -> (Option<String>, Vec<(u8, Option<String>, String)>) {
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
        if l.starts_with("Team  R  APM") {
            in_players = true;
            continue;
        }
        if in_players {
            if l.trim().is_empty() {
                continue;
            }
            let parts: Vec<&str> = l.split_whitespace().collect();
            if parts.len() >= 6 {
                let team = parts[0].parse::<u8>().unwrap_or(0);
                let r = parts[1];
                let race = match r.to_ascii_uppercase().as_str() {
                    "P" => Some("Protoss".to_string()),
                    "T" => Some("Terran".to_string()),
                    "Z" => Some("Zerg".to_string()),
                    _ => None,
                };
                let name = parts[5..].join(" ");
                players.push((team, race, name));
            }
        }
    }
    (winner, players)
}

pub fn parse_screp_duration_seconds(text: &str) -> Option<u32> {
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if !lower.contains("length") {
            continue;
        }
        let value = line.split_once(':').map(|(_, rest)| rest).unwrap_or(line);
        let trimmed = value.trim();
        let parts: Vec<&str> = trimmed.split(':').collect();
        if parts.len() < 2 {
            continue;
        }
        let hours_offset = if parts.len() == 3 { 1 } else { 0 };
        let minutes_idx = if parts.len() == 3 { 1 } else { 0 };
        let seconds_idx = if parts.len() == 3 { 2 } else { 1 };
        let hours = if hours_offset == 1 {
            parts[0].trim().parse::<u32>().ok().unwrap_or(0)
        } else {
            0
        };
        if let (Ok(minutes), Ok(seconds)) = (
            parts[minutes_idx].trim().parse::<u32>(),
            parts[seconds_idx]
                .split_whitespace()
                .next()
                .unwrap_or("0")
                .parse::<u32>(),
        ) {
            return Some(hours * 3600 + minutes * 60 + seconds);
        }
    }
    None
}

pub fn system_time_secs(st: SystemTime) -> Option<u64> {
    st.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}
use crate::app::App;
use crate::config::Config;
use crate::history::{OpponentRecord, save_history};
use crate::overlay;
use std::process::Command;

pub fn tick_replay_and_rating_retry(app: &mut App, cfg: &Config) {
    // Pending rating retry after replay
    if app.rating_retry_retries > 0 {
        if let Some(next_at) = app.rating_retry_next_at {
            if next_at <= std::time::Instant::now() {
                if let (Some(api), Some(name), Some(gw)) =
                    (&app.api, &app.self_profile_name, app.self_gateway)
                {
                    match api.get_toon_info(name, gw) {
                        Ok(info) => {
                            let new = api.compute_rating_for_name(&info, name);
                            if new != app.rating_retry_baseline {
                                app.self_profile_rating = new;
                                app.rating_retry_retries = 0;
                                app.rating_retry_next_at = None;
                                app.rating_retry_baseline = None;
                                overlay::write_rating(cfg, app);
                            } else {
                                app.rating_retry_retries =
                                    app.rating_retry_retries.saturating_sub(1);
                                app.rating_retry_next_at = Some(
                                    std::time::Instant::now()
                                        .checked_add(cfg.rating_retry_interval)
                                        .unwrap_or_else(std::time::Instant::now),
                                );
                            }
                        }
                        Err(_) => {
                            app.rating_retry_retries = app.rating_retry_retries.saturating_sub(1);
                            app.rating_retry_next_at = Some(
                                std::time::Instant::now()
                                    .checked_add(cfg.rating_retry_interval)
                                    .unwrap_or_else(std::time::Instant::now),
                            );
                        }
                    }
                } else {
                    app.rating_retry_retries = 0;
                    app.rating_retry_next_at = None;
                    app.rating_retry_baseline = None;
                }
            }
        }
    }

    // Replay watch and screp processing
    if app.screp_available {
        if let Ok(meta) = std::fs::metadata(&cfg.last_replay_path) {
            if let Ok(mtime) = meta.modified() {
                let changed = app.last_replay_processed_mtime.is_none_or(|p| mtime > p);
                if changed {
                    app.last_replay_mtime = Some(mtime);
                    if app.replay_changed_at.is_none() {
                        app.replay_changed_at = Some(std::time::Instant::now());
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
                        let (winner, players) = crate::replay::parse_screp_overview(&text);
                        // Find self and opponent in players
                        if let (Some(self_name), Some(wlab)) = (&app.self_profile_name, winner) {
                            let mut self_team: Option<u8> = None;
                            let mut opp: Option<(u8, String)> = None;
                            for (team, _race, name) in players.iter() {
                                if name.eq_ignore_ascii_case(self_name) {
                                    self_team = Some(*team);
                                }
                            }
                            if let Some(st) = self_team {
                                // opponent is anyone in players with team != st; choose first
                                for (team, _race, name) in players.iter() {
                                    if *team != st {
                                        opp = Some((*team, name.clone()));
                                        break;
                                    }
                                }
                                if let Some((_ot, opp_name)) = opp {
                                    // Determine win/loss
                                    let win = wlab
                                        .to_ascii_lowercase()
                                        .contains(&format!("team {}", st).to_ascii_lowercase());
                                    let key = opp_name.to_ascii_lowercase();
                                    let entry = app
                                        .opponent_history
                                        .entry(key.clone())
                                        .or_insert_with(|| OpponentRecord {
                                            name: opp_name.clone(),
                                            gateway: app.gateway.unwrap_or(0),
                                            race: None,
                                            current_rating: None,
                                            previous_rating: None,
                                            wins: 0,
                                            losses: 0,
                                            last_match_ts: None,
                                        });
                                    if let Some(mt) = app.last_replay_mtime {
                                        entry.last_match_ts = crate::replay::system_time_secs(mt);
                                    }
                                    if win {
                                        entry.wins = entry.wins.saturating_add(1);
                                    } else {
                                        entry.losses = entry.losses.saturating_add(1);
                                    }
                                    // Fill race if unknown from screp
                                    if entry.race.is_none() {
                                        if let Some((_, race, _)) =
                                            players.iter().find(|(t, _, n)| {
                                                *t != st && n.eq_ignore_ascii_case(&opp_name)
                                            })
                                        {
                                            entry.race = race.clone();
                                        }
                                    }
                                    save_history(&cfg.opponent_history_path, &app.opponent_history);

                                    // Refresh rating once per replay; if unchanged, schedule short retries
                                    if let (Some(api), Some(name), Some(gw)) =
                                        (&app.api, &app.self_profile_name, app.self_gateway)
                                    {
                                        if let Ok(info) = api.get_toon_info(name, gw) {
                                            let old = app.self_profile_rating;
                                            let new = api.compute_rating_for_name(&info, name);
                                            app.self_profile_rating = new;
                                            overlay::write_rating(cfg, app);
                                            if new == old {
                                                app.rating_retry_baseline = old;
                                                app.rating_retry_retries = cfg.rating_retry_max;
                                                app.rating_retry_next_at = Some(
                                                    std::time::Instant::now()
                                                        .checked_add(cfg.rating_retry_interval)
                                                        .unwrap_or_else(std::time::Instant::now),
                                                );
                                            } else {
                                                app.rating_retry_retries = 0;
                                                app.rating_retry_next_at = None;
                                                app.rating_retry_baseline = None;
                                            }
                                        } else {
                                            // If immediate fetch fails, also schedule retries
                                            app.rating_retry_baseline = app.self_profile_rating;
                                            app.rating_retry_retries = cfg.rating_retry_max;
                                            app.rating_retry_next_at = Some(
                                                std::time::Instant::now()
                                                    .checked_add(cfg.rating_retry_interval)
                                                    .unwrap_or_else(std::time::Instant::now),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        // Mark processed
                        app.last_replay_processed_mtime = app.last_replay_mtime;
                    }
                    _ => {
                        app.last_profile_text =
                            Some("screp failed to parse LastReplay".to_string());
                    }
                }
                app.replay_changed_at = None;
            }
        }
    }
}
