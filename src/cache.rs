use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use chrome_cache_parser::block_file::{BlockFileCacheEntry, LazyBlockFileCacheEntry};
use chrome_cache_parser::ChromeCache;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use percent_encoding::percent_decode_str;
use url::Url;

fn extract_port(url: &str) -> Option<u16> {
    Url::parse(url).ok().and_then(|parsed| parsed.port())
}

fn decode_segment(seg: &str) -> String {
    percent_decode_str(seg).decode_utf8_lossy().into_owned()
}

fn parse_profile_from_url_mmgameloading(url: &str) -> Option<(String, u16)> {
    let parsed = Url::parse(url).ok()?;
    let has_flag = parsed
        .query_pairs()
        .any(|(k, v)| k == "request_flags" && v.as_ref().contains("scr_mmgameloading"));
    if !has_flag {
        return None;
    }

    let mut segments = parsed.path_segments()?;
    let s1 = segments.next()?;
    let s2 = segments.next()?;
    let s3 = segments.next()?;
    if s1 != "web-api" || s2 != "v2" || s3 != "aurora-profile-by-toon" {
        return None;
    }
    let profile = decode_segment(segments.next()?);
    let gateway_str = segments.next()?;
    let gateway: u16 = gateway_str.parse().ok()?;
    Some((profile, gateway))
}

fn parse_profile_from_url_path(url: &str) -> Option<(String, u16)> {
    let parsed = Url::parse(url).ok()?;
    let mut segments = parsed.path_segments()?;
    let s1 = segments.next()?;
    let s2 = segments.next()?;
    let s3 = segments.next()?;
    if s1 != "web-api" || s2 != "v2" || s3 != "aurora-profile-by-toon" {
        return None;
    }
    let profile = decode_segment(segments.next()?);
    let gw: u16 = segments.next()?.parse().ok()?;
    Some((profile, gw))
}

pub struct CacheReader {
    cache_dir: PathBuf,
    cache: ChromeCache,
}

