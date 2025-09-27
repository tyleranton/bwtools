use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::app::App;
use crate::config::Config;
use crate::overlay::{OverlayError, OverlayService};
use crate::profile_history::{MatchOutcome, ProfileHistoryKey, ProfileHistoryService, StoredMatch};
use anyhow::{Context, Result as AnyhowResult};
use reqwest::blocking::Client;
use thiserror::Error;
use tracing::info;

use crate::replay::parse_screp_duration_seconds;
use crate::replay_io::{download_replay, run_screp_overview, sanitize_identifier};

pub struct ProfileService;

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("api error")]
    Api(#[source] anyhow::Error),
    #[error("overlay error")]
    Overlay(#[from] OverlayError),
}

impl ProfileService {
    pub fn fetch_self_profile(
        app: &mut App,
        cfg: &Config,
        mut profile_history: Option<&mut ProfileHistoryService>,
    ) -> Result<(), ProfileError> {
        let (api, name, gw) = match (
            &app.detection.api,
            &app.self_profile.name,
            app.self_profile.gateway,
        ) {
            (Some(api), Some(name), Some(gw)) => (api, name.clone(), gw),
            _ => return Ok(()),
        };

        let info = api.get_toon_info(&name, gw).map_err(ProfileError::Api)?;
        let profiles = info.profiles.as_deref().unwrap_or(&[]);
        let mut out = String::new();
        out.push_str(&format!("profiles ({}):\n", profiles.len()));
        for (i, p) in profiles.iter().enumerate() {
            out.push_str(&format!(
                "{:>3}. title={}, toon={}, toon_guid={}, private={}\n",
                i + 1,
                p.title,
                p.toon,
                p.toon_guid,
                p.private
            ));
        }
        app.status.last_profile_text = Some(out);
        app.self_profile.rating = api.compute_rating_for_name(&info, &name);

        app.self_profile.own_profiles = profiles.iter().map(|p| p.toon.clone()).collect();
        if let Ok(profile) = api.get_scr_profile(&name, gw) {
            let history_key = ProfileHistoryKey::new(&name, gw);
            if let Some(history) = profile_history.as_deref_mut()
                && !history.has_matches(&history_key)
                && let Err(err) = seed_profile_history(api, cfg, &profile, &name, gw, history)
            {
                tracing::warn!(error = %err, "failed to seed profile history");
            }

            let (mr, lines, _results, self_dodged, opp_dodged) =
                api.profile_stats_last100(&profile, &name, profile_history, Some(&history_key));
            app.self_profile.main_race = mr;
            app.self_profile.matchups = lines;
            app.self_profile.self_dodged = self_dodged;
            app.self_profile.opponent_dodged = opp_dodged;
        }
        app.self_profile.last_rating_poll = Some(std::time::Instant::now());
        app.self_profile.profile_fetched = true;
        OverlayService::write_rating(cfg, app)?;
        Ok(())
    }

    pub fn poll_self_rating(
        app: &mut App,
        cfg: &Config,
        mut profile_history: Option<&mut ProfileHistoryService>,
    ) -> Result<(), ProfileError> {
        if app.detection.screp_available {
            return Ok(());
        }
        let due = app
            .self_profile
            .last_rating_poll
            .is_none_or(|t| t.elapsed() >= cfg.rating_poll_interval);
        if !due {
            return Ok(());
        }
        let (api, name, gw) = match (
            &app.detection.api,
            &app.self_profile.name,
            app.self_profile.gateway,
        ) {
            (Some(api), Some(name), Some(gw)) => (api, name.clone(), gw),
            _ => return Ok(()),
        };

        let info = api.get_toon_info(&name, gw).map_err(ProfileError::Api)?;
        app.self_profile.rating = api.compute_rating_for_name(&info, &name);
        app.self_profile.last_rating_poll = Some(std::time::Instant::now());
        if let Ok(profile) = api.get_scr_profile(&name, gw) {
            let history_key = ProfileHistoryKey::new(&name, gw);
            if let Some(history) = profile_history.as_deref_mut()
                && !history.has_matches(&history_key)
                && let Err(err) = seed_profile_history(api, cfg, &profile, &name, gw, history)
            {
                tracing::warn!(error = %err, "failed to seed profile history");
            }

            let (mr, lines, _results, self_dodged, opp_dodged) =
                api.profile_stats_last100(&profile, &name, profile_history, Some(&history_key));
            app.self_profile.main_race = mr;
            app.self_profile.matchups = lines;
            app.self_profile.self_dodged = self_dodged;
            app.self_profile.opponent_dodged = opp_dodged;
        }
        OverlayService::write_rating(cfg, app)?;
        Ok(())
    }
}

fn seed_profile_history(
    api: &crate::api::ApiHandle,
    cfg: &Config,
    profile: &bw_web_api_rs::models::aurora_profile::ScrProfile,
    main_name: &str,
    gateway: u16,
    history: &mut ProfileHistoryService,
) -> AnyhowResult<()> {
    let history_key = ProfileHistoryKey::new(main_name, gateway);
    if history.has_matches(&history_key) {
        return Ok(());
    }
    if profile.replays.is_empty() {
        return Ok(());
    }

    let workspace = SeedWorkspace::create(&cfg.replay_library_root)?;

    info!(
        replays = profile.replays.len(),
        "seeding profile history from recent replays"
    );

    let lookup = GameLookup::new(profile);

    let client = Client::builder()
        .build()
        .context("create HTTP client for replay seeding")?;

    let mut processed = 0usize;
    let mut dodges = 0usize;
    for replay in profile.replays.iter() {
        if processed >= 25 {
            break;
        }
        let mut was_dodge = false;
        match fetch_replay_duration(api, cfg, &client, workspace.path(), replay) {
            Ok(Some(duration)) if duration < 60 => was_dodge = true,
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    replay_link = %replay.link,
                    "failed to obtain replay duration; defaulting to API result"
                );
            }
        }

        match seed_single_replay(main_name, history, &history_key, replay, &lookup, was_dodge) {
            Ok(true) => {
                processed += 1;
                if was_dodge {
                    dodges += 1;
                }
            }
            Ok(false) => {}
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    replay_link = %replay.link,
                    "failed to seed replay entry"
                );
            }
        }
    }

    info!(processed, dodges, "profile history seeding complete");
    Ok(())
}

