use crate::app::App;
use crate::config::Config;
use crate::overlay::{OverlayError, OverlayService};
use thiserror::Error;

pub struct ProfileService;

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("api error")]
    Api(#[source] anyhow::Error),
    #[error("overlay error")]
    Overlay(#[from] OverlayError),
}

impl ProfileService {
    pub fn fetch_self_profile(app: &mut App, cfg: &Config) -> Result<(), ProfileError> {
        let (api, name, gw) = match (&app.api, &app.self_profile_name, app.self_gateway) {
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
        app.last_profile_text = Some(out);
        app.self_profile_rating = api.compute_rating_for_name(&info, &name);

        app.own_profiles = profiles.iter().map(|p| p.toon.clone()).collect();
        if let Ok(profile) = api.get_scr_profile(&name, gw) {
            let (mr, lines, _results) = api.profile_stats_last100(&profile, &name);
            app.self_main_race = mr;
            app.self_matchups = lines;
        }
        app.last_rating_poll = Some(std::time::Instant::now());
        app.profile_fetched = true;
        OverlayService::write_rating(cfg, app)?;
        Ok(())
    }

    pub fn poll_self_rating(app: &mut App, cfg: &Config) -> Result<(), ProfileError> {
        if app.screp_available {
            return Ok(());
        }
        let due = app
            .last_rating_poll
            .is_none_or(|t| t.elapsed() >= cfg.rating_poll_interval);
        if !due {
            return Ok(());
        }
        let (api, name, gw) = match (&app.api, &app.self_profile_name, app.self_gateway) {
            (Some(api), Some(name), Some(gw)) => (api, name.clone(), gw),
            _ => return Ok(()),
        };

        let info = api.get_toon_info(&name, gw).map_err(ProfileError::Api)?;
        app.self_profile_rating = api.compute_rating_for_name(&info, &name);
        app.last_rating_poll = Some(std::time::Instant::now());
        if let Ok(profile) = api.get_scr_profile(&name, gw) {
            let (mr, lines, _results) = api.profile_stats_last100(&profile, &name);
            app.self_main_race = mr;
            app.self_matchups = lines;
        }
        OverlayService::write_rating(cfg, app)?;
        Ok(())
    }
}
