use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::api::ApiHandle;
use crate::config::Config;
use crate::error::render_error_message;
use crate::replay_io::{download_replay, run_screp_overview, sanitize_component};

pub struct ReplayStorage {
    root: PathBuf,
}

impl ReplayStorage {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn bwtools_root(&self) -> PathBuf {
        self.root.join("bwtools")
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.bwtools_root().join(".meta").join("manifest.json")
    }

    pub fn profile_dir(&self, profile: &str) -> PathBuf {
        self.bwtools_root().join(profile)
    }

    pub fn matchup_dir(&self, profile: &str, matchup: &str) -> PathBuf {
        self.profile_dir(profile).join(matchup)
    }

    pub fn ensure_base_dirs(&self) -> io::Result<()> {
        fs::create_dir_all(self.bwtools_root())?;
        let meta_dir = self.bwtools_root().join(".meta");
        fs::create_dir_all(meta_dir)
    }

    pub fn ensure_matchup_dir(&self, profile: &str, matchup: &str) -> io::Result<PathBuf> {
        let dir = self.matchup_dir(profile, matchup);
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}

#[derive(Debug, Clone)]
pub struct ReplayDownloadRequest {
    pub toon: String,
    pub gateway: u16,
    pub matchup: Option<String>,
    pub limit: usize,
    pub alias: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ReplayManifest {
    pub entries: HashMap<String, ManifestEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub path: String,
    pub saved_at: u64,
}

impl ReplayManifest {
    pub fn load(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<ReplayManifest>(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        fs::write(path, data)
    }
}

#[derive(Debug, Default)]
pub struct ReplayDownloadSummary {
    pub requested: usize,
    pub attempted: usize,
    pub saved: usize,
    pub skipped_existing: usize,
    pub filtered_short: usize,
    pub errors: Vec<String>,
    pub saved_paths: Vec<PathBuf>,
}

impl ReplayDownloadSummary {
    fn record_error(&mut self, err: anyhow::Error) {
        self.errors.push(render_error_message(&err));
    }
}

pub struct ReplayDownloadJob {
    api: ApiHandle,
    cfg: Config,
    storage: ReplayStorage,
    request: ReplayDownloadRequest,
}

impl ReplayDownloadJob {
    pub fn new(base_url: String, cfg: Config, request: ReplayDownloadRequest) -> Result<Self> {
        let api = ApiHandle::new(base_url)?;
        let storage = ReplayStorage::new(cfg.replay_library_root.clone());
        Ok(Self {
            api,
            cfg,
            storage,
            request,
        })
    }

    pub fn run(self) -> ReplayDownloadSummary {
        let mut summary = ReplayDownloadSummary::default();
        if let Err(err) = self.storage.ensure_base_dirs() {
            summary.record_error(anyhow!(err).context("failed to ensure replay directories"));
            return summary;
        }

        let manifest_path = self.storage.manifest_path();
        let mut manifest = ReplayManifest::load(&manifest_path);

        let profile = match self.load_profile() {
            Ok(profile) => profile,
            Err(err) => {
                summary.record_error(err);
                return summary;
            }
        };

        let filtered = self.filtered_replays(profile);

        summary.requested = filtered.len();
        if summary.requested == 0 {
            return summary;
        }

        let ctx = match self.prepare_context() {
            Ok(ctx) => ctx,
            Err(err) => {
                summary.record_error(err);
                return summary;
            }
        };

        for replay in filtered {
            summary.attempted += 1;
            match self.process_replay(&ctx, &mut manifest, &replay) {
                Ok(Some(path)) => {
                    summary.saved += 1;
                    summary.saved_paths.push(path);
                }
                Ok(None) => {
                    summary.filtered_short += 1;
                }
                Err(ReplayProcessError::AlreadyExists) => {
                    summary.skipped_existing += 1;
                }
                Err(ReplayProcessError::Other(err)) => {
                    summary.record_error(err);
                }
            }
        }

        if let Err(err) = manifest.save(&manifest_path) {
            summary.record_error(anyhow!(err).context("failed to write replay manifest"));
        }

        summary
    }

    fn load_profile(&self) -> Result<bw_web_api_rs::models::aurora_profile::ScrProfile> {
        self.api
            .get_scr_profile(&self.request.toon, self.request.gateway)
            .with_context(|| format!("failed to load profile for {}", self.request.toon))
    }

    fn filtered_replays(
        &self,
        profile: bw_web_api_rs::models::aurora_profile::ScrProfile,
    ) -> Vec<bw_web_api_rs::models::common::Replay> {
        let mut candidates: Vec<_> = profile.replays.into_iter().collect();
        candidates.sort_by(|a, b| b.create_time.cmp(&a.create_time));

        let matchup_filter = self
            .request
            .matchup
            .as_deref()
            .and_then(parse_matchup_filter);

        candidates
            .into_iter()
            .filter(|replay| match &matchup_filter {
                Some((a, b)) => replay_matches(&replay.attributes.replay_player_races, (*a, *b)),
                None => true,
            })
            .take(self.request.limit.min(20))
            .collect()
    }

