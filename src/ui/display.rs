pub fn bulleted(parts: &[String]) -> String {
    parts.join(" • ")
}

pub fn opponent_header(name: &str, gateway_label: &str, race: Option<&str>, rating: Option<u32>) -> String {
    let mut parts: Vec<String> = vec![name.to_string(), gateway_label.to_string()];
    if let Some(r) = race { parts.push(r.to_string()); }
    if let Some(v) = rating { parts.push(v.to_string()); }
    bulleted(&parts)
}

pub fn toon_line(toon: &str, gateway_label: &str, rating: u32) -> String {
    bulleted(&[toon.to_string(), gateway_label.to_string(), rating.to_string()])
}

