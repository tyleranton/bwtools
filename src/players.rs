use std::collections::BTreeMap;

use anyhow::Result;
use serde::Deserialize;

const PLAYER_LIST_JSON: &str = include_str!("../player_list.json");

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerRecord {
    pub aurora_id: u64,
    pub battle_tag: String,
}

pub type PlayerMap = BTreeMap<String, Vec<PlayerRecord>>;

#[derive(Debug, Clone)]
pub struct PlayerEntry {
    pub name: String,
    #[allow(dead_code)]
    pub aurora_id: u64,
    pub battle_tag: String,
}

pub fn load_player_map() -> Result<PlayerMap> {
    let map: PlayerMap = serde_json::from_str(PLAYER_LIST_JSON)?;
    Ok(map)
}

pub fn flatten_players(map: &PlayerMap) -> Vec<PlayerEntry> {
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
