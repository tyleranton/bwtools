use anyhow::{Result, anyhow};
use bw_web_api_rs::{ApiClient, ApiConfig, types::Gateway};
use bw_web_api_rs::models::aurora_profile::{ScrToonInfo, ScrMmGameLoading, ScrProfile};

pub struct ApiHandle {
    client: ApiClient,
    rt: tokio::runtime::Runtime,
}

impl ApiHandle {
    pub fn new(base_url: String) -> Result<Self> {
        let config = ApiConfig { base_url, api_key: None };
        let client = ApiClient::new(config)?;
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        Ok(Self { client, rt })
    }

    pub fn get_toon_info(&self, name: &str, gw_num: u16) -> Result<ScrToonInfo> {
        let gw = map_gateway(gw_num).ok_or_else(|| anyhow!("Unknown gateway: {}", gw_num))?;
        let fut = self.client.get_aurora_profile_by_toon_toon_info(name.to_string(), gw);
        let toon_info: ScrToonInfo = self.rt.block_on(fut)?;
        Ok(toon_info)
    }

    pub fn get_mm_game_loading(&self, name: &str, gw_num: u16) -> Result<ScrMmGameLoading> {
        let gw = map_gateway(gw_num).ok_or_else(|| anyhow!("Unknown gateway: {}", gw_num))?;
        let fut = self.client.get_aurora_profile_by_toon_mm_game_loading(name.to_string(), gw);
        let data: ScrMmGameLoading = self.rt.block_on(fut)?;
        Ok(data)
    }

    pub fn opponent_toons_summary(&self, name: &str, gw_num: u16) -> Result<Vec<(String, u16, u32)>> {
        let data = self.get_mm_game_loading(name, gw_num)?;

        let mut guid_to_gateway: std::collections::HashMap<u32, u16> = std::collections::HashMap::new();
        for (gw_str, mapping) in data.toon_guid_by_gateway.iter() {
            if let Ok(gw) = gw_str.parse::<u16>() {
                for (_toon_name, guid) in mapping.iter() {
                    guid_to_gateway.insert(*guid, gw);
                }
            }
        }

        // Aggregate per guid: total games in season and max rating in season
        let season = data.matchmaked_current_season;
        let mut agg: std::collections::HashMap<u32, (String, u16, u32, u32)> = std::collections::HashMap::new();
        for s in data.matchmaked_stats.iter().filter(|s| s.season_id == season) {
            let gw = guid_to_gateway.get(&s.toon_guid).copied().unwrap_or(0);
            let entry = agg.entry(s.toon_guid).or_insert_with(|| (s.toon.clone(), gw, 0, 0));
            // entry = (toon, gw, total_games, max_rating)
            entry.2 = entry.2.saturating_add(s.wins + s.losses);
            if s.rating > entry.3 { entry.3 = s.rating; }
        }
        // Keep only those with total games >= 5
        let by_guid: std::collections::HashMap<u32, (String, u16, u32)> = agg
            .into_iter()
            .filter(|(_, (_, _, total, _))| *total >= 5)
            .map(|(guid, (toon, gw, _total, max_rating))| (guid, (toon, gw, max_rating)))
            .collect();

        let mut out: Vec<(String, u16, u32)> = by_guid.into_values().collect();
        out.sort_by(|a, b| b.2.cmp(&a.2));
        Ok(out)
    }

    pub fn get_scr_profile(&self, name: &str, gw_num: u16) -> Result<ScrProfile> {
        let gw = map_gateway(gw_num).ok_or_else(|| anyhow!("Unknown gateway: {}", gw_num))?;
        let fut = self.client.get_aurora_profile_by_toon_scr_profile(name.to_string(), gw);
        let data: ScrProfile = self.rt.block_on(fut)?;
        Ok(data)
    }

