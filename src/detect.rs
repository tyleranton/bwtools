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
    #[error("history persistence error")]
    History(#[source] anyhow::Error),
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
        detect_port(app, cfg, reader);
        detect_self_bootstrap(app, cfg, reader);
        init_api(app);
        detect_self_switch(app, cfg, reader)?;
        detect_opponent(app, cfg, reader, history)?;
        Ok(())
    }
}

fn detect_port(app: &mut App, cfg: &Config, reader: &mut CacheReader) {
    if app.detection.port.is_some() {
        return;
    }

    match reader.parse_for_port(cfg.scan_window_secs) {
        Ok(Some(port)) => {
            app.debug.port_text = Some(format!("Detected API port: {}", port));
            app.detection.port = Some(port);
        }
        Ok(None) => {}
        Err(err) => tracing::warn!(error = %err, "failed to read port from cache"),
    }
}

fn detect_self_bootstrap(app: &mut App, cfg: &Config, reader: &mut CacheReader) {
    if app.detection.port.is_none() || app.self_profile.name.is_some() {
        return;
    }

    match reader.latest_self_profile(cfg.scan_window_secs) {
        Ok(Some((name, gw))) => {
            app.self_profile.name = Some(name);
            app.self_profile.gateway = Some(gw);
        }
        Ok(None) => {}
        Err(err) => tracing::warn!(error = %err, "failed to read self profile from cache"),
    }
}

fn init_api(app: &mut App) {
    if let Some(p) = app.detection.port {
        let stale = app.detection.api.is_none() || app.detection.last_port_used != Some(p);
        if stale {
            let base_url = format!("http://127.0.0.1:{p}");
            app.detection.api = crate::api::ApiHandle::new(base_url).ok();
            app.detection.last_port_used = Some(p);
        }
    }
}

fn detect_self_switch(
    app: &mut App,
    cfg: &Config,
    reader: &mut CacheReader,
) -> Result<(), DetectionError> {
    if !app.is_ready() {
        return Ok(());
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
                app.self_profile.name = Some(mm_name);
                app.self_profile.gateway = Some(mm_gw);
                app.self_profile.rating = None;
                app.self_profile.profile_fetched = false;
                app.status.last_profile_text = None;
                app.self_profile.last_rating_poll = None;
                app.reset_opponent_state();
                OverlayService::write_rating(cfg, app)?;
            } else if !is_own {
                tracing::debug!(
                    mm_name = %mm_name,
                    mm_gateway = mm_gw,
                    "mmgameloading entry ignored because profile not owned"
                );
            } else {
                tracing::debug!(
                    mm_name = %mm_name,
                    mm_gateway = mm_gw,
                    "mmgameloading entry matched current self profile"
                );
            }
        }
        Ok(None) => {}
        Err(err) => tracing::warn!(error = %err, "failed to detect self switch"),
    }
    Ok(())
}

