use std::collections::BTreeMap;

use anyhow::Result;
use serde::Deserialize;

const PLAYER_LIST_JSON: &str = include_str!("../player_list.json");

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
        let map: PlayerMap = serde_json::from_str(PLAYER_LIST_JSON)?;
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
