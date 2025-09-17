use crate::app::App;
use crate::config::Config;
use crate::overlay;

pub fn fetch_self_profile(app: &mut App, cfg: &Config) {
    if let (Some(api), Some(name), Some(gw)) = (&app.api, &app.self_profile_name, app.self_gateway)
    {
        match api.get_toon_info(name, gw) {
            Ok(info) => {
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
                app.self_profile_rating = api.compute_rating_for_name(&info, name);

                app.own_profiles = profiles.iter().map(|p| p.toon.clone()).collect();
                if let Ok(profile) = api.get_scr_profile(name, gw) {
                    let (mr, lines, _results) = api.profile_stats_last100(&profile, name);
                    app.self_main_race = mr;
                    app.self_matchups = lines;
                }
                app.last_rating_poll = Some(std::time::Instant::now());
                app.profile_fetched = true;
                if let Err(err) = overlay::write_rating(cfg, app) {
                    tracing::error!(error = %err, "failed to update rating overlay");
                    app.last_profile_text = Some(format!("Overlay error: {err}"));
                }
            }
            Err(err) => {
                tracing::error!(error = %err, "failed to fetch self toon info");
                app.last_profile_text = Some(format!("API error: {err}"));
                app.last_rating_poll = Some(std::time::Instant::now());
                app.profile_fetched = true;
            }
        }
    }
}

pub fn poll_self_rating(app: &mut App, cfg: &Config) {
    if app.screp_available {
        return;
    }
    let due = app
        .last_rating_poll
        .is_none_or(|t| t.elapsed() >= cfg.rating_poll_interval);
    if !due {
        return;
    }
    if let (Some(api), Some(name), Some(gw)) = (&app.api, &app.self_profile_name, app.self_gateway)
    {
        match api.get_toon_info(name, gw) {
            Ok(info) => {
                app.self_profile_rating = api.compute_rating_for_name(&info, name);
                app.last_rating_poll = Some(std::time::Instant::now());
                if let Ok(profile) = api.get_scr_profile(name, gw) {
                    let (mr, lines, _results) = api.profile_stats_last100(&profile, name);
                    app.self_main_race = mr;
                    app.self_matchups = lines;
                }
                if let Err(err) = overlay::write_rating(cfg, app) {
                    tracing::error!(error = %err, "failed to update rating overlay");
                    app.last_profile_text = Some(format!("Overlay error: {err}"));
                }
            }
            Err(err) => {
                app.last_rating_poll = Some(std::time::Instant::now());
                tracing::error!(error = %err, "failed to poll self rating");
            }
        }
    }
}