    pub fn compute_rating_for_guid(&self, info: &ScrToonInfo, target_guid: u32) -> Option<u32> {
        let season = info.matchmaked_current_season;
        // total games across all season buckets
        let mut total_games: u32 = 0;
        let mut max_rating: Option<u32> = None;
        for s in info.matchmaked_stats.iter().filter(|s| s.toon_guid == target_guid && s.season_id == season) {
            total_games = total_games.saturating_add(s.wins + s.losses);
            max_rating = Some(max_rating.map_or(s.rating, |m| m.max(s.rating)));
        }
        if total_games >= 5 { max_rating } else { None }
    }

    pub fn other_toons_with_ratings(&self, info: &ScrToonInfo, main_toon: &str) -> Vec<(String, u16, u32)> {
        // guid -> gateway
        let mut guid_to_gateway: std::collections::HashMap<u32, u16> = std::collections::HashMap::new();
        for (gw_str, mapping) in info.toon_guid_by_gateway.iter() {
            if let Ok(gw) = gw_str.parse::<u16>() {
                for (_toon_name, guid) in mapping.iter() {
                    guid_to_gateway.insert(*guid, gw);
                }
            }
        }
        // Aggregate from stats to ensure we capture all toons present this season
        let season = info.matchmaked_current_season;
        let mut agg: std::collections::HashMap<u32, (String, u16, u32, u32)> = std::collections::HashMap::new();
        for s in info.matchmaked_stats.iter().filter(|s| s.season_id == season) {
            let gw = guid_to_gateway.get(&s.toon_guid).copied().unwrap_or(0);
            let entry = agg.entry(s.toon_guid).or_insert_with(|| (s.toon.clone(), gw, 0, 0));
            entry.2 = entry.2.saturating_add(s.wins + s.losses); // total games
            if s.rating > entry.3 { entry.3 = s.rating; } // max rating
            // prefer non-empty toon name
            if entry.0.trim().is_empty() && !s.toon.trim().is_empty() { entry.0 = s.toon.clone(); }
        }
        let mut out: Vec<(String, u16, u32)> = agg
            .into_values()
            .filter(|(toon, _gw, total, _maxr)| *total >= 5 && !toon.eq_ignore_ascii_case(main_toon))
            .map(|(toon, gw, _total, maxr)| (toon, gw, maxr))
            .collect();
        out.sort_by(|a,b| b.2.cmp(&a.2));
        out
    }

    pub fn match_summaries(&self, profile: &ScrProfile, main_toon: &str) -> Vec<String> {
        let mut out = Vec::new();
        for g in profile.game_results.iter() {
            // Keep only real players: type == "player" and non-empty toon
            let actual: Vec<(usize, &bw_web_api_rs::models::common::Player)> = g
                .players
                .iter()
                .enumerate()
                .filter(|(_, p)| p.attributes.r#type == "player" && !p.toon.trim().is_empty())
                .collect();
            if actual.len() != 2 { continue; }

            // Find main among actual players (case-insensitive)
            let main_pos = actual
                .iter()
                .position(|(_, p)| p.toon.eq_ignore_ascii_case(main_toon));
            let Some(mi_pos) = main_pos else { continue };
            let (mi_idx, main_player) = actual[mi_pos];
            let (_, opp_player) = actual[1 - mi_pos];

            // Sanitize opponent name
            let opp = if opp_player.toon.trim().is_empty() { "Unknown".to_string() } else { opp_player.toon.clone() };

            // Use main player's result
            let result = match main_player.result.to_ascii_lowercase().as_str() {
                "win" => "Win",
                "loss" => "Loss",
                _ => &main_player.result,
            };
            let _ = mi_idx; // silence unused if optimized away
            out.push(format!("{} vs {}", result, opp));
        }
        out
    }
}
pub fn map_gateway(num: u16) -> Option<Gateway> {
    match num {
        10 => Some(Gateway::USWest),
        11 => Some(Gateway::USEast),
        20 => Some(Gateway::Europe),
        30 => Some(Gateway::Korea),
        45 => Some(Gateway::Asia),
        _ => None,
    }
}

pub fn gateway_label(num: u16) -> &'static str {
    match num {
        10 => "US West",
        11 => "US East",
        20 => "Europe",
        30 => "Korea",
        45 => "Asia",
        _ => "Unknown",
    }
}
