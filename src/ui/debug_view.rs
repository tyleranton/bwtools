use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::App;

pub fn render_debug(frame: &mut ratatui::Frame, area: ratatui::layout::Rect, app: &mut App) {
    let sub = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    let mut resp_lines: Vec<Line> = Vec::new();
    if let Some(port_text) = &app.debug.port_text {
        resp_lines.push(Line::from(Span::styled(
            port_text.clone(),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )));
        resp_lines.push(Line::from(Span::raw(String::new())));
    }
    if let Some(txt) = &app.status.last_profile_text {
        for l in txt.lines() {
            resp_lines.push(Line::from(Span::raw(l.to_string())));
        }
    } else {
        resp_lines.push(Line::from(Span::styled(
            "No profile fetched yet",
            Style::default().fg(Color::DarkGray),
        )));
    }
    let resp = Paragraph::new(resp_lines)
        .scroll((app.debug.scroll, 0))
        .wrap(Wrap { trim: false })
        .alignment(ratatui::layout::Alignment::Left)
        .block(
            Block::default().borders(Borders::ALL).title(Span::styled(
                "Debug: Profile Response",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
        );
    frame.render_widget(resp, sub[0]);

    let mut recent_lines: Vec<Line> = Vec::new();
    for entry in app.debug.recent.iter().take(50) {
        let lc = entry.to_ascii_lowercase();
        let style = if lc.contains("aurora-profile-by-toon") {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::Gray)
        };
        recent_lines.push(Line::from(Span::styled(entry.clone(), style)));
    }
    let recent = Paragraph::new(recent_lines)
        .wrap(Wrap { trim: true })
        .alignment(ratatui::layout::Alignment::Left)
        .block(
            Block::default().borders(Borders::ALL).title(Span::styled(
                format!("Debug: Recent /web-api/ ({}s)", app.debug.window_secs),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )),
        );
    frame.render_widget(recent, sub[1]);
}
