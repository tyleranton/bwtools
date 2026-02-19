pub fn normalize_label(raw: &str) -> String {
    match raw.to_ascii_lowercase().as_str() {
        "protoss" => "Protoss".to_string(),
        "terran" => "Terran".to_string(),
        "zerg" => "Zerg".to_string(),
        "random" => "Random".to_string(),
        _ => raw.to_string(),
    }
}

pub fn lower_key(raw: &str) -> String {
    raw.to_ascii_lowercase()
}

pub fn is_random(raw: &str) -> bool {
    raw.eq_ignore_ascii_case("random")
}

pub fn should_replace(existing: Option<&str>, incoming: &str) -> bool {
    if existing.is_none() {
        return true;
    }

    is_random(incoming) && !is_random(existing.unwrap_or_default())
}

pub fn initial(raw: &str) -> &'static str {
    match raw.to_ascii_lowercase().as_str() {
        "protoss" => "P",
        "terran" => "T",
        "zerg" => "Z",
        "random" => "R",
        _ => "?",
    }
}

pub fn display_label(raw: &str) -> &'static str {
    match raw.to_ascii_lowercase().as_str() {
        "protoss" => "Protoss",
        "terran" => "Terran",
        "zerg" => "Zerg",
        "random" => "Random",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_label_canonicalizes_known_races() {
        assert_eq!(normalize_label("terran"), "Terran");
        assert_eq!(normalize_label("RANDOM"), "Random");
        assert_eq!(normalize_label("unknown"), "unknown");
    }

    #[test]
    fn should_replace_prefers_random_and_missing_values() {
        assert!(should_replace(None, "Terran"));
        assert!(should_replace(Some("Terran"), "Random"));
        assert!(!should_replace(Some("Random"), "Zerg"));
        assert!(!should_replace(Some("Protoss"), "Terran"));
    }

    #[test]
    fn initial_and_display_label_map_known_races() {
        assert_eq!(initial("protoss"), "P");
        assert_eq!(display_label("zerg"), "Zerg");
        assert_eq!(initial("unknown"), "?");
        assert_eq!(display_label("unknown"), "Unknown");
    }
}
