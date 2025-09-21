use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Wrap};

use crate::app::App;

pub fn render_players(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let query_chars: Vec<char> = app.players.search_query.chars().collect();
    let cursor = app.players.search_cursor.min(query_chars.len());
    let before: String = query_chars[..cursor].iter().collect();
    let after: String = query_chars[cursor..].iter().collect();

    let search_line = Line::from(vec![
        Span::styled("Search: ", Style::default().fg(Color::DarkGray)),
        Span::styled(before, Style::default().fg(Color::White)),
        Span::styled("|", Style::default().fg(Color::LightBlue)),
        Span::styled(after, Style::default().fg(Color::White)),
    ]);

    let search = Paragraph::new(search_line).block(
        Block::default()
            .title("Filter")
            .borders(Borders::ALL)
            .padding(Padding::new(1, 1, 0, 0)),
    );
    frame.render_widget(search, chunks[0]);

    let total_entries = app
        .players
        .directory
        .as_ref()
        .map(|dir| dir.entries().len())
        .unwrap_or(0);

    let lines: Vec<Line> = if app.players.missing_data {
        vec![Line::from(vec![Span::styled(
            "player_list.json not bundled with this build",
            Style::default().fg(Color::Red),
        )])]
    } else if app.players.directory.is_none() {
        vec![Line::from(vec![Span::styled(
            "Loading player list…",
            Style::default().fg(Color::DarkGray),
        )])]
    } else if total_entries == 0 {
        vec![Line::from(vec![Span::styled(
            "Player list unavailable",
            Style::default().fg(Color::DarkGray),
        )])]
    } else if app.players.filtered.is_empty() {
        vec![Line::from(vec![Span::styled(
            "No matches",
            Style::default().fg(Color::DarkGray),
        )])]
    } else {
        app.players
            .filtered
            .iter()
            .map(|entry| {
                Line::from(vec![
                    Span::styled(
                        entry.name.as_str(),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("  •  ", Style::default().fg(Color::DarkGray)),
                    Span::styled(entry.battle_tag.as_str(), Style::default().fg(Color::Cyan)),
                ])
            })
            .collect()
    };

    let title = format!(
        "Player Directory ({}/{})",
        app.players.filtered.len(),
        total_entries
    );
    let list = Paragraph::new(lines)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .padding(Padding::uniform(1)),
        )
        .scroll((app.players.scroll, 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(list, chunks[1]);
}
