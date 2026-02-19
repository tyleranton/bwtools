use anyhow::{Context, Result, anyhow};
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

impl OpponentRecord {
    pub fn new(name: impl Into<String>, gateway: u16) -> Self {
        Self {
            name: name.into(),
            gateway,
            ..Self::default()
        }
    }

    pub fn apply_race_observation(&mut self, incoming: &str) {
        if crate::race::should_replace(self.race.as_deref(), incoming) {
            self.race = Some(crate::race::normalize_label(incoming));
        }
    }

    pub fn set_race_if_unknown(&mut self, incoming: Option<&str>) {
        if self.race.is_none()
            && let Some(race) = incoming
        {
            self.race = Some(crate::race::normalize_label(race));
        }
    }
}

pub type OpponentHistory = std::collections::HashMap<String, OpponentRecord>;

pub trait HistorySource {
    fn load(&self) -> Result<OpponentHistory>;
    fn save(&self, hist: &OpponentHistory) -> Result<()>;
}

pub struct FileHistorySource {
    path: std::path::PathBuf,
}

impl FileHistorySource {
    pub fn new(path: std::path::PathBuf) -> Self {
        Self { path }
    }
}

impl HistorySource for FileHistorySource {
    fn load(&self) -> Result<OpponentHistory> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice::<OpponentHistory>(&bytes)
                .with_context(|| format!("deserialize opponent history {}", self.path.display())),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Ok(OpponentHistory::default())
            }
            Err(err) => {
                Err(anyhow!(err).context(format!("read opponent history {}", self.path.display())))
            }
        }
    }

    fn save(&self, hist: &OpponentHistory) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create history directory {}", parent.display()))?;
        }
        let data =
            serde_json::to_vec_pretty(hist).context("serialize opponent history for saving")?;
        std::fs::write(&self.path, data)
            .with_context(|| format!("write opponent history {}", self.path.display()))?;
        Ok(())
    }
}

pub struct HistoryService<S: HistorySource> {
    source: S,
}

impl<S: HistorySource> HistoryService<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }

    pub fn load(&self) -> Result<OpponentHistory> {
        self.source.load()
    }

    pub fn save(&self, hist: &OpponentHistory) -> Result<()> {
        self.source.save(hist)
    }
}

// Derive win/loss vs a specific opponent from a self profile. Also returns the
// latest match timestamp and the opponent race if available.
pub fn derive_wl_and_race(
    profile: &bw_web_api_rs::models::aurora_profile::ScrProfile,
    self_name: &str,
    opp_name: &str,
) -> (u32, u32, Option<u64>, Option<String>) {
    let mut wins: u32 = 0;
    let mut losses: u32 = 0;
    let mut last_ts: u64 = 0;
    let mut last_race: Option<String> = None;

    for g in profile.game_results.iter() {
        let players: Vec<&bw_web_api_rs::models::common::Player> = g
            .players
            .iter()
            .filter(|p| p.attributes.r#type == "player" && !p.toon.trim().is_empty())
            .collect();

        if players.len() != 2 {
            continue;
        }

        let mi = if players[0].toon.eq_ignore_ascii_case(self_name) {
            0
        } else if players[1].toon.eq_ignore_ascii_case(self_name) {
            1
        } else {
            continue;
        };

        let oi = 1 - mi;
        if !players[oi].toon.eq_ignore_ascii_case(opp_name) {
            continue;
        }

        let ts = g.create_time.parse::<u64>().unwrap_or(0);
        let res = players[mi].result.to_ascii_lowercase();
        if res == "win" {
            wins = wins.saturating_add(1);
        }
        if res == "loss" {
            losses = losses.saturating_add(1);
        }

        if ts > last_ts {
            last_ts = ts;
            last_race = players[oi].attributes.race.clone();
        }
    }

    let last_ts_opt = if last_ts > 0 { Some(last_ts) } else { None };
    (wins, losses, last_ts_opt, last_race)
}
