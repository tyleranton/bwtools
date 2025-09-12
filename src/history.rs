use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpponentRecord {
    pub name: String,
    pub gateway: u16,
    pub race: Option<String>,
    pub current_rating: Option<u32>,
    pub previous_rating: Option<u32>,
    pub wins: u32,
    pub losses: u32,
    pub last_match_ts: Option<u64>,
}

pub type OpponentHistory = std::collections::HashMap<String, OpponentRecord>;

pub fn load_history(path: &std::path::Path) -> OpponentHistory {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<OpponentHistory>(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_history(path: &std::path::Path, hist: &OpponentHistory) {
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    if let Ok(data) = serde_json::to_vec_pretty(hist) {
        let _ = std::fs::write(path, data);
    }
}
