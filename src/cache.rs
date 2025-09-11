use std::path::PathBuf;

use anyhow::{Context, Result};
use chrome_cache_parser::ChromeCache;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use url::Url;

fn extract_port(url: &str) -> Option<u16> {
    Url::parse(url).ok().and_then(|parsed| parsed.port())
}

fn parse_profile_from_url_mmgameloading(url: &str) -> Option<(String, u16)> {
    let parsed = Url::parse(url).ok()?;
    // Check request_flags contains scr_mmgameloading
    let has_flag = parsed
        .query_pairs()
        .any(|(k, v)| k == "request_flags" && v.as_ref().contains("scr_mmgameloading"));
    if !has_flag {
        return None;
    }

    let mut segments = parsed.path_segments()?;
    // Expect: /web-api/v2/aurora-profile-by-toon/{profile}/{gateway}/
    let s1 = segments.next()?; // web-api
    let s2 = segments.next()?; // v2
    let s3 = segments.next()?; // aurora-profile-by-toon
    if s1 != "web-api" || s2 != "v2" || s3 != "aurora-profile-by-toon" {
        return None;
    }
    let profile = segments.next()?.to_string();
    let gateway_str = segments.next()?;
    let gateway: u16 = gateway_str.parse().ok()?;
    Some((profile, gateway))
}

fn parse_profile_from_url_path(url: &str) -> Option<(String, u16)> {
    let parsed = Url::parse(url).ok()?;
    let mut segments = parsed.path_segments()?;
    let s1 = segments.next()?; // web-api
    let s2 = segments.next()?; // v2
    let s3 = segments.next()?; // aurora-profile-by-toon
    if s1 != "web-api" || s2 != "v2" || s3 != "aurora-profile-by-toon" {
        return None;
    }
    let profile = segments.next()?.to_string();
    let gw: u16 = segments.next()?.parse().ok()?;
    Some((profile, gw))
}

pub struct CacheReader {
    cache_dir: PathBuf,
    cache: ChromeCache,
}

impl CacheReader {
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        let cache = ChromeCache::from_path(cache_dir.clone()).context("Failed to open Chrome cache")?;
        Ok(Self { cache_dir, cache })
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.cache = ChromeCache::from_path(self.cache_dir.clone()).context("Failed to refresh Chrome cache")?;
        Ok(())
    }

    pub fn parse_for_port(&self, window_secs: i64) -> Result<Option<u16>> {
        let entries = self.cache.entries().context("Failed to read cache entries")?;

        let now = Utc::now();

        let latest_port = entries
            .filter_map(|e| {
                let entry = e.get().ok()?;
                let key = entry.key.to_string();
                if !key.contains("/web-api/") {
                    return None;
                }
                let creation_time: DateTime<Utc> = entry.creation_time.into_datetime_utc().ok()?;
                if (now - creation_time) >= ChronoDuration::seconds(window_secs) {
                    return None;
                }
                let port = extract_port(&key)?;
                Some((port, creation_time))
            })
            .max_by_key(|(_, creation_time)| *creation_time)
            .map(|(port, _)| port);

        Ok(latest_port)
    }

    pub fn latest_opponent_profile(&self, exclude_name: Option<&str>, window_secs: i64) -> Result<Option<(String, u16)>> {
        let entries = self.cache.entries().context("Failed to read cache entries")?;

        let now = Utc::now();

        let latest = entries
            .filter_map(|e| {
                let entry = e.get().ok()?;
                let key = entry.key.to_string();
                // quick contains filter first
                if !(key.contains("/web-api/v2/aurora-profile-by-toon/") && key.contains("scr_mmgameloading")) {
                    return None;
                }
                let creation_time: DateTime<Utc> = entry.creation_time.into_datetime_utc().ok()?;
                if (now - creation_time) >= ChronoDuration::seconds(window_secs) {
                    return None;
                }
                let (profile, gateway) = parse_profile_from_url_mmgameloading(&key)?;
                if let Some(ex) = exclude_name {
                    if profile.eq_ignore_ascii_case(ex) { return None; }
                }
                Some(((profile, gateway), creation_time))
            })
            .max_by_key(|(_, creation_time)| *creation_time)
            .map(|(data, _)| data);

        Ok(latest)
    }

    pub fn latest_mmgameloading_profile(&self, window_secs: i64) -> Result<Option<(String, u16)>> {
        let entries = self.cache.entries().context("Failed to read cache entries")?;
        let now = Utc::now();
        let latest = entries
            .filter_map(|e| {
                let entry = e.get().ok()?;
                let key = entry.key.to_string();
                if !(key.contains("/web-api/v2/aurora-profile-by-toon/") && key.contains("scr_mmgameloading")) {
                    return None;
                }
                let creation_time: DateTime<Utc> = entry.creation_time.into_datetime_utc().ok()?;
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

    pub fn latest_self_profile(&self, window_secs: i64) -> Result<Option<(String, u16)>> {
        let entries = self.cache.entries().context("Failed to read cache entries")?;
        let now = Utc::now();
        let latest = entries
            .filter_map(|e| {
                let entry = e.get().ok()?;
                let key = entry.key.to_string();
                if !(key.contains("/web-api/v2/aurora-profile-by-toon/") && key.contains("scr_tooninfo")) {
                    return None;
                }
                let creation_time: DateTime<Utc> = entry.creation_time.into_datetime_utc().ok()?;
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

    // Debug helpers

    pub fn recent_keys(&self, window_secs: i64, max: usize) -> Result<Vec<(String, i64)>> {
        let now = Utc::now();
        let entries = self.cache.entries().context("Failed to read cache entries")?;
        let mut items: Vec<(String, i64, DateTime<Utc>)> = entries
            .filter_map(|e| {
                let entry = e.get().ok()?;
                let key = entry.key.to_string();
                // Only include web-api endpoints for debug visibility
                if !key.contains("/web-api/") {
                    return None;
                }
                let creation_time: DateTime<Utc> = entry.creation_time.into_datetime_utc().ok()?;
                let age = (now - creation_time).num_seconds();
                if age <= window_secs {
                    Some((key, age, creation_time))
                } else {
                    None
                }
            })
            .collect();
        // sort by newest first
        items.sort_by_key(|(_, _, ct)| *ct);
        items.reverse();
        Ok(items
            .into_iter()
            .take(max)
            .map(|(k, age, _)| (k, age))
            .collect())
    }
}
