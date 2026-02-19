use ratatui::layout::Alignment;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;

pub fn render_status(frame: &mut ratatui::Frame, area: ratatui::layout::Rect, app: &mut App) {
    let (status_label, status_style, status_detail) = match app.detection.port {
        Some(_) => (
            "Connected".to_string(),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            String::new(),
        ),
        None => (
            "Waiting for port...".to_string(),
            Style::default().fg(Color::Yellow),
            "Launch StarCraft: Remastered to detect the API port.".to_string(),
        ),
    };
    let self_line_opt = match (&app.self_profile.name, app.self_profile.gateway) {
        (Some(name), Some(gw)) => Some(Line::from(Span::styled(
            format!("Self: {} • {}", name, crate::gateway::label(gw)),
            Style::default().fg(Color::Yellow),
        ))),
        (Some(name), None) => Some(Line::from(Span::styled(
            format!("Self: {}", name),
            Style::default().fg(Color::Yellow),
        ))),
        _ => None,
    };

    let mut status_lines = vec![Line::from(Span::styled(status_label, status_style))];
    if !status_detail.is_empty() {
        status_lines.push(Line::from(status_detail));
    }
    if let Some(self_line) = self_line_opt {
        status_lines.push(self_line);
    }

    let status_block = Block::default().borders(Borders::ALL).title(Span::styled(
        "Status",
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    ));
    let status = Paragraph::new(status_lines)
        .alignment(Alignment::Left)
        .block(status_block);
    frame.render_widget(status, area);
}
