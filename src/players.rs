use std::{
    collections::BTreeMap,
    env,
    fs,
    path::PathBuf,
};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
struct PlayerRecord {
    aurora_id: u64,
    battle_tag: String,
}

type PlayerMap = BTreeMap<String, Vec<PlayerRecord>>;

#[derive(Debug, Clone)]
pub struct PlayerEntry {
    pub name: String,
    #[allow(dead_code)]
    pub aurora_id: u64,
    pub battle_tag: String,
}

#[derive(Debug, Clone)]
pub struct PlayerDirectory {
    entries: Vec<PlayerEntry>,
}

impl PlayerDirectory {
    pub fn load() -> Result<Self> {
        let (raw, path) = load_player_list()?;
        let map: PlayerMap = serde_json::from_str(&raw)
            .with_context(|| format!("invalid JSON in {}", path.display()))?;
        Ok(Self {
            entries: flatten_players(&map),
        })
    }

    pub fn entries(&self) -> &[PlayerEntry] {
        &self.entries
    }

    pub fn filter(&self, query: &str) -> Vec<PlayerEntry> {
        let needle = query.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return self.entries.clone();
        }
        self.entries
            .iter()
            .filter(|entry| entry.name.to_ascii_lowercase().contains(&needle))
            .cloned()
            .collect()
    }
}

fn flatten_players(map: &PlayerMap) -> Vec<PlayerEntry> {
    let mut entries = Vec::new();
    for (name, records) in map.iter() {
        for record in records {
            entries.push(PlayerEntry {
                name: name.clone(),
                aurora_id: record.aurora_id,
                battle_tag: record.battle_tag.clone(),
            });
        }
    }
    entries
}

fn load_player_list() -> Result<(String, PathBuf)> {
    let mut tried_paths = Vec::new();

    for candidate in candidate_player_list_paths() {
        tried_paths.push(candidate.display().to_string());
        match fs::read_to_string(&candidate) {
            Ok(contents) => return Ok((contents, candidate)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                return Err(err).with_context(|| {
                    format!("failed to read {}", candidate.display())
                });
            }
        }
    }

    Err(anyhow!(
        "player_list.json not found; looked in: {}",
        tried_paths.join(", ")
    ))
}

fn candidate_player_list_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(exe_path) = env::current_exe() {
        if let Some(dir) = exe_path.parent() {
            candidates.push(dir.join("player_list.json"));
        }
    }

    if let Ok(cwd) = env::current_dir() {
        candidates.push(cwd.join("player_list.json"));
    }

    if let Some(manifest_dir) = option_env!("CARGO_MANIFEST_DIR") {
        candidates.push(PathBuf::from(manifest_dir).join("player_list.json"));
    }

    candidates
}
