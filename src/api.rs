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
        // Keep only those with total games >= RATING_MIN_GAMES
        let by_guid: std::collections::HashMap<u32, (String, u16, u32)> = agg
            .into_iter()
            .filter(|(_, (_, _, total, _))| *total >= RATING_MIN_GAMES)
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
        if total_games >= RATING_MIN_GAMES { max_rating } else { None }
    }

    pub fn compute_rating_for_name(&self, info: &ScrToonInfo, profile_name: &str) -> Option<u32> {
        let guid = find_guid_for_toon(info, profile_name)?;
        self.compute_rating_for_guid(info, guid)
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

    pub fn profile_stats_last100(&self, profile: &ScrProfile, main_toon: &str) -> (Option<String>, Vec<String>, Vec<bool>) {
        // Collect 1v1 games with real players and identify main player and opponent with races
        // Sort by create_time desc, then take 100
        let mut games: Vec<(&bw_web_api_rs::models::common::Player, &bw_web_api_rs::models::common::Player, u64)> = Vec::new();
        for g in profile.game_results.iter() {
            // Filter players
            let actual: Vec<&bw_web_api_rs::models::common::Player> = g
                .players
                .iter()
                .filter(|p| p.attributes.r#type == "player" && !p.toon.trim().is_empty())
                .collect();
            if actual.len() != 2 { continue; }
            // Find main
            let mi = if actual[0].toon.eq_ignore_ascii_case(main_toon) { 0 } else if actual[1].toon.eq_ignore_ascii_case(main_toon) { 1 } else { continue };
            let oi = 1 - mi;
            let ts = g.create_time.parse::<u64>().unwrap_or(0);
            games.push((actual[mi], actual[oi], ts));
        }
        games.sort_by(|a,b| b.2.cmp(&a.2));
        games.truncate(100);

        // Determine main race by most frequent race of main player
        let mut rc = std::collections::HashMap::new();
        for (m, _, _) in games.iter() {
            if let Some(r) = &m.attributes.race { *rc.entry(r.to_lowercase()).or_insert(0usize) += 1; }
        }
        let main_race = rc.into_iter().max_by_key(|(_, n)| *n).map(|(r, _)| r);

        // Compute matchup winrates for games where main played main_race
        let mut vs: std::collections::HashMap<String, (u32, u32)> = std::collections::HashMap::new();
        if let Some(ref mr) = main_race {
            for (m, o, _) in games.iter() {
                let mrace = m.attributes.race.as_deref().unwrap_or("").to_lowercase();
                if mrace != *mr { continue; }
                let o_race = o.attributes.race.as_deref().unwrap_or("").to_lowercase();
                let entry = vs.entry(o_race).or_insert((0,0));
                // wins, total
                entry.1 += 1;
                let res = m.result.to_lowercase();
                if res == "win" { entry.0 += 1; }
            }
        }
        // Build recent results (newest first): win => true, loss => false
        let mut results: Vec<bool> = Vec::new();
        for (m, _o, _ts) in games.iter() {
            results.push(m.result.eq_ignore_ascii_case("win"));
        }
        // Format lines as XvX with main race initial
        let main_label = |r: &str| match r { "protoss"=>"Protoss", "terran"=>"Terran", "zerg"=>"Zerg", _=>"Unknown" };
        let main_initial = |r: &str| match r { "protoss"=>"P", "terran"=>"T", "zerg"=>"Z", _=>"?" };
        let opp_initial = |r: &str| match r { "protoss"=>"P", "terran"=>"T", "zerg"=>"Z", _=>"?" };
        let order = ["protoss","terran","zerg"];
        let mut lines: Vec<String> = Vec::new();
        let mr_init = main_race.as_deref().map(|s| main_initial(s)).unwrap_or("?");
        for r in order.iter() {
            if let Some((wins, total)) = vs.get(&r.to_string()) {
                if *total > 0 {
                    let pct = ((*wins as f32) / (*total as f32)) * 100.0;
                    lines.push(format!("{}v{}: {:.0}% ({} / {})", mr_init, opp_initial(r), pct.round(), wins, total));
                }
            }
        }
        (main_race.map(|s| main_label(&s).to_string()), lines, results)
    }
}
// Minimum games threshold used for displaying a rating
pub const RATING_MIN_GAMES: u32 = 5;

pub fn find_guid_for_toon(info: &ScrToonInfo, profile_name: &str) -> Option<u32> {
    let season = info.matchmaked_current_season;
    info
        .profiles
        .iter()
        .find(|p| p.toon.eq_ignore_ascii_case(profile_name))
        .map(|p| p.toon_guid)
        .or_else(|| {
            info
                .matchmaked_stats
                .iter()
                .find(|s| s.season_id == season && s.toon.eq_ignore_ascii_case(profile_name))
                .map(|s| s.toon_guid)
        })
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
