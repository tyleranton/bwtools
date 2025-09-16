use crate::app::App;
use crate::config::Config;
use std::path::Path;

pub fn write_if_changed(path: &Path, last_text: &mut Option<String>, text: String) {
    if last_text.as_deref() == Some(text.as_str()) {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, &text);
    *last_text = Some(text);
}

pub fn write_rating(cfg: &Config, app: &mut App) {
    if !cfg.rating_output_enabled {
        return;
    }
    let text = match app.self_profile_rating {
        Some(r) => r.to_string(),
        None => "N/A".to_string(),
    };
    write_if_changed(
        &cfg.rating_output_path,
        &mut app.rating_output_last_text,
        text,
    );
}

pub fn write_opponent(cfg: &Config, app: &mut App) {
    if !cfg.opponent_output_enabled {
        return;
    }
    let name = match &app.profile_name {
        Some(n) => n.clone(),
        None => {
            return;
        }
    };
    let race = app
        .opponent_race
        .clone()
        .unwrap_or_else(|| "Unknown".to_string());
    let rating_opt = app
        .opponent_toons_data
        .iter()
        .find(|(t, _, _)| t.eq_ignore_ascii_case(&name))
        .map(|(_, _, r)| *r);
    let rating_text = rating_opt
        .map(|r| r.to_string())
        .unwrap_or_else(|| "N/A".to_string());
    let wl_text = app
        .opponent_history
        .get(&name.to_ascii_lowercase())
        .filter(|rec| rec.wins + rec.losses > 0)
        .map(|rec| format!(" • W-L {}-{}", rec.wins, rec.losses))
        .unwrap_or_default();
    let text = format!("{} • {} • {}{}", name, race, rating_text, wl_text);
    write_if_changed(
        &cfg.opponent_output_path,
        &mut app.opponent_output_last_text,
        text,
    );
}