fn detect_opponent(
    app: &mut App,
    cfg: &Config,
    reader: &mut CacheReader,
    history: Option<&HistoryService<FileHistorySource>>,
) -> Result<(), DetectionError> {
    if !app.is_ready() {
        return Ok(());
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
                return Ok(());
            }

            tracing::debug!(
                opponent = %name,
                gateway = gw,
                "mmgameloading opponent candidate detected"
            );
            app.opponent.name = Some(name);
            app.opponent.gateway = Some(gw);

            if let (Some(api), Some(opp_name), Some(opp_gw)) =
                (&app.detection.api, &app.opponent.name, app.opponent.gateway)
            {
                let identity = (opp_name.clone(), opp_gw);
                if app.opponent.last_identity.as_ref() == Some(&identity) {
                    tracing::debug!(opponent = %opp_name, gateway = opp_gw, "opponent identity unchanged; skipping refresh");
                    return Ok(());
                }

                app.opponent.race = None;
                app.opponent.matchups.clear();

                match api.opponent_toons_summary(opp_name, opp_gw) {
                    Ok(list) => {
                        tracing::debug!(
                            opponent = %opp_name,
                            gateway = opp_gw,
                            toon_count = list.len(),
                            "opponent toons summary fetched"
                        );
                        app.opponent.toons_data = list.clone();
                        app.opponent.toons = list
                            .into_iter()
                            .map(|(toon, gw_num, rating)| {
                                format!(
                                    "{} • {} • {}",
                                    toon,
                                    crate::api::gateway_label(gw_num),
                                    rating,
                                )
                            })
                            .collect();
                        app.opponent.last_identity = Some(identity);
                    }
                    Err(err) => tracing::error!(error = %err, "opponent toons summary failed"),
                }

                match api.get_scr_profile(opp_name, opp_gw) {
                    Ok(profile) => {
                        let (mr, lines, _results, _, _) =
                            api.profile_stats_last100(&profile, opp_name, None, None);
                        tracing::debug!(
                            opponent = %opp_name,
                            gateway = opp_gw,
                            main_race = ?mr,
                            "opponent profile fetched"
                        );
                        app.opponent.race = mr;
                        app.opponent.matchups = lines;
                    }
                    Err(err) => tracing::error!(error = %err, "opponent profile fetch failed"),
                }

                match api.get_toon_info(opp_name, opp_gw) {
                    Ok(info) => {
                        let season = info.matchmaked_current_season;
                        let profiles = info.profiles.as_deref().unwrap_or(&[]);
                        let guid = profiles
                            .iter()
                            .find(|p| p.toon.eq_ignore_ascii_case(opp_name))
                            .map(|p| p.toon_guid)
                            .or_else(|| {
                                info.matchmaked_stats
                                    .iter()
                                    .find(|s| {
                                        s.season_id == season
                                            && s.toon.eq_ignore_ascii_case(opp_name)
                                    })
                                    .map(|s| s.toon_guid)
                            });
                        let rating = guid.and_then(|g| api.compute_rating_for_guid(&info, g));

                        let key = opp_name.to_ascii_lowercase();
                        let is_new = !app.opponent.history.contains_key(&key);
                        let entry = app.opponent.history.entry(key.clone()).or_insert_with(|| {
                            OpponentRecord {
                                name: opp_name.clone(),
                                gateway: opp_gw,
                                race: None,
                                current_rating: None,
                                previous_rating: None,
                                wins: 0,
                                losses: 0,
                                last_match_ts: None,
                            }
                        });

                        entry.name = opp_name.clone();
                        entry.gateway = opp_gw;
                        entry.previous_rating = entry.current_rating;
                        entry.current_rating = rating;

                        let no_wl = entry.wins + entry.losses == 0;
                        if (is_new || no_wl)
                            && let (Some(self_name), Some(self_gw)) =
                                (&app.self_profile.name, app.self_profile.gateway)
                        {
                            match api.get_scr_profile(self_name, self_gw) {
                                Ok(profile) => {
                                    let (w, l, ts, race) =
                                        derive_wl_and_race(&profile, self_name, opp_name);
                                    entry.wins = w;
                                    entry.losses = l;
                                    entry.last_match_ts = ts;
                                    if entry.race.is_none() {
                                        entry.race =
                                            race.map(|s| match s.to_lowercase().as_str() {
                                                "protoss" => "Protoss".to_string(),
                                                "terran" => "Terran".to_string(),
                                                "zerg" => "Zerg".to_string(),
                                                _ => s,
                                            });
                                    }
                                }
                                Err(err) => {
                                    tracing::error!(error = %err, "self profile fetch for opponent history failed")
                                }
                            }
                        }

                        if let Some(service) = history {
                            service
                                .save(&app.opponent.history)
                                .map_err(DetectionError::History)?;
                        }
                    }
                    Err(err) => return Err(DetectionError::Api(err)),
                }
            }
        }
        Ok(None) => {}
        Err(err) => tracing::warn!(error = %err, "failed to read opponent profile from cache"),
    }
    Ok(())
}
