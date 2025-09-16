use crate::app::App;
use crate::config::Config;
use crate::overlay;

pub fn fetch_self_profile(app: &mut App, cfg: &Config) {
    if let (Some(api), Some(name), Some(gw)) = (&app.api, &app.self_profile_name, app.self_gateway)
    {
        match api.get_toon_info(name, gw) {
            Ok(info) => {
                let mut out = String::new();
                out.push_str(&format!("profiles ({}):\n", info.profiles.len()));
                for (i, p) in info.profiles.iter().enumerate() {
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

                app.own_profiles = info.profiles.iter().map(|p| p.toon.clone()).collect();
                if let Ok(profile) = api.get_scr_profile(name, gw) {
                    let (mr, lines, _results) = api.profile_stats_last100(&profile, name);
                    app.self_main_race = mr;
                    app.self_matchups = lines;
                }
                app.last_rating_poll = Some(std::time::Instant::now());
                app.profile_fetched = true;
                overlay::write_rating(cfg, app);
            }
            Err(err) => {
                app.last_profile_text = Some(format!("API error: {}", err));
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
                overlay::write_rating(cfg, app);
            }
            Err(_) => {
                app.last_rating_poll = Some(std::time::Instant::now());
            }
        }
    }
}
