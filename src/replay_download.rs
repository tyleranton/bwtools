use std::collections::HashMap;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::api::ApiHandle;
use crate::config::Config;

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
        self.errors.push(err.to_string());
    }
}

pub fn spawn_download_job(
    base_url: String,
    cfg: Config,
    request: ReplayDownloadRequest,
) -> (thread::JoinHandle<()>, Receiver<ReplayDownloadSummary>) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let summary = match ApiHandle::new(base_url) {
            Ok(api) => {
                let storage = ReplayStorage::new(cfg.replay_library_root.clone());
                match download_replays(&api, &cfg, &storage, request) {
                    Ok(summary) => summary,
                    Err(err) => {
                        let mut summary = ReplayDownloadSummary::default();
                        summary.record_error(err);
                        summary
                    }
                }
            }
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

pub fn download_replays(
    api: &ApiHandle,
    cfg: &Config,
    storage: &ReplayStorage,
    request: ReplayDownloadRequest,
) -> Result<ReplayDownloadSummary> {
    storage
        .ensure_base_dirs()
        .context("failed to ensure replay directories")?;

    let mut manifest = ReplayManifest::load(&storage.manifest_path());
    let mut summary = ReplayDownloadSummary::default();

    let profile = api
        .get_scr_profile(&request.toon, request.gateway)
        .with_context(|| format!("failed to load profile for {}", request.toon))?;

    let mut candidates: Vec<_> = profile.replays.into_iter().collect();
    // Sort newest first
    candidates.sort_by(|a, b| b.create_time.cmp(&a.create_time));

    let matchup_filter = request.matchup.as_deref().and_then(parse_matchup_filter);
    let filtered = candidates
        .into_iter()
        .filter(|replay| match &matchup_filter {
            Some((a, b)) => replay_matches(&replay.attributes.replay_player_races, (*a, *b)),
            None => true,
        })
        .take(request.limit.min(20))
        .collect::<Vec<_>>();

    summary.requested = filtered.len();
    if summary.requested == 0 {
        return Ok(summary);
    }

    let client = Client::builder()
        .build()
        .context("failed to create http client")?;
    let manifest_path = storage.manifest_path();
    let sanitized_profile = sanitize_component(&request.toon);
    let matchup_label = request
        .matchup
        .as_deref()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "All".to_string());
    let sanitized_matchup = sanitize_component(&matchup_label);
    storage
        .ensure_matchup_dir(&sanitized_profile, &sanitized_matchup)
        .with_context(|| format!("failed to prepare replay directory for {}", request.toon))?;

    for replay in filtered {
        summary.attempted += 1;
        match process_replay(
            api,
            cfg,
            &client,
            storage,
            &mut manifest,
            &request,
            &sanitized_profile,
            &sanitized_matchup,
            &replay,
        ) {
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

    manifest
        .save(&manifest_path)
        .context("failed to write replay manifest")?;

    Ok(summary)
}

enum ReplayProcessError {
    AlreadyExists,
    Other(anyhow::Error),
}

#[allow(clippy::too_many_arguments)]
fn process_replay(
    api: &ApiHandle,
    cfg: &Config,
    client: &Client,
    storage: &ReplayStorage,
    manifest: &mut ReplayManifest,
    request: &ReplayDownloadRequest,
    profile_dir: &str,
    matchup_dir: &str,
    replay: &bw_web_api_rs::models::common::Replay,
) -> Result<Option<PathBuf>, ReplayProcessError> {
    let detail = api
        .get_matchmaker_player_info(&replay.link)
        .map_err(|e| ReplayProcessError::Other(e.context("failed matchmaker detail")))?;

    let best = detail
        .replays
        .into_iter()
        .max_by(|a, b| a.create_time.cmp(&b.create_time))
        .ok_or_else(|| ReplayProcessError::Other(anyhow!("no replay URLs in matchmaker detail")))?;

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

    let target_dir = storage
        .ensure_matchup_dir(profile_dir, matchup_dir)
        .map_err(|e| ReplayProcessError::Other(e.into()))?;

    let tmp_path = target_dir.join(format!(".tmp-{}.rep", truncate_identifier(&identifier)));
    if let Err(err) = download_to_path(client, &best.url, &tmp_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(ReplayProcessError::Other(err));
    }

    let overview = match run_screp_overview(cfg, &tmp_path) {
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

    let (main_name, main_race, opp_name, opp_race) = extract_players(&overview, &request.toon)
        .ok_or_else(|| ReplayProcessError::Other(anyhow!("failed to parse players from screp")))?;

    let file_name = build_filename(&main_name, &main_race, &opp_name, &opp_race);
    let mut final_path = target_dir.join(&file_name);
    let mut counter = 1;
    while final_path.exists() {
        let alt = format!("{}-{}.rep", file_name.trim_end_matches(".rep"), counter);
        final_path = target_dir.join(alt);
        counter += 1;
    }

    fs::rename(&tmp_path, &final_path).map_err(|e| ReplayProcessError::Other(anyhow!(e)))?;

    manifest.entries.insert(
        identifier,
        ManifestEntry {
            path: final_path.to_string_lossy().into_owned(),
            saved_at: current_timestamp(),
        },
    );

    Ok(Some(final_path))
}

fn download_to_path(client: &Client, url: &str, path: &Path) -> Result<()> {
    let mut response = client
        .get(url)
        .send()
        .with_context(|| format!("failed to download replay {}", url))?;
    if !response.status().is_success() {
        return Err(anyhow!("http status {}", response.status()));
    }
    let mut file = File::create(path).with_context(|| format!("create file {:?}", path))?;
    io::copy(&mut response, &mut file).with_context(|| format!("write replay to {:?}", path))?;
    Ok(())
}

fn run_screp_overview(cfg: &Config, path: &Path) -> Result<String> {
    let output = Command::new(&cfg.screp_cmd)
        .arg("-overview")
        .arg(path)
        .output()
        .with_context(|| format!("failed to run screp on {:?}", path))?;
    if !output.status.success() {
        return Err(anyhow!(
            "screp exited with status {}",
            output.status.code().unwrap_or(-1)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn extract_players(overview: &str, target_toon: &str) -> Option<(String, String, String, String)> {
    let (_, players) = crate::replay::parse_screp_overview(overview);
    if players.is_empty() {
        return None;
    }
    let target_lower = target_toon.to_ascii_lowercase();
    let mut seen = std::collections::HashSet::new();
    let mut ordered: Vec<(String, String)> = Vec::new();

    for (_team, race, name) in players {
        let key = name.trim();
        if key.is_empty() {
            continue;
        }
        let lower = key.to_ascii_lowercase();
        if !seen.insert(lower.clone()) {
            continue;
        }
        let race_label = race.unwrap_or_else(|| "Unknown".to_string());
        ordered.push((key.to_string(), race_label));
    }

    if ordered.is_empty() {
        return None;
    }

    let mut main: Option<(String, String)> = None;
    let mut opp: Option<(String, String)> = None;

    for (name, race) in ordered.iter() {
        if name.to_ascii_lowercase() == target_lower {
            main = Some((name.clone(), race.clone()));
        } else if opp.is_none() {
            opp = Some((name.clone(), race.clone()));
        }
    }

    if let Some(main) = main {
        let opponent = opp.unwrap_or_else(|| ("Opponent".to_string(), "Unknown".to_string()));
        Some((main.0, main.1, opponent.0, opponent.1))
    } else if ordered.len() >= 2 {
        Some((
            ordered[0].0.clone(),
            ordered[0].1.clone(),
            ordered[1].0.clone(),
            ordered[1].1.clone(),
        ))
    } else {
        None
    }
}

fn build_filename(p1: &str, r1: &str, p2: &str, r2: &str) -> String {
    format!(
        "{}({})_vs_{}({}).rep",
        sanitize_component(p1),
        sanitize_component(&capitalize_race(r1)),
        sanitize_component(p2),
        sanitize_component(&capitalize_race(r2))
    )
}

fn capitalize_race(race: &str) -> String {
    let lower = race.trim();
    if lower.is_empty() {
        return "Unknown".to_string();
    }
    let mut chars = lower.chars();
    match chars.next() {
        Some(first) => format!(
            "{}{}",
            first.to_ascii_uppercase(),
            chars.as_str().to_ascii_lowercase()
        ),
        None => "Unknown".to_string(),
    }
}

fn sanitize_component(input: &str) -> String {
    let trimmed = input.trim();
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || ch.is_control() {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    let cleaned = out.trim_matches('.').trim();
    if cleaned.is_empty() {
        "Unknown".to_string()
    } else {
        cleaned.to_string()
    }
}

fn parse_matchup_filter(input: &str) -> Option<(char, char)> {
    let s = input.trim().to_ascii_uppercase();
    if let Some((left, right)) = s.split_once('v') {
        let a = left.chars().find(|c| c.is_ascii_alphabetic())?;
        let b = right.chars().find(|c| c.is_ascii_alphabetic())?;
        return Some((a, b));
    }
    if let Some((a, b)) = s.split_once(',') {
        let ac = a.chars().next()?;
        let bc = b.chars().next()?;
        return Some((ac, bc));
    }
    None
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
