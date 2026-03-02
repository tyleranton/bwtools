use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use reqwest::blocking::Client;

use crate::config::Config;

pub fn download_replay(client: &Client, url: &str, path: &Path) -> Result<()> {
    let mut response = client
        .get(url)
        .send()
        .with_context(|| format!("send replay download request: {}", url))?
        .error_for_status()
        .with_context(|| format!("replay download HTTP status: {}", url))?;
    let mut file =
        fs::File::create(path).with_context(|| format!("create replay file at {:?}", path))?;
    io::copy(&mut response, &mut file)
        .with_context(|| format!("write replay data to {:?}", path))?;
    Ok(())
}

pub fn run_screp_overview(cfg: &Config, path: &Path) -> Result<String> {
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

pub fn sanitize_identifier(input: &str) -> String {
    let mut out: String = input
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if out.is_empty() {
        out.push('r');
    }
    out
}

pub fn sanitize_component(input: &str) -> String {
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
