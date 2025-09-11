use anyhow::{Result, anyhow};
use bw_web_api_rs::{ApiClient, ApiConfig, types::Gateway};
use bw_web_api_rs::models::aurora_profile::{ScrToonInfo, ScrMmGameLoading};

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

        let mut by_guid: std::collections::HashMap<u32, (String, u16, u32)> = std::collections::HashMap::new();
        for s in data.matchmaked_stats.iter() {
            let gw = guid_to_gateway.get(&s.toon_guid).copied().unwrap_or(0);
            let entry = by_guid.entry(s.toon_guid).or_insert_with(|| (s.toon.clone(), gw, s.rating));
            if s.rating > entry.2 {
                entry.2 = s.rating;
            }
        }

        let mut out: Vec<(String, u16, u32)> = by_guid.into_values().collect();
        out.sort_by(|a, b| b.2.cmp(&a.2));
        Ok(out)
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
