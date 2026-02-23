use std::time::SystemTime;

use crate::app::App;
use crate::config::Config;
use crate::history::{FileHistorySource, HistoryService};
use crate::overlay::OverlayError;
use crate::profile_history::{MatchOutcome, ProfileHistoryService};
use thiserror::Error;

pub struct ReplayService;

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("overlay error")]
    Overlay(#[from] OverlayError),
    #[error("history persistence error")]
    History(#[source] anyhow::Error),
}

type ScrepPlayers = Vec<(u8, Option<String>, String)>;
type ScrepParsed = (Option<String>, ScrepPlayers);
type ScrepOverview = (Option<String>, ScrepPlayers, Option<u32>);

impl ReplayService {
    pub fn tick(
        app: &mut App,
        cfg: &Config,
        history: Option<&HistoryService<FileHistorySource>>,
        profile_history: &mut ProfileHistoryService,
    ) -> Result<(), ReplayError> {
        rating_retry::run(app, cfg)?;
        screp_watch::run(app, cfg, history, profile_history)?;
        Ok(())
    }
}
mod rating_retry {
    use super::ReplayError;
    use crate::app::App;
    use crate::config::Config;
    use crate::overlay::OverlayService;

    fn schedule_retry(app: &mut App, cfg: &Config) {
        let retry = &mut app.self_profile.rating_retry;
        retry.retries = retry.retries.saturating_sub(1);
        retry.next_at = Some(
            std::time::Instant::now()
                .checked_add(cfg.rating_retry_interval)
                .unwrap_or_else(std::time::Instant::now),
        );
    }

    fn reset_retry(app: &mut App) {
        let retry = &mut app.self_profile.rating_retry;
        retry.retries = 0;
        retry.next_at = None;
        retry.baseline = None;
    }

    pub(super) fn run(app: &mut App, cfg: &Config) -> Result<(), ReplayError> {
        let retry = &mut app.self_profile.rating_retry;
        if retry.retries == 0 {
            return Ok(());
        }
        if let Some(next_at) = retry.next_at
            && next_at > std::time::Instant::now()
        {
            return Ok(());
        }

        if let (Some(api), Some(name), Some(gw)) = (
            app.detection.api.as_ref(),
            app.self_profile.name.clone(),
            app.self_profile.gateway,
        ) {
            match api.get_toon_info(&name, gw) {
                Ok(info) => {
                    let new = api.compute_rating_for_name(&info, &name);
                    if new != retry.baseline {
                        app.self_profile.rating = new;
                        reset_retry(app);
                        OverlayService::write_rating(cfg, app)?;
                    } else {
                        schedule_retry(app, cfg);
                    }
                }
                Err(_) => {
                    schedule_retry(app, cfg);
                }
            }
        } else {
            reset_retry(app);
        }
        Ok(())
    }
}

mod screp_watch {
    use super::{
        classify_short_game_outcome, parse_screp_duration_seconds, parse_screp_overview,
        system_time_secs, ReplayError, ScrepOverview,
    };
    use crate::app::{App, DodgeCandidate};
    use crate::config::Config;
    use crate::history::{derive_wl_and_race, FileHistorySource, HistoryService, OpponentRecord};
    use crate::overlay::OverlayService;
    use crate::profile_history::{
        MatchOutcome, ProfileHistoryKey, ProfileHistoryService, StoredMatch,
    };
    use crate::replay_io::run_screp_overview;

    pub(super) fn run(
        app: &mut App,
        cfg: &Config,
        history: Option<&HistoryService<FileHistorySource>>,
        profile_history: &mut ProfileHistoryService,
    ) -> Result<(), ReplayError> {
        if !app.detection.screp_available {
            return Ok(());
        }

        if let Ok(meta) = std::fs::metadata(&cfg.last_replay_path)
            && let Ok(mtime) = meta.modified()
        {
            let changed = app
                .replay_watch
                .last_processed_mtime
                .is_none_or(|p| mtime > p);
            if changed {
                app.replay_watch.last_mtime = Some(mtime);
                if app.replay_watch.changed_at.is_none() {
                    app.replay_watch.changed_at = Some(std::time::Instant::now());
                }
            }
        }

        if let Some(start) = app.replay_watch.changed_at
            && start.elapsed() >= cfg.replay_settle
        {
            process_last_replay(app, cfg, history, profile_history)?;
            app.replay_watch.changed_at = None;
        }

        Ok(())
    }

    fn process_last_replay(
        app: &mut App,
        cfg: &Config,
        history: Option<&HistoryService<FileHistorySource>>,
        profile_history: &mut ProfileHistoryService,
    ) -> Result<(), ReplayError> {
        app.overlays.opponent_waiting = true;
        OverlayService::write_opponent(cfg, app)?;

        let Some((winner, players, duration)) = load_latest_overview(cfg)? else {
            return Ok(());
        };
        app.replay_watch.last_dodge_candidate = None;
        if let Some(self_name) = &app.self_profile.name
            && let Some(resolved) = resolve_opponent(&players, self_name)
        {
            if let Some(duration_secs) = duration
                && duration_secs < 60
            {
                let outcome_guess = winner.as_deref().and_then(|winner_label| {
                    classify_short_game_outcome(
                        winner_label,
                        self_name,
                        resolved.self_team,
                        &resolved.opponent_name,
                        resolved.opponent_team,
                    )
                });
                app.replay_watch.last_dodge_candidate = Some(DodgeCandidate {
                    opponent: resolved.opponent_name.clone(),
                    outcome: outcome_guess,
                    approx_timestamp: app.replay_watch.last_mtime.and_then(system_time_secs),
                });
            }
            update_opponent_history(
                app,
                cfg,
                &resolved.opponent_name,
                resolved.opponent_race,
                history,
                profile_history,
            )?;
        }
        app.replay_watch.last_processed_mtime = app.replay_watch.last_mtime;
        Ok(())
    }

    fn load_latest_overview(cfg: &Config) -> Result<Option<ScrepOverview>, ReplayError> {
        let text = match run_screp_overview(cfg, &cfg.last_replay_path) {
            Ok(text) => text,
            Err(err) => {
                tracing::error!(error = %err, "failed to run screp on last replay");
                return Ok(None);
            }
        };

        let parsed = parse_screp_overview(&text);
        let duration = parse_screp_duration_seconds(&text);
        Ok(Some((parsed.0, parsed.1, duration)))
    }

    struct ResolvedOpponent {
        opponent_name: String,
        opponent_race: Option<String>,
        opponent_team: u8,
        self_team: u8,
    }

    fn resolve_opponent(
        players: &[(u8, Option<String>, String)],
        self_name: &str,
    ) -> Option<ResolvedOpponent> {
        let self_team = players
            .iter()
            .find(|(_, _, name)| name.eq_ignore_ascii_case(self_name))?
            .0;

        let (team, race, name) = players
            .iter()
            .find(|(team, _, _)| *team != self_team && *team != 0)?;

        Some(ResolvedOpponent {
            opponent_name: name.clone(),
            opponent_race: race.clone(),
            opponent_team: *team,
            self_team,
        })
    }

    fn update_opponent_history(
        app: &mut App,
        cfg: &Config,
        opp_name: &str,
        opp_race: Option<String>,
        history: Option<&HistoryService<FileHistorySource>>,
        profile_history: &mut ProfileHistoryService,
    ) -> Result<(), ReplayError> {
        let key = crate::race::lower_key(opp_name);
        let gateway = app.opponent.gateway.unwrap_or(0);
        let last_replay_ts = app.replay_watch.last_mtime.and_then(system_time_secs);

        {
            let entry = app
                .opponent
                .history
                .entry(key.clone())
                .or_insert_with(|| OpponentRecord::new(opp_name, gateway));
            entry.name = opp_name.to_string();
            entry.gateway = gateway;
            if let Some(ts) = last_replay_ts {
                entry.last_match_ts = Some(ts);
            }
            entry.set_race_if_unknown(opp_race.as_deref());
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
        let dodge_candidate = app.replay_watch.last_dodge_candidate.clone();

        if let (Some(api), Some(name), Some(gw)) = (
            app.detection.api.as_ref(),
            app.self_profile.name.clone(),
            app.self_profile.gateway,
        ) {
            match api.get_toon_info(&name, gw) {
                Ok(info) => {
                    let old = app.self_profile.rating;
                    let new = api.compute_rating_for_name(&info, &name);
                    new_profile_rating = Some(new);
                    refresh_rating_overlay = true;

                    match api.get_scr_profile(&name, gw) {
                        Ok(profile) => {
                            history_update = Some(derive_wl_and_race(&profile, &name, opp_name));

                            let history_key = ProfileHistoryKey::new(&name, gw);

                            if let Some(candidate) = dodge_candidate.as_ref()
                                && let Some((stored, _resolved_outcome)) =
                                    build_dodged_match(&profile, &name, candidate)
                            {
                                match profile_history.upsert_match(&history_key, stored) {
                                    Ok(()) => {
                                        clear_dodge_candidate = true;
                                    }
                                    Err(err) => tracing::error!(
                                        error = %err,
                                        "failed to record dodged match"
                                    ),
                                }
                            }

                            let (mr, lines, _results, self_dodged, opp_dodged) = api
                                .profile_stats_last100(
                                    &profile,
                                    &name,
                                    Some(profile_history),
                                    Some(&history_key),
                                    Some(&app.opponent.history),
                                );
                            main_race_update = Some(mr);
                            matchup_update = Some(lines);
                            dodge_counts_update = Some((self_dodged, opp_dodged));
                        }
                        Err(err) => {
                            tracing::error!(
                                error = %err,
                                "failed to refresh self profile after replay"
                            );
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
                        baseline: app.self_profile.rating,
                    });
                }
            }
        }

        if refresh_rating_overlay {
            OverlayService::write_rating(cfg, app)?;
        }

        if let Some(new_rating) = new_profile_rating {
            app.self_profile.rating = new_rating;
        }

        if let Some(update) = rating_retry_update {
            match update {
                RatingRetryUpdate::Reset => {
                    app.self_profile.rating_retry.retries = 0;
                    app.self_profile.rating_retry.next_at = None;
                    app.self_profile.rating_retry.baseline = None;
                }
                RatingRetryUpdate::Schedule { baseline } => {
                    app.self_profile.rating_retry.baseline = baseline;
                    app.self_profile.rating_retry.retries = cfg.rating_retry_max;
                    app.self_profile.rating_retry.next_at = Some(
                        std::time::Instant::now()
                            .checked_add(cfg.rating_retry_interval)
                            .unwrap_or_else(std::time::Instant::now),
                    );
                }
            }
        }

        if let Some(mr) = main_race_update {
            app.self_profile.main_race = mr;
        }

        if let Some(lines) = matchup_update {
            app.self_profile.matchups = lines;
        }

        if let Some((self_dodged, opp_dodged)) = dodge_counts_update {
            app.self_profile.self_dodged = self_dodged;
            app.self_profile.opponent_dodged = opp_dodged;
        }

        if clear_dodge_candidate {
            app.replay_watch.last_dodge_candidate = None;
        }

        if let Some((wins, losses, ts, race)) = history_update
            && let Some(entry) = app.opponent.history.get_mut(&key)
        {
            entry.wins = wins;
            entry.losses = losses;
            if let Some(latest) = ts {
                entry.last_match_ts = Some(latest);
            }
            entry.set_race_if_unknown(race.as_deref());
        }

        if let Some(service) = history {
            service
                .save(&app.opponent.history)
                .map_err(ReplayError::History)?;
        }

        Ok(())
    }

    fn build_dodged_match(
        profile: &bw_web_api_rs::models::aurora_profile::ScrProfile,
        self_name: &str,
        candidate: &DodgeCandidate,
    ) -> Option<(StoredMatch, MatchOutcome)> {
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
            if let Some(approx) = candidate.approx_timestamp
                && ts.abs_diff(approx) > 300
            {
                continue;
            }

            let resolved_outcome = match candidate.outcome {
                Some(outcome @ MatchOutcome::OpponentDodged) => {
                    if !actual[mi].result.eq_ignore_ascii_case("win") {
                        continue;
                    }
                    outcome
                }
                Some(outcome @ MatchOutcome::SelfDodged) => {
                    if !actual[mi].result.eq_ignore_ascii_case("loss") {
                        continue;
                    }
                    outcome
                }
                _ => {
                    if actual[mi].result.eq_ignore_ascii_case("win") {
                        MatchOutcome::OpponentDodged
                    } else if actual[mi].result.eq_ignore_ascii_case("loss") {
                        MatchOutcome::SelfDodged
                    } else {
                        continue;
                    }
                }
            };

            let opponent_name = if actual[oi].toon.trim().is_empty() {
                "Unknown".to_string()
            } else {
                actual[oi].toon.clone()
            };

            return Some((
                StoredMatch {
                    timestamp: ts,
                    opponent: opponent_name,
                    opponent_race: actual[oi].attributes.race.clone(),
                    main_race: actual[mi].attributes.race.clone(),
                    result: resolved_outcome,
                },
                resolved_outcome,
            ));
        }

        None
    }
}

pub fn parse_screp_overview(text: &str) -> ScrepParsed {
    let mut winner: Option<String> = None;
    let mut in_players = false;
    let mut players: Vec<(u8, Option<String>, String)> = Vec::new();
    for line in text.lines() {
        let l = line.trim_end();
        if l.to_ascii_lowercase().starts_with("winner")
            && let Some((_, v)) = l.split_once(':')
        {
            winner = Some(v.trim().to_string());
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
                let Ok(team) = parts[0].parse::<u8>() else {
                    continue;
                };
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

fn classify_short_game_outcome(
    winner_label: &str,
    self_name: &str,
    self_team: u8,
    opponent_name: &str,
    opponent_team: u8,
) -> Option<MatchOutcome> {
    let trimmed = winner_label.trim();
    if trimmed.is_empty() {
        return None;
    }

    let winner_lower = trimmed.to_ascii_lowercase();
    let self_lower = self_name.trim().to_ascii_lowercase();
    let opp_lower = opponent_name.trim().to_ascii_lowercase();

    if !self_lower.is_empty() && winner_lower.contains(&self_lower) {
        return Some(MatchOutcome::OpponentDodged);
    }
    if !opp_lower.is_empty() && winner_lower.contains(&opp_lower) {
        return Some(MatchOutcome::SelfDodged);
    }

    if let Some(team) = winner_team_number(&winner_lower) {
        if team == self_team {
            return Some(MatchOutcome::OpponentDodged);
        }
        if team == opponent_team {
            return Some(MatchOutcome::SelfDodged);
        }
    }

    None
}

fn winner_team_number(label: &str) -> Option<u8> {
    let mut lower = label.to_ascii_lowercase();
    if let Some(idx) = lower.find("team") {
        lower.drain(..idx + 4);
        let digits: String = lower
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !digits.is_empty() {
            return digits.parse().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_screp_overview_extracts_winner_and_players() {
        let input = "Winner: Alice\nTeam  R  APM\n1 P 100 0 0 Alice\n2 T 120 0 0 Bob\n";

        let (winner, players) = parse_screp_overview(input);

        assert_eq!(winner.as_deref(), Some("Alice"));
        assert_eq!(players.len(), 2);
        assert_eq!(
            players[0],
            (1, Some("Protoss".to_string()), "Alice".to_string())
        );
        assert_eq!(
            players[1],
            (2, Some("Terran".to_string()), "Bob".to_string())
        );
    }

    #[test]
    fn parse_screp_overview_skips_invalid_team_rows() {
        let input = "Winner: Alice\nTeam  R  APM\nX P 100 0 0 Alice\n2 Z 120 0 0 Bob\n";

        let (_winner, players) = parse_screp_overview(input);

        assert_eq!(players.len(), 1);
        assert_eq!(players[0], (2, Some("Zerg".to_string()), "Bob".to_string()));
    }

    #[test]
    fn parse_screp_duration_supports_mm_ss_and_hh_mm_ss() {
        assert_eq!(parse_screp_duration_seconds("Length: 1:23"), Some(83));
        assert_eq!(parse_screp_duration_seconds("Length: 0:05:12"), Some(312));
    }

    #[test]
    fn parse_screp_duration_ignores_invalid_lines() {
        assert_eq!(parse_screp_duration_seconds("Length: invalid"), None);
        assert_eq!(parse_screp_duration_seconds("Something else"), None);
    }

    #[test]
    fn classify_short_game_outcome_prefers_name_matches() {
        let self_win = classify_short_game_outcome("Alice wins", "Alice", 1, "Bob", 2);
        let opp_win = classify_short_game_outcome("Bob wins", "Alice", 1, "Bob", 2);

        assert_eq!(self_win, Some(MatchOutcome::OpponentDodged));
        assert_eq!(opp_win, Some(MatchOutcome::SelfDodged));
    }

    #[test]
    fn classify_short_game_outcome_falls_back_to_team_number() {
        let self_team = classify_short_game_outcome("Team 1", "Alice", 1, "Bob", 2);
        let opp_team = classify_short_game_outcome("Team 2", "Alice", 1, "Bob", 2);

        assert_eq!(self_team, Some(MatchOutcome::OpponentDodged));
        assert_eq!(opp_team, Some(MatchOutcome::SelfDodged));
    }

    #[test]
    fn winner_team_number_extracts_digits_after_team_keyword() {
        assert_eq!(winner_team_number("Team 12 wins"), Some(12));
        assert_eq!(winner_team_number("no team info"), None);
    }
}
