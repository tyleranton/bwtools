use crate::app::App;
use crate::config::Config;
use anyhow::{Context, Result};
use std::path::Path;

fn write_if_changed(path: &Path, last_text: &mut Option<String>, text: String) -> Result<()> {
    if last_text.as_deref() == Some(text.as_str()) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create overlay directory {}", parent.display()))?;
    }
    std::fs::write(path, &text)
        .with_context(|| format!("write overlay file {}", path.display()))?;
    *last_text = Some(text);
    Ok(())
}

pub fn write_rating(cfg: &Config, app: &mut App) -> Result<()> {
    if !cfg.rating_output_enabled {
        return Ok(());
    }
    let text = match app.self_profile_rating {
        Some(r) => r.to_string(),
        None => "N/A".to_string(),
    };
    write_if_changed(
        &cfg.rating_output_path,
        &mut app.rating_output_last_text,
        text,
    )
}

pub fn write_opponent(cfg: &Config, app: &mut App) -> Result<()> {
    if !cfg.opponent_output_enabled {
        return Ok(());
    }
    let name = match &app.profile_name {
        Some(n) => n.clone(),
        None => {
            return Ok(());
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
    )
}
