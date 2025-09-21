use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

pub fn profile_stat_lines(
    rating: Option<u32>,
    main_race: Option<&str>,
    matchups: &[String],
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    let rating_text = rating
        .map(|r| r.to_string())
        .unwrap_or_else(|| "N/A".to_string());
    let race_text = main_race.unwrap_or("N/A").to_string();

    lines.push(Line::from(vec![
        Span::styled(
            "Rating: ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(rating_text),
        Span::raw("    "),
        Span::styled(
            "Race: ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(race_text),
    ]));

    if matchups.is_empty() {
        lines.push(Line::from(Span::styled(
            "No matchup stats.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for entry in matchups {
            if let Some((label, rest)) = entry.split_once(':') {
                let label_text = label.trim().to_string();
                lines.push(Line::from(vec![
                    Span::styled(
                        label_text,
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(":"),
                    Span::raw(rest.to_string()),
                ]));
            } else {
                lines.push(Line::from(Span::raw(entry.clone())));
            }
        }
    }

    lines
}
