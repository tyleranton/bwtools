#![allow(clippy::collapsible_if, clippy::type_complexity)]
use std::process::Command;
use std::time::SystemTime;

use crate::app::{App, DodgeCandidate};
use crate::config::Config;
use crate::history::{FileHistorySource, HistoryService, OpponentRecord, derive_wl_and_race};
use crate::overlay::{OverlayError, OverlayService};
use crate::profile_history::{MatchOutcome, ProfileHistoryKey, ProfileHistoryService, StoredMatch};
use thiserror::Error;

pub struct ReplayService;

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("overlay error")]
    Overlay(#[from] OverlayError),
    #[error("history persistence error")]
    History(#[source] anyhow::Error),
}

impl ReplayService {
    pub fn tick(
        app: &mut App,
        cfg: &Config,
        history: Option<&HistoryService<FileHistorySource>>,
        profile_history: &mut ProfileHistoryService,
    ) -> Result<(), ReplayError> {
        handle_rating_retry(app, cfg)?;
        handle_screp_watch(app, cfg, history, profile_history)?;
        Ok(())
    }
}

fn handle_rating_retry(app: &mut App, cfg: &Config) -> Result<(), ReplayError> {
    if app.rating_retry_retries == 0 {
        return Ok(());
    }
    if let Some(next_at) = app.rating_retry_next_at {
        if next_at > std::time::Instant::now() {
            return Ok(());
        }
    }
    if let (Some(api), Some(name), Some(gw)) = (&app.api, &app.self_profile_name, app.self_gateway)
    {
        match api.get_toon_info(name, gw) {
            Ok(info) => {
                let new = api.compute_rating_for_name(&info, name);
                if new != app.rating_retry_baseline {
                    app.self_profile_rating = new;
                    app.rating_retry_retries = 0;
                    app.rating_retry_next_at = None;
                    app.rating_retry_baseline = None;
                    OverlayService::write_rating(cfg, app)?;
                } else {
                    app.rating_retry_retries = app.rating_retry_retries.saturating_sub(1);
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
    Ok(())
}

fn handle_screp_watch(
    app: &mut App,
    cfg: &Config,
    history: Option<&HistoryService<FileHistorySource>>,
    profile_history: &mut ProfileHistoryService,
) -> Result<(), ReplayError> {
    if !app.screp_available {
        return Ok(());
    }

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
            process_last_replay(app, cfg, history, profile_history)?;
            app.replay_changed_at = None;
        }
    }

    Ok(())
}

fn process_last_replay(
    app: &mut App,
    cfg: &Config,
    history: Option<&HistoryService<FileHistorySource>>,
    profile_history: &mut ProfileHistoryService,
) -> Result<(), ReplayError> {
    let output = Command::new(&cfg.screp_cmd)
        .arg("-overview")
        .arg(&cfg.last_replay_path)
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).to_string();
            let (winner, players) = parse_screp_overview(&text);
            let duration = parse_screp_duration_seconds(&text);
            app.last_dodge_candidate = None;
            if let Some(self_name) = &app.self_profile_name {
                let mut self_team: Option<u8> = None;
                for (team, _race, name) in players.iter() {
                    if name.eq_ignore_ascii_case(self_name) {
                        self_team = Some(*team);
                    }
                }
                if let Some(st) = self_team {
                    if let Some((opp_name, opp_race)) =
                        players.iter().find_map(|(team, race, name)| {
                            if *team != st {
                                Some((name.clone(), race.clone()))
                            } else {
                                None
                            }
                        })
                    {
                        if let Some(duration_secs) = duration
                            && duration_secs < 60
                            && let Some(winner_name) = winner.as_ref()
                        {
                            let outcome = if winner_name.eq_ignore_ascii_case(self_name) {
                                Some(MatchOutcome::OpponentDodged)
                            } else if winner_name.eq_ignore_ascii_case(&opp_name) {
                                Some(MatchOutcome::SelfDodged)
                            } else {
                                None
                            };
                            if let Some(outcome) = outcome {
                                app.last_dodge_candidate = Some(DodgeCandidate {
                                    opponent: opp_name.clone(),
                                    outcome,
                                    approx_timestamp: app
                                        .last_replay_mtime
                                        .and_then(system_time_secs),
                                });
                            }
                        }
                        update_opponent_history(
                            app,
                            cfg,
                            &opp_name,
                            opp_race,
                            history,
                            profile_history,
                        )?;
                    }
                }
            }
            app.last_replay_processed_mtime = app.last_replay_mtime;
        }
        Ok(_) => {
            tracing::error!("screp failed to parse LastReplay");
        }
        Err(err) => {
            tracing::error!(error = %err, "failed to run screp on last replay");
        }
    }
    Ok(())
}