    fn prepare_context(&self) -> Result<DownloadContext> {
        let client = Client::builder()
            .build()
            .context("failed to create http client")?;

        let storage_profile = self
            .request
            .alias
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&self.request.toon);
        let sanitized_profile = sanitize_component(storage_profile);
        let matchup_label = self
            .request
            .matchup
            .as_deref()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "All".to_string());
        let sanitized_matchup = sanitize_component(&matchup_label);

        let target_dir = self
            .storage
            .ensure_matchup_dir(&sanitized_profile, &sanitized_matchup)
            .map_err(|e| anyhow!(e))
            .with_context(|| {
                format!("failed to prepare replay directory for {}", storage_profile)
            })?;

        Ok(DownloadContext { client, target_dir })
    }

    fn process_replay(
        &self,
        ctx: &DownloadContext,
        manifest: &mut ReplayManifest,
        replay: &bw_web_api_rs::models::common::Replay,
    ) -> Result<Option<PathBuf>, ReplayProcessError> {
        let detail = self
            .api
            .get_matchmaker_player_info(&replay.link)
            .map_err(|e| ReplayProcessError::Other(e.context("failed matchmaker detail")))?;

        let best = detail
            .replays
            .into_iter()
            .max_by(|a, b| a.create_time.cmp(&b.create_time))
            .ok_or_else(|| {
                ReplayProcessError::Other(anyhow!("no replay URLs in matchmaker detail"))
            })?;

        let identifier = if !best.md5.is_empty() {
            best.md5.clone()
        } else if !replay.attributes.game_id.is_empty() {
            replay.attributes.game_id.clone()
        } else {
            replay.link.clone()
        };

        if manifest.entries.contains_key(&identifier) {
            return Err(ReplayProcessError::AlreadyExists);
        }

        if best.url.trim().is_empty() {
            return Err(ReplayProcessError::Other(anyhow!("empty replay url")));
        }

        let tmp_path = ctx
            .target_dir
            .join(format!(".tmp-{}.rep", truncate_identifier(&identifier)));
        if let Err(err) = download_replay(&ctx.client, &best.url, &tmp_path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(ReplayProcessError::Other(err));
        }

        let overview = match run_screp_overview(&self.cfg, &tmp_path) {
            Ok(text) => text,
            Err(err) => {
                let _ = fs::remove_file(&tmp_path);
                return Err(ReplayProcessError::Other(err));
            }
        };

        if let Some(duration) = crate::replay::parse_screp_duration_seconds(&overview)
            && duration <= 120
        {
            let _ = fs::remove_file(&tmp_path);
            return Ok(None);
        }

        let (main_name, main_race, opp_name, opp_race) =
            extract_players(&overview, &self.request.toon).ok_or_else(|| {
                ReplayProcessError::Other(anyhow!("failed to parse players from screp"))
            })?;

        let date_prefix = replay_date_prefix(best.create_time)
            .or_else(|| replay_date_prefix(replay.create_time as u64));
        let file_name = build_filename(
            date_prefix.as_deref(),
            &main_name,
            &main_race,
            &opp_name,
            &opp_race,
        );
        let mut final_path = ctx.target_dir.join(&file_name);
        let mut counter = 1;
        while final_path.exists() {
            let alt = format!("{}-{}.rep", file_name.trim_end_matches(".rep"), counter);
            final_path = ctx.target_dir.join(alt);
            counter += 1;
        }

        fs::rename(&tmp_path, &final_path)
            .map_err(|e| ReplayProcessError::Other(anyhow!(e).context("finalize replay")))?;

        manifest.entries.insert(
            identifier,
            ManifestEntry {
                path: final_path.to_string_lossy().into_owned(),
                saved_at: current_timestamp(),
            },
        );

        Ok(Some(final_path))
    }
}

struct DownloadContext {
    client: Client,
    target_dir: PathBuf,
}

pub fn spawn_download_job(
    base_url: String,
    cfg: Config,
    request: ReplayDownloadRequest,
) -> (thread::JoinHandle<()>, Receiver<ReplayDownloadSummary>) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let summary = match ReplayDownloadJob::new(base_url, cfg, request) {
            Ok(job) => job.run(),
            Err(err) => {
                let mut summary = ReplayDownloadSummary::default();
                summary.record_error(err);
                summary
            }
        };
        let _ = tx.send(summary);
    });
    (handle, rx)
}

