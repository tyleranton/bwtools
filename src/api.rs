use std::sync::OnceLock;

use anyhow::{Result, anyhow};
use bw_web_api_rs::models::aurora_profile::{ScrMmGameLoading, ScrProfile, ScrToonInfo};
use bw_web_api_rs::models::matchmaker_player_info::MatchmakerPlayerInfo;
use bw_web_api_rs::{ApiClient, ApiConfig, types::Gateway};

use crate::profile_history::{MatchOutcome, ProfileHistoryKey, ProfileHistoryService, StoredMatch};

pub struct ApiHandle {
    client: ApiClient,
}

impl ApiHandle {
    pub fn new(base_url: String) -> Result<Self> {
        let config = ApiConfig {
            base_url,
            api_key: None,
        };
        let client = ApiClient::new(config)?;
        Ok(Self { client })
    }

    pub fn get_toon_info(&self, name: &str, gw_num: u16) -> Result<ScrToonInfo> {
        let gw = map_gateway(gw_num).ok_or_else(|| anyhow!("Unknown gateway: {}", gw_num))?;
        let fut = self
            .client
            .get_aurora_profile_by_toon_toon_info(name.to_string(), gw);
        let toon_info: ScrToonInfo = runtime().block_on(fut)?;
        Ok(toon_info)
    }

    pub fn get_mm_game_loading(&self, name: &str, gw_num: u16) -> Result<ScrMmGameLoading> {
        let gw = map_gateway(gw_num).ok_or_else(|| anyhow!("Unknown gateway: {}", gw_num))?;
        let fut = self
            .client
            .get_aurora_profile_by_toon_mm_game_loading(name.to_string(), gw);
        let data: ScrMmGameLoading = runtime().block_on(fut)?;
        Ok(data)
    }

