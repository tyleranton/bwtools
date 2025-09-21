use crate::app::App;
use thiserror::Error;

pub struct SearchService;

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("api error")]
    Api(#[source] anyhow::Error),
}

impl SearchService {
    pub fn run(app: &mut App) -> Result<(), SearchError> {
        app.search.in_progress = false;
        app.search.error = None;
        app.search.rating = None;
        app.search.other_toons.clear();
        app.search.matches.clear();
        app.search.matches_scroll = 0;
        app.search.main_race = None;
        app.search.matchups.clear();
        if let (Some(api), true) = (&app.api, !app.search.name.trim().is_empty()) {
            let name = app.search.name.trim().to_string();
            let gw = app.search.gateway;
            match api.get_toon_info(&name, gw) {
                Ok(info) => {
                    let season = info.matchmaked_current_season;
                    let profiles = info.profiles.as_deref().unwrap_or(&[]);
                    let guid = profiles
                        .iter()
                        .find(|p| p.toon.eq_ignore_ascii_case(&name))
                        .map(|p| p.toon_guid)
                        .or_else(|| {
                            info.matchmaked_stats
                                .iter()
                                .find(|s| {
                                    s.season_id == season && s.toon.eq_ignore_ascii_case(&name)
                                })
                                .map(|s| s.toon_guid)
                        });
                    app.search.rating = guid.and_then(|g| api.compute_rating_for_guid(&info, g));
                    let others = api.other_toons_with_ratings(&info, &name);
                    app.search.other_toons_data = others.clone();
                    app.search.other_toons = others
                        .into_iter()
                        .map(|(toon, gw_num, rating)| {
                            format!(
                                "{} • {} • {}",
                                toon,
                                crate::api::gateway_label(gw_num),
                                rating
                            )
                        })
                        .collect();
                    let eligible = guid
                        .map(|g| {
                            let season = info.matchmaked_current_season;
                            info.matchmaked_stats
                                .iter()
                                .filter(|s| s.toon_guid == g && s.season_id == season)
                                .fold(0u32, |acc, s| acc.saturating_add(s.wins + s.losses))
                        })
                        .map(|n| n >= crate::api::RATING_MIN_GAMES)
                        .unwrap_or(false);
                    match api.get_scr_profile(&name, gw) {
                        Ok(profile) => {
                            if eligible {
                                app.search.matches = api.match_summaries(&profile, &name);
                            } else {
                                app.search.matches.clear();
                            }
                            let (mr, lines, _results) = api.profile_stats_last100(&profile, &name);
                            app.search.main_race = mr;
                            app.search.matchups = lines;
                        }
                        Err(e) => {
                            app.search.error = Some(format!("profile error: {}", e));
                        }
                    }
                }
                Err(e) => {
                    app.search.error = Some(e.to_string());
                    return Err(SearchError::Api(e));
                }
            }
        }
        Ok(())
    }
}