#[derive(Debug, Error)]
pub enum ReplayProcessError {
    #[error("replay already downloaded")]
    AlreadyExists,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

fn extract_players(overview: &str, target_toon: &str) -> Option<(String, String, String, String)> {
    let (_, players) = crate::replay::parse_screp_overview(overview);
    if players.is_empty() {
        return None;
    }
    let target_lower = target_toon.to_ascii_lowercase();
    let mut seen = std::collections::HashSet::new();
    let mut ordered: Vec<(u8, String, String)> = Vec::new();

    for (team, race, name) in players {
        let key = name.trim();
        if key.is_empty() {
            continue;
        }
        let lower = key.to_ascii_lowercase();
        if !seen.insert(lower.clone()) {
            continue;
        }
        let race_label = race.unwrap_or_else(|| "Unknown".to_string());
        ordered.push((team, race_label, key.to_string()));
    }

    if ordered.is_empty() {
        return None;
    }

    let mut main_entry: Option<(u8, String, String)> = None;
    let mut others: Vec<(u8, String, String)> = Vec::new();

    for (team, race, name) in ordered.into_iter() {
        if name.to_ascii_lowercase() == target_lower {
            if main_entry.is_none() {
                main_entry = Some((team, race.clone(), name.clone()));
            }
        } else {
            others.push((team, race, name));
        }
    }

    let (main_team, main_race, main_name) = main_entry?;
    if main_team == 0 || main_team > 2 {
        // Target toon was only observing.
        return None;
    }

    let mut opponent: Option<(u8, String, String)> = None;
    for (team, race, name) in others.into_iter() {
        if team == main_team {
            continue;
        }
        if team > 0 && team <= 2 {
            opponent = Some((team, race.clone(), name.clone()));
            break;
        }
        if opponent.is_none() {
            opponent = Some((team, race, name));
        }
    }

    let (_, opp_race, opp_name) = opponent?;
    Some((main_name, main_race, opp_name, opp_race))
}

fn build_filename(prefix: Option<&str>, p1: &str, r1: &str, p2: &str, r2: &str) -> String {
    let base = format!(
        "{}({})_vs_{}({})",
        sanitize_component(p1),
        sanitize_component(&race_letter(r1)),
        sanitize_component(p2),
        sanitize_component(&race_letter(r2))
    );
    match prefix {
        Some(p) => format!("{}_{}.rep", sanitize_component(p), base),
        None => format!("{}.rep", base),
    }
}

fn race_letter(race: &str) -> String {
    let trimmed = race.trim();
    let Some(first) = trimmed.chars().next() else {
        return "U".to_string();
    };
    first.to_ascii_uppercase().to_string()
}

fn parse_matchup_filter(input: &str) -> Option<(char, char)> {
    let s = input.trim().to_ascii_uppercase();
    let splitters = ['V', ',', '/'];
    for sep in splitters {
        if let Some((left, right)) = s.split_once(sep) {
            let a = left.chars().find(|c| c.is_ascii_alphabetic())?;
            let b = right.chars().find(|c| c.is_ascii_alphabetic())?;
            return Some((a, b));
        }
    }
    let mut letters = s.chars().filter(|c| c.is_ascii_alphabetic());
    let a = letters.next()?;
    let b = letters.next()?;
    Some((a, b))
}

fn replay_matches(races: &str, filter: (char, char)) -> bool {
    let parts: Vec<char> = races
        .split(',')
        .filter_map(|s| s.chars().next())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if parts.len() < 2 {
        return false;
    }
    (parts[0] == filter.0 && parts[1] == filter.1) || (parts[0] == filter.1 && parts[1] == filter.0)
}

fn truncate_identifier(id: &str) -> String {
    let short = if id.len() > 16 { &id[..16] } else { id };
    sanitize_component(short)
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

fn replay_date_prefix(ts_secs: u64) -> Option<String> {
    if ts_secs == 0 || ts_secs == u32::MAX as u64 {
        return None;
    }
    DateTime::<Utc>::from_timestamp(ts_secs as i64, 0).map(|dt| dt.format("%Y%m%d").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_matchup_filter_accepts_common_formats() {
        assert_eq!(parse_matchup_filter("PvT"), Some(('P', 'T')));
        assert_eq!(parse_matchup_filter("p,t"), Some(('P', 'T')));
        assert_eq!(parse_matchup_filter("z/p"), Some(('Z', 'P')));
        assert_eq!(parse_matchup_filter(" PT "), Some(('P', 'T')));
    }

    #[test]
    fn parse_matchup_filter_rejects_invalid_input() {
        assert_eq!(parse_matchup_filter(""), None);
        assert_eq!(parse_matchup_filter("P"), None);
        assert_eq!(parse_matchup_filter("123"), None);
    }

    #[test]
    fn replay_matches_handles_order_independently() {
        assert!(replay_matches("P,T", ('P', 'T')));
        assert!(replay_matches("T,P", ('P', 'T')));
        assert!(!replay_matches("P,Z", ('P', 'T')));
        assert!(!replay_matches("P", ('P', 'T')));
    }

    #[test]
    fn race_letter_maps_first_character_or_unknown() {
        assert_eq!(race_letter("Protoss"), "P");
        assert_eq!(race_letter("zerg"), "Z");
        assert_eq!(race_letter(""), "U");
    }

    #[test]
    fn replay_date_prefix_filters_sentinel_and_formats_valid_epoch() {
        assert_eq!(replay_date_prefix(0), None);
        assert_eq!(replay_date_prefix(u32::MAX as u64), None);
        assert_eq!(
            replay_date_prefix(1_704_067_200),
            Some("20240101".to_string())
        );
    }

    #[test]
    fn truncate_identifier_limits_to_sixteen_chars() {
        assert_eq!(truncate_identifier("abcdefghijklmnop"), "abcdefghijklmnop");
        assert_eq!(truncate_identifier("abcdefghijklmnopq"), "abcdefghijklmnop");
    }
}
