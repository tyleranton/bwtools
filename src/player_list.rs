use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct PlayerListEntry {
    aurora_id: u32,
    #[serde(default, rename = "battle_tag")]
    _battle_tag: String,
}

pub fn load_known_players(path: &Path) -> Result<HashMap<u32, String>> {
    let data =
        std::fs::read(path).with_context(|| format!("read player list {}", path.display()))?;
    let raw: HashMap<String, Vec<PlayerListEntry>> =
        serde_json::from_slice(&data).context("parse player list json")?;

    let mut out: HashMap<u32, String> = HashMap::new();
    for (known_name, entries) in raw {
        for entry in entries {
            if entry.aurora_id == 0 {
                continue;
            }
            if let Some(existing) = out.get(&entry.aurora_id) {
                if existing != &known_name {
                    tracing::warn!(
                        aurora_id = entry.aurora_id,
                        existing = %existing,
                        incoming = %known_name,
                        "duplicate aurora_id in player list"
                    );
                }
                continue;
            }
            out.insert(entry.aurora_id, known_name.clone());
        }
    }
    Ok(out)
}

pub fn display_name_for_opponent(
    known_players: &HashMap<u32, String>,
    aurora_id: Option<u32>,
    toon: &str,
) -> String {
    if let Some(id) = aurora_id
        && let Some(known) = known_players.get(&id)
    {
        return format!("{} ({})", known, toon);
    }
    toon.to_string()
}