fn update_opponent_history(
    app: &mut App,
    cfg: &Config,
    opp_name: &str,
    opp_race: Option<String>,
    history: Option<&HistoryService<FileHistorySource>>,
    profile_history: &mut ProfileHistoryService,
) -> Result<(), ReplayError> {
    let key = opp_name.to_ascii_lowercase();
    let gateway = app.gateway.unwrap_or(0);
    let last_replay_ts = app.last_replay_mtime.and_then(system_time_secs);

    {
        let entry = app
            .opponent_history
            .entry(key.clone())
            .or_insert_with(|| OpponentRecord {
                name: opp_name.to_string(),
                gateway,
                race: None,
                current_rating: None,
                previous_rating: None,
                wins: 0,
                losses: 0,
                last_match_ts: None,
            });
        entry.name = opp_name.to_string();
        entry.gateway = gateway;
        if let Some(ts) = last_replay_ts {
            entry.last_match_ts = Some(ts);
        }
        if entry.race.is_none() {
            entry.race = opp_race.clone();
        }
    }

    let mut history_update: Option<(u32, u32, Option<u64>, Option<String>)> = None;
    let mut refresh_rating_overlay = false;

    enum RatingRetryUpdate {
        Reset,
        Schedule { baseline: Option<u32> },
    }

    let mut new_profile_rating: Option<Option<u32>> = None;
    let mut rating_retry_update: Option<RatingRetryUpdate> = None;
    let mut main_race_update: Option<Option<String>> = None;
    let mut matchup_update: Option<Vec<String>> = None;
    let mut dodge_counts_update: Option<(u32, u32)> = None;
    let mut clear_dodge_candidate = false;
    let dodge_candidate = app.last_dodge_candidate.clone();

    if let (Some(api), Some(name), Some(gw)) = (
        app.api.as_ref(),
        app.self_profile_name.clone(),
        app.self_gateway,
    ) {
        match api.get_toon_info(&name, gw) {
            Ok(info) => {
                let old = app.self_profile_rating;
                let new = api.compute_rating_for_name(&info, &name);
                new_profile_rating = Some(new);
                refresh_rating_overlay = true;

                match api.get_scr_profile(&name, gw) {
                    Ok(profile) => {
                        history_update = Some(derive_wl_and_race(&profile, &name, opp_name));

                        let history_key = ProfileHistoryKey::new(&name, gw);

                        if let Some(candidate) = dodge_candidate.as_ref() {
                            if let Some(stored) = build_dodged_match(&profile, &name, candidate) {
                                match profile_history.upsert_match(&history_key, stored) {
                                    Ok(()) => clear_dodge_candidate = true,
                                    Err(err) => tracing::error!(
                                        error = %err,
                                        "failed to record dodged match"
                                    ),
                                }
                            }
                        }

                        let (mr, lines, _results, self_dodged, opp_dodged) = api
                            .profile_stats_last100(
                                &profile,
                                &name,
                                Some(profile_history),
                                Some(&history_key),
                            );
                        main_race_update = Some(mr);
                        matchup_update = Some(lines);
                        dodge_counts_update = Some((self_dodged, opp_dodged));
                    }
                    Err(err) => {
                        tracing::error!(error = %err, "failed to refresh self profile after replay");
                    }
                }

                if new == old {
                    rating_retry_update = Some(RatingRetryUpdate::Schedule { baseline: old });
                } else {
                    rating_retry_update = Some(RatingRetryUpdate::Reset);
                }
            }
            Err(_) => {
                rating_retry_update = Some(RatingRetryUpdate::Schedule {
                    baseline: app.self_profile_rating,
                });
            }
        }
    }

    if refresh_rating_overlay {
        OverlayService::write_rating(cfg, app)?;
    }

    if let Some(new_rating) = new_profile_rating {
        app.self_profile_rating = new_rating;
    }

    if let Some(update) = rating_retry_update {
        match update {
            RatingRetryUpdate::Reset => {
                app.rating_retry_retries = 0;
                app.rating_retry_next_at = None;
                app.rating_retry_baseline = None;
            }
            RatingRetryUpdate::Schedule { baseline } => {
                app.rating_retry_baseline = baseline;
                app.rating_retry_retries = cfg.rating_retry_max;
                app.rating_retry_next_at = Some(
                    std::time::Instant::now()
                        .checked_add(cfg.rating_retry_interval)
                        .unwrap_or_else(std::time::Instant::now),
                );
            }
        }
    }

    if let Some(mr) = main_race_update {
        app.self_main_race = mr;
    }

    if let Some(lines) = matchup_update {
        app.self_matchups = lines;
    }

    if let Some((self_dodged, opp_dodged)) = dodge_counts_update {
        app.self_dodged = self_dodged;
        app.opponent_dodged = opp_dodged;
    }

    if clear_dodge_candidate {
        app.last_dodge_candidate = None;
    }

    if let Some((wins, losses, ts, race)) = history_update {
        if let Some(entry) = app.opponent_history.get_mut(&key) {
            entry.wins = wins;
            entry.losses = losses;
            if let Some(latest) = ts {
                entry.last_match_ts = Some(latest);
            }
            if entry.race.is_none() {
                entry.race = race.map(|s| match s.to_lowercase().as_str() {
                    "protoss" => "Protoss".to_string(),
                    "terran" => "Terran".to_string(),
                    "zerg" => "Zerg".to_string(),
                    _ => s,
                });
            }
        }
    }

    if let Some(service) = history {
        service
            .save(&app.opponent_history)
            .map_err(ReplayError::History)?;
    }

    Ok(())
}

