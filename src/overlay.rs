use crate::app::App;
use crate::config::Config;
use crate::history::aggregate_record_for_aurora_id;
use crate::player_list::display_name_for_opponent;
use std::path::{Path, PathBuf};
use std::{fs, io};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OverlayError {
    #[error("create overlay directory {path}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("write overlay file {path}")]
    WriteFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

pub struct OverlayService;

impl OverlayService {
    pub fn write_rating(cfg: &Config, app: &mut App) -> Result<(), OverlayError> {
        if !cfg.rating_output_enabled {
            return Ok(());
        }
        let text = match app.self_profile.rating {
            Some(r) => r.to_string(),
            None => "N/A".to_string(),
        };
        write_if_changed(
            &cfg.rating_output_path,
            &mut app.overlays.rating_last_text,
            text,
        )
    }

    pub fn write_opponent(cfg: &Config, app: &mut App) -> Result<(), OverlayError> {
        if !cfg.opponent_output_enabled {
            return Ok(());
        }

        let text = if app.overlays.opponent_waiting || app.opponent.name.is_none() {
            "Waiting for opponent...".to_string()
        } else {
            let name = app.opponent.name.clone().unwrap_or_default();
            let display_name =
                display_name_for_opponent(&app.known_players, app.opponent.aurora_id, &name);
            let race = app
                .opponent
                .race
                .clone()
                .unwrap_or_else(|| "Unknown".to_string());
            let rating_opt = app
                .opponent
                .toons_data
                .iter()
                .find(|(t, _, _)| t.eq_ignore_ascii_case(&name))
                .map(|(_, _, r)| *r);
            let rating_text = rating_opt
                .map(|r| r.to_string())
                .unwrap_or_else(|| "N/A".to_string());
            let known_id = app
                .opponent
                .aurora_id
                .filter(|id| app.known_players.contains_key(id));
            let wl_text = if let Some(id) = known_id {
                aggregate_record_for_aurora_id(&app.opponent.history, id)
                    .map(|(wins, losses)| format!(" • W-L {}-{}", wins, losses))
                    .unwrap_or_default()
            } else {
                app.opponent
                    .history
                    .get(&name.to_ascii_lowercase())
                    .filter(|rec| rec.wins + rec.losses > 0)
                    .map(|rec| format!(" • W-L {}-{}", rec.wins, rec.losses))
                    .unwrap_or_default()
            };
            format!("{} • {} • {}{}", display_name, race, rating_text, wl_text)
        };
        write_if_changed(
            &cfg.opponent_output_path,
            &mut app.overlays.opponent_last_text,
            text,
        )
    }
}

fn write_if_changed(
    path: &Path,
    last_text: &mut Option<String>,
    text: String,
) -> Result<(), OverlayError> {
    if last_text.as_deref() == Some(text.as_str()) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| OverlayError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    fs::write(path, &text).map_err(|source| OverlayError::WriteFile {
        path: path.to_path_buf(),
        source,
    })?;
    *last_text = Some(text);
    Ok(())
}