fn seed_single_replay(
    main_name: &str,
    history: &mut ProfileHistoryService,
    history_key: &ProfileHistoryKey,
    replay: &bw_web_api_rs::models::common::Replay,
    lookup: &GameLookup,
    was_dodge: bool,
) -> AnyhowResult<bool> {
    let game = lookup.find(replay);

    if game.is_none() {
        tracing::debug!(replay_link = %replay.link, "seeding skipped: no matching game result");
        return Ok(false);
    }
    let game = game.unwrap();

    let actual: Vec<&bw_web_api_rs::models::common::Player> = game
        .players
        .iter()
        .filter(|p| p.attributes.r#type == "player" && !p.toon.trim().is_empty())
        .collect();
    if actual.len() != 2 {
        return Ok(false);
    }

    let mi = if actual[0].toon.eq_ignore_ascii_case(main_name) {
        0
    } else if actual[1].toon.eq_ignore_ascii_case(main_name) {
        1
    } else {
        return Ok(false);
    };
    let oi = 1 - mi;
    let main_player = actual[mi];
    let opp_player = actual[oi];

    let mut outcome = if main_player.result.eq_ignore_ascii_case("win") {
        MatchOutcome::Win
    } else {
        MatchOutcome::Loss
    };

    if was_dodge {
        outcome = match outcome {
            MatchOutcome::Win => MatchOutcome::OpponentDodged,
            _ => MatchOutcome::SelfDodged,
        };
    }

    let timestamp = game.create_time.parse::<u64>().unwrap_or(0);
    let opponent_name = if opp_player.toon.trim().is_empty() {
        "Unknown".to_string()
    } else {
        opp_player.toon.clone()
    };

    let stored = StoredMatch {
        timestamp,
        opponent: opponent_name,
        opponent_race: opp_player.attributes.race.clone(),
        main_race: main_player.attributes.race.clone(),
        result: outcome,
    };

    history.upsert_match(history_key, stored)?;
    Ok(true)
}

fn fetch_replay_duration(
    api: &crate::api::ApiHandle,
    cfg: &Config,
    client: &Client,
    seed_root: &Path,
    replay: &bw_web_api_rs::models::common::Replay,
) -> AnyhowResult<Option<u32>> {
    let detail = api
        .get_matchmaker_player_info(&replay.link)
        .with_context(|| format!("fetch matchmaker replay detail: {}", replay.link))?;
    let Some(best) = detail
        .replays
        .into_iter()
        .max_by(|a, b| a.create_time.cmp(&b.create_time))
    else {
        return Ok(None);
    };

    if best.url.trim().is_empty() {
        return Ok(None);
    }

    let identifier = if !best.md5.is_empty() {
        best.md5.clone()
    } else if !replay.attributes.game_id.is_empty() {
        replay.attributes.game_id.clone()
    } else {
        replay.link.clone()
    };
    let sanitized = sanitize_identifier(&identifier);
    let tmp_path = seed_root.join(format!("{sanitized}.rep"));

    download_replay(client, &best.url, &tmp_path)
        .with_context(|| format!("download replay {}", best.url))?;
    let overview = run_screp_overview(cfg, &tmp_path)?;
    let _ = fs::remove_file(&tmp_path);

    Ok(parse_screp_duration_seconds(&overview))
}

struct SeedWorkspace {
    path: std::path::PathBuf,
}

impl SeedWorkspace {
    fn create(root: &Path) -> AnyhowResult<Self> {
        let path = root.join(".seed_tmp");
        if path.exists() {
            let _ = fs::remove_dir_all(&path);
        }
        fs::create_dir_all(&path).context("create seed replay temp directory")?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for SeedWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct GameLookup<'a> {
    by_id: HashMap<&'a str, &'a bw_web_api_rs::models::common::GameResult>,
    by_match: HashMap<&'a str, &'a bw_web_api_rs::models::common::GameResult>,
}

impl<'a> GameLookup<'a> {
    fn new(profile: &'a bw_web_api_rs::models::aurora_profile::ScrProfile) -> Self {
        let mut by_id = HashMap::new();
        let mut by_match = HashMap::new();
        for game in &profile.game_results {
            by_id.insert(game.game_id.as_str(), game);
            by_match.insert(game.match_guid.as_str(), game);
        }
        Self { by_id, by_match }
    }

    fn find(
        &self,
        replay: &bw_web_api_rs::models::common::Replay,
    ) -> Option<&'a bw_web_api_rs::models::common::GameResult> {
        if let Some(game) = self.by_id.get(replay.attributes.game_id.as_str()) {
            return Some(*game);
        }
        if let Some(game) = self.by_match.get(replay.link.as_str()) {
            return Some(*game);
        }
        None
    }
}