impl CacheReader {
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        let cache = Self::load_cache(&cache_dir, "open")?;
        Ok(Self { cache_dir, cache })
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.cache = Self::load_cache(&self.cache_dir, "refresh")?;
        Ok(())
    }

    fn load_cache(cache_dir: &Path, action: &str) -> Result<ChromeCache> {
        ChromeCache::from_path(cache_dir.to_path_buf()).map_err(|err| {
            anyhow!(
                "Failed to {} Chrome cache at {}: {}",
                action,
                cache_dir.display(),
                err
            )
        })
    }

    pub fn parse_for_port(&mut self, window_secs: i64) -> Result<Option<u16>> {
        let now = Utc::now();
        let entries = self
            .cache
            .entries()
            .context("Failed to read cache entries")?;
        let latest = entries
            .filter_map(|entry| {
                let entry_ref = entry.get().ok()?;
                let key = entry_ref.key.to_string();
                if !key.contains("/web-api/") {
                    return None;
                }
                let creation_time: DateTime<Utc> =
                    entry_ref.creation_time.into_datetime_utc().ok()?;
                if (now - creation_time) >= ChronoDuration::seconds(window_secs) {
                    return None;
                }
                let port = extract_port(&key)?;
                Some((port, creation_time))
            })
            .max_by_key(|(_, ct)| *ct)
            .map(|(p, _)| p);
        Ok(latest)
    }

    pub fn latest_opponent_profile(
        &mut self,
        exclude_name: Option<&str>,
        window_secs: i64,
    ) -> Result<Option<(String, u16, DateTime<Utc>)>> {
        let now = Utc::now();
        let entries = self
            .cache
            .entries()
            .context("Failed to read cache entries")?;
        let latest = entries
            .filter_map(|mut entry| {
                let (key, creation_time) = {
                    let entry_ref = entry.get().ok()?;
                    let key = entry_ref.key.to_string();
                    if !(key.contains("/web-api/v2/aurora-profile-by-toon/")
                        && key.contains("scr_mmgameloading"))
                    {
                        return None;
                    }
                    let creation_time = entry_creation_time(entry_ref);
                    (key, creation_time)
                };
                let last_used = entry_last_used(&mut entry).or(creation_time)?;
                if (now - last_used) >= ChronoDuration::seconds(window_secs) {
                    return None;
                }
                let (profile, gateway) = parse_profile_from_url_mmgameloading(&key)?;
                if let Some(ex) = exclude_name
                    && profile.eq_ignore_ascii_case(ex)
                {
                    return None;
                }
                Some(((profile, gateway), last_used))
            })
            .max_by_key(|(_, ct)| *ct)
            .map(|(data, observed_at)| (data.0, data.1, observed_at));
        Ok(latest)
    }

    pub fn latest_mmgameloading_profile(
        &mut self,
        window_secs: i64,
    ) -> Result<Option<(String, u16)>> {
        let now = Utc::now();
        let entries = self
            .cache
            .entries()
            .context("Failed to read cache entries")?;
        let latest = entries
            .filter_map(|entry| {
                let entry_ref = entry.get().ok()?;
                let key = entry_ref.key.to_string();
                if !(key.contains("/web-api/v2/aurora-profile-by-toon/")
                    && key.contains("scr_mmgameloading"))
                {
                    return None;
                }
                let creation_time = entry_creation_time(entry_ref)?;
                if (now - creation_time) >= ChronoDuration::seconds(window_secs) {
                    return None;
                }
                let (profile, gateway) = parse_profile_from_url_mmgameloading(&key)?;
                Some(((profile, gateway), creation_time))
            })
            .max_by_key(|(_, ct)| *ct)
            .map(|(data, _)| data);
        Ok(latest)
    }

    pub fn latest_self_profile(&mut self, window_secs: i64) -> Result<Option<(String, u16)>> {
        let now = Utc::now();
        let entries = self
            .cache
            .entries()
            .context("Failed to read cache entries")?;
        let latest = entries
            .filter_map(|entry| {
                let entry_ref = entry.get().ok()?;
                let key = entry_ref.key.to_string();
                if !(key.contains("/web-api/v2/aurora-profile-by-toon/")
                    && key.contains("scr_tooninfo"))
                {
                    return None;
                }
                let creation_time = entry_creation_time(entry_ref)?;
                if (now - creation_time) >= ChronoDuration::seconds(window_secs) {
                    return None;
                }
                let (profile, gw) = parse_profile_from_url_path(&key)?;
                Some(((profile, gw), creation_time))
            })
            .max_by_key(|(_, creation_time)| *creation_time)
            .map(|(data, _)| data);
        Ok(latest)
    }

    pub fn recent_keys(&mut self, window_secs: i64, max: usize) -> Result<Vec<(String, i64)>> {
        let now = Utc::now();
        let entries = self
            .cache
            .entries()
            .context("Failed to read cache entries")?;
        let mut items: Vec<(String, i64, DateTime<Utc>)> = entries
            .filter_map(|mut entry| {
                let key = entry.get().ok()?.key.to_string();
                if !key.contains("/web-api/") {
                    return None;
                }
                let last_used = entry_last_used(&mut entry)
                    .or_else(|| entry_creation_time(entry.get().ok()?))?;
                let age = (now - last_used).num_seconds();
                if age <= window_secs {
                    Some((key, age, last_used))
                } else {
                    None
                }
            })
            .collect();
        items.sort_by_key(|(_, _, ct)| *ct);
        items.reverse();
        Ok(items
            .into_iter()
            .take(max)
            .map(|(k, age, _)| (k, age))
            .collect())
    }
}

fn entry_last_used(entry: &mut LazyBlockFileCacheEntry) -> Option<DateTime<Utc>> {
    let rankings = entry.get_rankings_node().ok()?;
    let node = rankings.get().ok()?;
    node.last_used.into_datetime_utc().ok()
}

fn entry_creation_time(entry: &BlockFileCacheEntry) -> Option<DateTime<Utc>> {
    entry.creation_time.into_datetime_utc().ok()
}
