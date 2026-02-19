use bw_web_api_rs::types::Gateway;

pub const GATEWAY_CYCLE: [u16; 5] = [10, 11, 20, 30, 45];

pub fn next_gateway(current: u16) -> u16 {
    rotate_gateway(current, Rotation::Next)
}

pub fn prev_gateway(current: u16) -> u16 {
    rotate_gateway(current, Rotation::Prev)
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

pub fn label(num: u16) -> &'static str {
    match num {
        10 => "US West",
        11 => "US East",
        20 => "Europe",
        30 => "Korea",
        45 => "Asia",
        _ => "Unknown",
    }
}

enum Rotation {
    Next,
    Prev,
}

fn rotate_gateway(current: u16, direction: Rotation) -> u16 {
    let idx = GATEWAY_CYCLE
        .iter()
        .position(|gw| *gw == current)
        .unwrap_or_else(|| match direction {
            Rotation::Next => GATEWAY_CYCLE.len().saturating_sub(1),
            Rotation::Prev => 0,
        });

    match direction {
        Rotation::Next => GATEWAY_CYCLE[(idx + 1) % GATEWAY_CYCLE.len()],
        Rotation::Prev => GATEWAY_CYCLE[(idx + GATEWAY_CYCLE.len() - 1) % GATEWAY_CYCLE.len()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_and_label_cover_known_gateways() {
        assert_eq!(map_gateway(10), Some(Gateway::USWest));
        assert_eq!(map_gateway(11), Some(Gateway::USEast));
        assert_eq!(map_gateway(20), Some(Gateway::Europe));
        assert_eq!(map_gateway(30), Some(Gateway::Korea));
        assert_eq!(map_gateway(45), Some(Gateway::Asia));
        assert_eq!(map_gateway(999), None);

        assert_eq!(label(10), "US West");
        assert_eq!(label(11), "US East");
        assert_eq!(label(20), "Europe");
        assert_eq!(label(30), "Korea");
        assert_eq!(label(45), "Asia");
        assert_eq!(label(999), "Unknown");
    }

    #[test]
    fn next_and_previous_gateways_wrap_cycle() {
        assert_eq!(next_gateway(10), 11);
        assert_eq!(next_gateway(45), 10);
        assert_eq!(prev_gateway(11), 10);
        assert_eq!(prev_gateway(10), 45);
    }

    #[test]
    fn unknown_gateways_use_cycle_defaults() {
        assert_eq!(next_gateway(999), 10);
        assert_eq!(prev_gateway(999), 45);
    }
}
