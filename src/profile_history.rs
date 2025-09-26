use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

const MAX_STORED_MATCHES: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchOutcome {
    Win,
    Loss,
    SelfDodged,
    OpponentDodged,
}

impl MatchOutcome {
    pub fn is_win(self) -> bool {
        matches!(self, MatchOutcome::Win)
    }

    pub fn is_self_dodged(self) -> bool {
        matches!(self, MatchOutcome::SelfDodged)
    }

    pub fn is_opponent_dodged(self) -> bool {
        matches!(self, MatchOutcome::OpponentDodged)
    }

    pub fn counts_for_record(self) -> bool {
        matches!(self, MatchOutcome::Win | MatchOutcome::Loss)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMatch {
    pub timestamp: u64,
    pub opponent: String,
    pub opponent_race: Option<String>,
    pub main_race: Option<String>,
    pub result: MatchOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProfileHistoryKey {
    name: String,
    gateway: u16,
}

impl ProfileHistoryKey {
    pub fn new(name: &str, gateway: u16) -> Self {
        Self {
            name: name.to_ascii_lowercase(),
            gateway,
        }
    }

    pub fn storage_key(&self) -> String {
        format!("{}#{}", self.name, self.gateway)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ProfileHistoryData {
    profiles: HashMap<String, Vec<StoredMatch>>,
}

pub struct ProfileHistoryService {
    path: PathBuf,
    data: ProfileHistoryData,
}

impl ProfileHistoryService {
    pub fn new(path: PathBuf) -> Result<Self> {
        let data = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<ProfileHistoryData>(&bytes)
                .with_context(|| format!("deserialize profile history {}", path.display()))?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => ProfileHistoryData::default(),
            Err(err) => {
                return Err(
                    anyhow!(err).context(format!("read profile history {}", path.display()))
                );
            }
        };
        Ok(Self { path, data })
    }

    pub fn empty(path: PathBuf) -> Self {
        Self {
            path,
            data: ProfileHistoryData::default(),
        }
    }

    pub fn merge_matches(
        &mut self,
        key: &ProfileHistoryKey,
        mut incoming: Vec<StoredMatch>,
    ) -> Result<Vec<StoredMatch>> {
        incoming.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        let storage_key = key.storage_key();
        let mut changed = false;

        {
            let entry = self
                .data
                .profiles
                .entry(storage_key.clone())
                .or_insert_with(Vec::new);

            for m in incoming.into_iter() {
                if m.timestamp == 0 {
                    continue;
                }
                if let Some(existing) = entry.iter_mut().find(|existing| {
                    existing.timestamp == m.timestamp
                        && existing.opponent.eq_ignore_ascii_case(m.opponent.as_str())
                }) {
                    if existing.main_race.is_none() && m.main_race.is_some() {
                        existing.main_race = m.main_race.clone();
                        changed = true;
                    }
                    if existing.opponent_race.is_none() && m.opponent_race.is_some() {
                        existing.opponent_race = m.opponent_race.clone();
                        changed = true;
                    }
                    continue;
                }
                entry.push(m);
                changed = true;
            }

            if changed {
                entry.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
                if entry.len() > MAX_STORED_MATCHES {
                    entry.truncate(MAX_STORED_MATCHES);
                }
            }
        }

        if changed {
            self.save()?;
        }

        let output = self
            .data
            .profiles
            .get(&storage_key)
            .map(|entry| {
                let limit = entry.len().min(100);
                entry.iter().take(limit).cloned().collect()
            })
            .unwrap_or_default();
        Ok(output)
    }

    fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create profile history directory {}", parent.display())
            })?;
        }
        let data = serde_json::to_vec_pretty(&self.data).context("serialize profile history")?;
        fs::write(&self.path, data)
            .with_context(|| format!("write profile history {}", self.path.display()))?;
        Ok(())
    }
}

impl ProfileHistoryService {
    pub fn upsert_match(&mut self, key: &ProfileHistoryKey, new_match: StoredMatch) -> Result<()> {
        let storage_key = key.storage_key();
        let entry = self
            .data
            .profiles
            .entry(storage_key.clone())
            .or_insert_with(Vec::new);

        let mut changed = false;
        if let Some(existing) = entry.iter_mut().find(|existing| {
            existing.timestamp == new_match.timestamp
                && existing
                    .opponent
                    .eq_ignore_ascii_case(new_match.opponent.as_str())
        }) {
            if existing.result != new_match.result
                || existing.main_race != new_match.main_race
                || existing.opponent_race != new_match.opponent_race
                || existing.opponent != new_match.opponent
            {
                *existing = new_match;
                changed = true;
            }
        } else {
            entry.push(new_match);
            changed = true;
        }

        if changed {
            entry.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
            if entry.len() > MAX_STORED_MATCHES {
                entry.truncate(MAX_STORED_MATCHES);
            }
            self.save()?;
        }

        Ok(())
    }

    pub fn has_matches(&self, key: &ProfileHistoryKey) -> bool {
        let storage_key = key.storage_key();
        self.data
            .profiles
            .get(&storage_key)
            .map(|entry| !entry.is_empty())
            .unwrap_or(false)
    }
}
