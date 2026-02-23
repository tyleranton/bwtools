use crate::app::App;
use crate::config::Config;
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
            let wl_text = app
                .opponent
                .history
                .get(&name.to_ascii_lowercase())
                .filter(|rec| rec.wins + rec.losses > 0)
                .map(|rec| format!(" • W-L {}-{}", rec.wins, rec.losses))
                .unwrap_or_default();
            format!("{} • {} • {}{}", name, race, rating_text, wl_text)
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
