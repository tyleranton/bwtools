use crate::app::App;
use crate::cache::CacheReader;
use crate::config::Config;
use crate::history::{FileHistorySource, HistoryService, OpponentRecord, derive_wl_and_race};
use crate::overlay::{OverlayError, OverlayService};
use thiserror::Error;

pub struct DetectionService;

#[derive(Debug, Error)]
pub enum DetectionError {
    #[error("overlay error")]
    Overlay(#[from] OverlayError),
    #[error("api error")]
    Api(#[source] anyhow::Error),
}

impl DetectionService {
    pub fn tick(
        app: &mut App,
        cfg: &Config,
        reader: &mut CacheReader,
        history: Option<&HistoryService<FileHistorySource>>,
    ) -> Result<(), DetectionError> {
        let outcome = DetectionOutcome {
            port: detect_port(app, cfg, reader),
            self_bootstrap: detect_self_bootstrap(app, cfg, reader),
            api_initialized: init_api(app),
            self_switch: detect_self_switch(app, cfg, reader)?,
            opponent: detect_opponent(app, cfg, reader)?,
        };

        outcome.apply(app, cfg, history);
        Ok(())
    }
}

#[derive(Default)]
struct DetectionOutcome {
    port: Option<u16>,
    self_bootstrap: Option<(String, u16)>,
    api_initialized: bool,
    self_switch: Option<SelfProfileSwitch>,
    opponent: Option<OpponentOutcome>,
}

impl DetectionOutcome {
    fn apply(
        self,
        app: &mut App,
        cfg: &Config,
        history: Option<&HistoryService<FileHistorySource>>,
    ) {
        if let Some(port) = self.port {
            app.detection.port = Some(port);
            app.debug.port_text = Some(format!("Detected API port: {}", port));
        }

        if let Some((name, gw)) = self.self_bootstrap {
            app.self_profile.name = Some(name);
            app.self_profile.gateway = Some(gw);
        }

        if self.api_initialized
            && let Some(port) = app.detection.port
        {
            app.detection.last_port_used = Some(port);
        }

        if let Some(switch) = self.self_switch {
            switch.apply(app, cfg);
        }

        if let Some(opp) = self.opponent {
            opp.apply(app, cfg, history);
        }
    }
}

struct SelfProfileSwitch {
    name: String,
    gateway: u16,
}

impl SelfProfileSwitch {
    fn apply(self, app: &mut App, cfg: &Config) {
        app.self_profile.name = Some(self.name);
        app.self_profile.gateway = Some(self.gateway);
        app.self_profile.rating = None;
        app.self_profile.profile_fetched = false;
        app.status.last_profile_text = None;
        app.self_profile.last_rating_poll = None;
        app.reset_opponent_state();
        if let Err(err) = OverlayService::write_rating(cfg, app) {
            tracing::error!(error = %err, "failed to update overlay after self switch");
        }
    }
}

struct OpponentOutcome {
    name: String,
    gateway: u16,
    toons: Vec<(String, u16, u32)>,
    race: Option<String>,
    matchups: Vec<String>,
    last_identity: Option<(String, u16)>,
    history_update: Option<OpponentHistoryUpdate>,
}

impl OpponentOutcome {
    fn apply(
        self,
        app: &mut App,
        cfg: &Config,
        history: Option<&HistoryService<FileHistorySource>>,
    ) {
        app.opponent.name = Some(self.name.clone());
        app.opponent.gateway = Some(self.gateway);
        if let Some(id) = self.last_identity {
            app.opponent.last_identity = Some(id);
        }
        app.opponent.toons_data = self.toons.clone();
        app.opponent.toons = self
            .toons
            .into_iter()
            .map(|(toon, gw, rating)| {
                format!("{} • {} • {}", toon, crate::api::gateway_label(gw), rating)
            })
            .collect();
        app.opponent.race = self.race.clone();
        app.opponent.matchups = self.matchups.clone();

        if let Some(update) = self.history_update {
            update.apply(app, cfg, history);
        }
    }
}

fn detect_port(app: &App, cfg: &Config, reader: &mut CacheReader) -> Option<u16> {
    if app.detection.port.is_some() {
        return None;
    }

    match reader.parse_for_port(cfg.scan_window_secs) {
        Ok(port) => port,
        Err(err) => {
            tracing::warn!(error = %err, "failed to read port from cache");
            None
        }
    }
}

fn detect_self_bootstrap(
    app: &App,
    cfg: &Config,
    reader: &mut CacheReader,
) -> Option<(String, u16)> {
    if app.detection.port.is_none() || app.self_profile.name.is_some() {
        return None;
    }

    match reader.latest_self_profile(cfg.scan_window_secs) {
        Ok(result) => result,
        Err(err) => {
            tracing::warn!(error = %err, "failed to read self profile from cache");
            None
        }
    }
}

fn init_api(app: &mut App) -> bool {
    if let Some(p) = app.detection.port {
        let stale = app.detection.api.is_none() || app.detection.last_port_used != Some(p);
        if stale {
            let base_url = format!("http://127.0.0.1:{p}");
            app.detection.api = crate::api::ApiHandle::new(base_url).ok();
            return app.detection.api.is_some();
        }
    }
    false
}

fn detect_self_switch(
    app: &App,
    cfg: &Config,
    reader: &mut CacheReader,
) -> Result<Option<SelfProfileSwitch>, DetectionError> {
    if !app.is_ready() {
        return Ok(None);
    }

    match reader.latest_mmgameloading_profile(cfg.scan_window_secs) {
        Ok(Some((mm_name, mm_gw))) => {
            let is_own = app.self_profile.own_profiles.contains(&mm_name);
            let current_name = app.self_profile.name.as_deref().unwrap_or("<none>");
            let current_gateway = app.self_profile.gateway.unwrap_or(0);
            tracing::debug!(
                mm_name = %mm_name,
                mm_gateway = mm_gw,
                current_name,
                current_gateway,
                is_known_own = is_own,
                "mmgameloading entry observed"
            );
            let changed_name = app.self_profile.name.as_deref() != Some(&mm_name);
            let changed_gw = app.self_profile.gateway != Some(mm_gw);
            if is_own && (changed_name || changed_gw) {
                return Ok(Some(SelfProfileSwitch {
                    name: mm_name,
                    gateway: mm_gw,
                }));
            }
        }
        Ok(None) => {}
        Err(err) => tracing::warn!(error = %err, "failed to detect self switch"),
    }
    Ok(None)
}

fn detect_opponent(
    app: &App,
    cfg: &Config,
    reader: &mut CacheReader,
) -> Result<Option<OpponentOutcome>, DetectionError> {
    if !app.is_ready() {
        return Ok(None);
    }

    let self_name = app.self_profile.name.as_deref();
    match reader.latest_opponent_profile(self_name, cfg.scan_window_secs) {
        Ok(Some((name, gw))) => {
            if app.self_profile.own_profiles.contains(&name) {
                tracing::debug!(
                    opponent = %name,
                    gateway = gw,
                    "ignoring opponent candidate because it is an owned profile"
                );
                return Ok(None);
            }

            tracing::debug!(
                opponent = %name,
                gateway = gw,
                "mmgameloading opponent candidate detected"
            );
            if let Some(api) = app.detection.api.as_ref() {
                return build_opponent_outcome(app, api, &name, gw);
            }
        }
        Ok(None) => {}
        Err(err) => tracing::warn!(error = %err, "failed to read opponent profile from cache"),
    }
    Ok(None)
}

fn build_opponent_outcome(
    app: &App,
    api: &crate::api::ApiHandle,
    opp_name: &str,
    opp_gw: u16,
) -> Result<Option<OpponentOutcome>, DetectionError> {
    let identity = (opp_name.to_string(), opp_gw);
    if app.opponent.last_identity.as_ref() == Some(&identity) {
        tracing::debug!(
            opponent = %opp_name,
            gateway = opp_gw,
            "opponent identity unchanged; skipping refresh"
        );
        return Ok(None);
    }

    let toons = match api.opponent_toons_summary(opp_name, opp_gw) {
        Ok(list) => {
            tracing::debug!(
                opponent = %opp_name,
                gateway = opp_gw,
                toon_count = list.len(),
                "opponent toons summary fetched"
            );
            list
        }
        Err(err) => {
            tracing::error!(error = %err, "opponent toons summary failed");
            Vec::new()
        }
    };

    let (race, matchups) = match api.get_scr_profile(opp_name, opp_gw) {
        Ok(profile) => {
            let (mr, lines, _results, _, _) =
                api.profile_stats_last100(&profile, opp_name, None, None);
            tracing::debug!(
                opponent = %opp_name,
                gateway = opp_gw,
                main_race = ?mr,
                "opponent profile fetched"
            );
            (mr, lines)
        }
        Err(err) => {
            tracing::error!(error = %err, "opponent profile fetch failed");
            (None, Vec::new())
        }
    };

    Ok(Some(OpponentOutcome {
        name: opp_name.to_string(),
        gateway: opp_gw,
        toons,
        race,
        matchups,
        last_identity: Some(identity),
        history_update: match api.get_toon_info(opp_name, opp_gw) {
            Ok(info) => build_history_update(app, api, opp_name, opp_gw, info),
            Err(err) => return Err(DetectionError::Api(err)),
        },
    }))
}

fn build_history_update(
    app: &App,
    api: &crate::api::ApiHandle,
    opp_name: &str,
    opp_gw: u16,
    info: bw_web_api_rs::models::aurora_profile::ScrToonInfo,
) -> Option<OpponentHistoryUpdate> {
    let season = info.matchmaked_current_season;
    let profiles = info.profiles.as_deref().unwrap_or(&[]);
    let guid = profiles
        .iter()
        .find(|p| p.toon.eq_ignore_ascii_case(opp_name))
        .map(|p| p.toon_guid)
        .or_else(|| {
            info.matchmaked_stats
                .iter()
                .find(|s| s.season_id == season && s.toon.eq_ignore_ascii_case(opp_name))
                .map(|s| s.toon_guid)
        });
    let rating = guid.and_then(|g| api.compute_rating_for_guid(&info, g));

    let key = opp_name.to_ascii_lowercase();
    let existing = app.opponent.history.get(&key);

    let mut wins = existing.map(|r| r.wins).unwrap_or(0);
    let mut losses = existing.map(|r| r.losses).unwrap_or(0);
    let mut last_match_ts = existing.and_then(|r| r.last_match_ts);
    let mut race = existing.and_then(|r| r.race.clone());
    let previous_rating = existing.and_then(|r| r.current_rating);

    let needs_record = existing.map(|r| r.wins + r.losses == 0).unwrap_or(true);
    if needs_record
        && let (Some(self_name), Some(self_gw)) = (&app.self_profile.name, app.self_profile.gateway)
    {
        match api.get_scr_profile(self_name, self_gw) {
            Ok(profile) => {
                let (w, l, ts, race_opt) = derive_wl_and_race(&profile, self_name, opp_name);
                wins = w;
                losses = l;
                last_match_ts = ts;
                if race.is_none() {
                    race = race_opt.map(|s| match s.to_lowercase().as_str() {
                        "protoss" => "Protoss".to_string(),
                        "terran" => "Terran".to_string(),
                        "zerg" => "Zerg".to_string(),
                        _ => s,
                    });
                }
            }
            Err(err) => tracing::error!(
                error = %err,
                "self profile fetch for opponent history failed"
            ),
        }
    }

    Some(OpponentHistoryUpdate {
        key,
        gateway: opp_gw,
        race,
        last_match_ts,
        wins,
        losses,
        current_rating: rating,
        previous_rating,
    })
}

struct OpponentHistoryUpdate {
    key: String,
    gateway: u16,
    race: Option<String>,
    last_match_ts: Option<u64>,
    wins: u32,
    losses: u32,
    current_rating: Option<u32>,
    previous_rating: Option<u32>,
}

impl OpponentHistoryUpdate {
    fn apply(
        self,
        app: &mut App,
        cfg: &Config,
        history: Option<&HistoryService<FileHistorySource>>,
    ) {
        let entry = app
            .opponent
            .history
            .entry(self.key)
            .or_insert_with(|| OpponentRecord {
                name: app.opponent.name.clone().unwrap_or_default(),
                gateway: self.gateway,
                race: None,
                current_rating: None,
                previous_rating: None,
                wins: 0,
                losses: 0,
                last_match_ts: None,
            });

        if let Some(name) = &app.opponent.name {
            entry.name = name.clone();
        }
        entry.gateway = self.gateway;
        if let Some(ts) = self.last_match_ts {
            entry.last_match_ts = Some(ts);
        }
        if entry.race.is_none() {
            entry.race = self.race.clone();
        }
        entry.previous_rating = self.previous_rating;
        entry.current_rating = self.current_rating;
        entry.wins = self.wins;
        entry.losses = self.losses;

        if let Err(err) = OverlayService::write_opponent(cfg, app) {
            tracing::error!(error = %err, "failed to update opponent overlay");
        }

        if let Some(service) = history
            && let Err(err) = service.save(&app.opponent.history)
        {
            tracing::error!(error = %err, "failed to persist opponent history");
        }
    }
}