    pub fn opponent_toons_summary(
        &self,
        name: &str,
        gw_num: u16,
    ) -> Result<Vec<(String, u16, u32)>> {
        let data = self.get_mm_game_loading(name, gw_num)?;

        let mut guid_to_gateway: std::collections::HashMap<u32, u16> =
            std::collections::HashMap::new();
        for (gw_str, mapping) in data.toon_guid_by_gateway.iter() {
            if let Ok(gw) = gw_str.parse::<u16>() {
                for (_toon_name, guid) in mapping.iter() {
                    guid_to_gateway.insert(*guid, gw);
                }
            }
        }

        // Aggregate per guid: total games in season and max rating in season
        let season = data.matchmaked_current_season;
        let mut agg: std::collections::HashMap<u32, (String, u16, u32, u32)> =
            std::collections::HashMap::new();
        for s in data
            .matchmaked_stats
            .iter()
            .filter(|s| s.season_id == season)
        {
            let gw = guid_to_gateway.get(&s.toon_guid).copied().unwrap_or(0);
            let entry = agg
                .entry(s.toon_guid)
                .or_insert_with(|| (s.toon.clone(), gw, 0, 0));
            // entry = (toon, gw, total_games, max_rating)
            entry.2 = entry.2.saturating_add(s.wins + s.losses);
            if s.rating > entry.3 {
                entry.3 = s.rating;
            }
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
        let fut = self
            .client
            .get_aurora_profile_by_toon_scr_profile(name.to_string(), gw);
        let data: ScrProfile = runtime().block_on(fut)?;
        Ok(data)
    }

    pub fn get_matchmaker_player_info(&self, match_id: &str) -> Result<MatchmakerPlayerInfo> {
        let fut = self.client.get_matchmaker_player_info(match_id.to_string());
        let data: MatchmakerPlayerInfo = runtime().block_on(fut)?;
        Ok(data)
    }

    pub fn compute_rating_for_guid(&self, info: &ScrToonInfo, target_guid: u32) -> Option<u32> {
        let season = info.matchmaked_current_season;
        // total games across all season buckets
        let mut total_games: u32 = 0;
        let mut max_rating: Option<u32> = None;
        for s in info
            .matchmaked_stats
            .iter()
            .filter(|s| s.toon_guid == target_guid && s.season_id == season)
        {
            total_games = total_games.saturating_add(s.wins + s.losses);
            max_rating = Some(max_rating.map_or(s.rating, |m| m.max(s.rating)));
        }
        if total_games >= RATING_MIN_GAMES {
            max_rating
        } else {
            None
        }
    }

    pub fn compute_rating_for_name(&self, info: &ScrToonInfo, profile_name: &str) -> Option<u32> {
        let guid = find_guid_for_toon(info, profile_name)?;
        self.compute_rating_for_guid(info, guid)
    }

    pub fn other_toons_with_ratings(
        &self,
        info: &ScrToonInfo,
        main_toon: &str,
    ) -> Vec<(String, u16, u32)> {
        // guid -> gateway
        let mut guid_to_gateway: std::collections::HashMap<u32, u16> =
            std::collections::HashMap::new();
        for (gw_str, mapping) in info.toon_guid_by_gateway.iter() {
            if let Ok(gw) = gw_str.parse::<u16>() {
                for (_toon_name, guid) in mapping.iter() {
                    guid_to_gateway.insert(*guid, gw);
                }
            }
        }
        // Aggregate from stats to ensure we capture all toons present this season
        let season = info.matchmaked_current_season;
        let mut agg: std::collections::HashMap<u32, (String, u16, u32, u32)> =
            std::collections::HashMap::new();
        for s in info
            .matchmaked_stats
            .iter()
            .filter(|s| s.season_id == season)
        {
            let gw = guid_to_gateway.get(&s.toon_guid).copied().unwrap_or(0);
            let entry = agg
                .entry(s.toon_guid)
                .or_insert_with(|| (s.toon.clone(), gw, 0, 0));
            entry.2 = entry.2.saturating_add(s.wins + s.losses); // total games
            if s.rating > entry.3 {
                entry.3 = s.rating;
            } // max rating
            // prefer non-empty toon name
            if entry.0.trim().is_empty() && !s.toon.trim().is_empty() {
                entry.0 = s.toon.clone();
            }
        }
        let mut out: Vec<(String, u16, u32)> = agg
            .into_values()
            .filter(|(toon, _gw, total, _maxr)| {
                *total >= 5 && !toon.eq_ignore_ascii_case(main_toon)
            })
            .map(|(toon, gw, _total, maxr)| (toon, gw, maxr))
            .collect();
        out.sort_by(|a, b| b.2.cmp(&a.2));
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
            if actual.len() != 2 {
                continue;
            }

            // Find main among actual players (case-insensitive)
            let main_pos = actual
                .iter()
                .position(|(_, p)| p.toon.eq_ignore_ascii_case(main_toon));
            let Some(mi_pos) = main_pos else { continue };
            let (mi_idx, main_player) = actual[mi_pos];
            let (_, opp_player) = actual[1 - mi_pos];

            // Sanitize opponent name
            let opp = if opp_player.toon.trim().is_empty() {
                "Unknown".to_string()
            } else {
                opp_player.toon.clone()
            };

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

    pub fn profile_stats_last100(
        &self,
        profile: &ScrProfile,
        main_toon: &str,
        profile_history: Option<&mut ProfileHistoryService>,
        history_key: Option<&ProfileHistoryKey>,
    ) -> (Option<String>, Vec<String>, Vec<bool>, u32, u32) {
        let mut matches: Vec<StoredMatch> = Vec::new();
        for g in profile.game_results.iter() {
            let actual: Vec<&bw_web_api_rs::models::common::Player> = g
                .players
                .iter()
                .filter(|p| p.attributes.r#type == "player" && !p.toon.trim().is_empty())
                .collect();
            if actual.len() != 2 {
                continue;
            }
            let mi = if actual[0].toon.eq_ignore_ascii_case(main_toon) {
                0
            } else if actual[1].toon.eq_ignore_ascii_case(main_toon) {
                1
            } else {
                continue;
            };
            let oi = 1 - mi;
            let ts = g.create_time.parse::<u64>().unwrap_or(0);
            let main_player = actual[mi];
            let opp_player = actual[oi];
            let result = if main_player.result.eq_ignore_ascii_case("win") {
                MatchOutcome::Win
            } else {
                MatchOutcome::Loss
            };
            let opponent_name = if opp_player.toon.trim().is_empty() {
                "Unknown".to_string()
            } else {
                opp_player.toon.clone()
            };
            matches.push(StoredMatch {
                timestamp: ts,
                opponent: opponent_name,
                opponent_race: opp_player.attributes.race.clone(),
                main_race: main_player.attributes.race.clone(),
                result,
            });
        }
        matches.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        let combined = if let (Some(history), Some(key)) = (profile_history, history_key) {
            match history.merge_matches(key, matches.clone()) {
                Ok(merged) => merged,
                Err(err) => {
                    tracing::error!(error = %err, "failed to merge profile history");
                    matches.into_iter().take(100).collect()
                }
            }
        } else {
            matches.into_iter().take(100).collect()
        };

        let mut race_counts = std::collections::HashMap::new();
        for m in combined.iter() {
            if let Some(r) = &m.main_race {
                let lower = r.to_ascii_lowercase();
                *race_counts.entry(lower).or_insert(0usize) += 1;
            }
        }

        let is_random_player = ["protoss", "terran", "zerg"]
            .into_iter()
            .all(|race| race_counts.get(race).copied().unwrap_or_default() > 0);

        let main_race_lower = if is_random_player {
            Some("random".to_string())
        } else {
            race_counts
                .into_iter()
                .max_by_key(|(_, count)| *count)
                .map(|(race, _)| race)
        };

        let mut matchup = std::collections::HashMap::new();
        for m in combined.iter() {
            if !m.result.counts_for_record() {
                continue;
            }

            let include_match = if is_random_player {
                m.main_race.as_deref().map_or(false, |race| {
                    matches!(
                        race.to_ascii_lowercase().as_str(),
                        "protoss" | "terran" | "zerg"
                    )
                })
            } else if let Some(ref mr) = main_race_lower {
                m.main_race
                    .as_deref()
                    .map(|race| race.eq_ignore_ascii_case(mr))
                    .unwrap_or(false)
            } else {
                false
            };

            if !include_match {
                continue;
            }

            let opp = m.opponent_race.as_deref().unwrap_or("").to_lowercase();
            let entry = matchup.entry(opp).or_insert((0u32, 0u32));
            entry.1 = entry.1.saturating_add(1);
            if m.result.is_win() {
                entry.0 = entry.0.saturating_add(1);
            }
        }

        let mut results: Vec<bool> = Vec::new();
        let mut total_wins: u32 = 0;
        let mut total_games: u32 = 0;
        let mut self_dodged: u32 = 0;
        let mut opponent_dodged: u32 = 0;
        for m in combined.iter() {
            if m.result.counts_for_record() {
                let is_win = m.result.is_win();
                total_games = total_games.saturating_add(1);
                if is_win {
                    total_wins = total_wins.saturating_add(1);
                }
                results.push(is_win);
            } else if m.result.is_self_dodged() {
                self_dodged = self_dodged.saturating_add(1);
            } else if m.result.is_opponent_dodged() {
                opponent_dodged = opponent_dodged.saturating_add(1);
            }
        }

        let main_label = |r: &str| match r {
            "protoss" => "Protoss",
            "terran" => "Terran",
            "zerg" => "Zerg",
            "random" => "Random",
            _ => "Unknown",
        };
        let main_initial = |r: &str| match r {
            "protoss" => "P",
            "terran" => "T",
            "zerg" => "Z",
            "random" => "R",
            _ => "?",
        };
        let opp_initial = |r: &str| match r {
            "protoss" => "P",
            "terran" => "T",
            "zerg" => "Z",
            "random" => "R",
            _ => "?",
        };

        let order = ["protoss", "terran", "zerg", "random"];
        let mut lines: Vec<String> = Vec::new();
        let mr_init = main_race_lower.as_deref().map(main_initial).unwrap_or("?");
        #[allow(clippy::collapsible_if)]
        for r in order.iter() {
            if let Some((wins, total)) = matchup.get(*r) {
                if *total > 0 {
                    let pct = ((*wins as f32) / (*total as f32)) * 100.0;
                    lines.push(format!(
                        "{}v{}: {:.0}% ({} / {})",
                        mr_init,
                        opp_initial(r),
                        pct.round(),
                        wins,
                        total,
                    ));
                }
            }
        }

        if total_games > 0 {
            let overall_pct = ((total_wins as f32) / (total_games as f32)) * 100.0;
            lines.push(format!(
                "Overall: {:.0}% ({} / {})",
                overall_pct.round(),
                total_wins,
                total_games,
            ));
        } else {
            lines.push("Overall: N/A".to_string());
        }

        let main_race_display = main_race_lower
            .as_deref()
            .map(|race| main_label(race).to_string());

        (
            main_race_display,
            lines,
            results,
            self_dodged,
            opponent_dodged,
        )
    }
}
// Minimum games threshold used for displaying a rating
pub const RATING_MIN_GAMES: u32 = 5;

pub fn find_guid_for_toon(info: &ScrToonInfo, profile_name: &str) -> Option<u32> {
    let season = info.matchmaked_current_season;
    info.profiles
        .as_ref()
        .and_then(|profiles| {
            profiles
                .iter()
                .find(|p| p.toon.eq_ignore_ascii_case(profile_name))
                .map(|p| p.toon_guid)
        })
        .or_else(|| {
            info.matchmaked_stats
                .iter()
                .find(|s| s.season_id == season && s.toon.eq_ignore_ascii_case(profile_name))
                .map(|s| s.toon_guid)
        })
}

fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build global tokio runtime")
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