fn build_dodged_match(
    profile: &bw_web_api_rs::models::aurora_profile::ScrProfile,
    self_name: &str,
    candidate: &DodgeCandidate,
) -> Option<StoredMatch> {
    if !matches!(
        candidate.outcome,
        MatchOutcome::SelfDodged | MatchOutcome::OpponentDodged
    ) {
        return None;
    }

    for g in profile.game_results.iter() {
        let actual: Vec<&bw_web_api_rs::models::common::Player> = g
            .players
            .iter()
            .filter(|p| p.attributes.r#type == "player" && !p.toon.trim().is_empty())
            .collect();
        if actual.len() != 2 {
            continue;
        }
        let mi = if actual[0].toon.eq_ignore_ascii_case(self_name) {
            0
        } else if actual[1].toon.eq_ignore_ascii_case(self_name) {
            1
        } else {
            continue;
        };
        let oi = 1 - mi;
        if !actual[oi]
            .toon
            .eq_ignore_ascii_case(candidate.opponent.as_str())
        {
            continue;
        }

        let ts = g.create_time.parse::<u64>().ok()?;
        if let Some(approx) = candidate.approx_timestamp {
            if ts.abs_diff(approx) > 300 {
                continue;
            }
        }

        let expected = match candidate.outcome {
            MatchOutcome::OpponentDodged => "win",
            MatchOutcome::SelfDodged => "loss",
            _ => unreachable!(),
        };
        if !actual[mi].result.eq_ignore_ascii_case(expected) {
            continue;
        }

        let opponent_name = if actual[oi].toon.trim().is_empty() {
            "Unknown".to_string()
        } else {
            actual[oi].toon.clone()
        };

        return Some(StoredMatch {
            timestamp: ts,
            opponent: opponent_name,
            opponent_race: actual[oi].attributes.race.clone(),
            main_race: actual[mi].attributes.race.clone(),
            result: candidate.outcome,
        });
    }

    None
}

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
