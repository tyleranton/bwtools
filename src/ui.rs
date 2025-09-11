use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::{App, View};

pub fn render(frame: &mut ratatui::Frame, app: &App) {
    let size = frame.area();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(size);

    let (status_label, status_style, status_detail) = match app.port {
        Some(_) => (
            "Connected".to_string(),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            String::new(),
        ),
        None => (
            "Waiting for port...".to_string(),
            Style::default().fg(Color::Yellow),
            "Launch StarCraft: Remastered to detect the API port.".to_string(),
        ),
    };
    let self_line_opt = match (&app.self_profile_name, app.self_gateway) {
        (Some(name), Some(gw)) => {
            let rating = app.self_profile_rating.map(|r| format!(" • Rating {}", r)).unwrap_or_default();
            Some(Line::from(Span::styled(
                format!("Self: {} • {}{}", name, crate::api::gateway_label(gw), rating),
                Style::default().fg(Color::Yellow),
            )))
        }
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
    if let Some(self_line) = self_line_opt { status_lines.push(self_line); }

    let status = Paragraph::new(status_lines)
    .alignment(Alignment::Left)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(
                "Status",
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(status, layout[0]);

    let profile_line_opt = match (&app.profile_name, app.gateway) {
        (Some(name), Some(gw)) => Some(format!("Opponent: {}  •  {}", name, crate::api::gateway_label(gw))),
        (Some(name), None) => Some(format!("Opponent: {}", name)),
        _ => None,
    };

    match app.view {
        View::Main => {
            let mut lines: Vec<Line> = vec![Line::from(Span::raw("Press 'q' or Esc to quit."))];
            if let Some(text) = profile_line_opt {
                lines.push(Line::from(Span::raw("")));
                lines.push(Line::from(Span::raw(text)));
                if !app.opponent_toons.is_empty() {
                    lines.push(Line::from(Span::raw("")));
                    lines.push(Line::from(Span::styled(
                        "Opponent toons:",
                        Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
                    )));
                    for item in app.opponent_toons.iter().take(20) {
                        lines.push(Line::from(Span::raw(item.clone())));
                    }
                }
            }
            let panel = Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(Span::styled(
                            "Main",
                            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                        )),
                );
            frame.render_widget(panel, layout[1]);
        }
        View::Debug => {
            let middle = layout[1];
            let sub = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(middle);

            let mut resp_lines: Vec<Line> = Vec::new();
            if let Some(txt) = &app.last_profile_text {
                for l in txt.lines() { resp_lines.push(Line::from(Span::raw(l.to_string()))); }
            } else {
                resp_lines.push(Line::from(Span::styled("No profile fetched yet", Style::default().fg(Color::DarkGray))));
            }
            let resp = Paragraph::new(resp_lines)
                .scroll((app.debug_scroll, 0))
                .wrap(Wrap { trim: false })
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(Span::styled("Debug: Profile Response", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
                );
            frame.render_widget(resp, sub[0]);

            let mut recent_lines: Vec<Line> = Vec::new();
            for entry in app.debug_recent.iter().take(50) {
                let lc = entry.to_ascii_lowercase();
                let style = if lc.contains("aurora-profile-by-toon") { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::Gray) };
                recent_lines.push(Line::from(Span::styled(entry.clone(), style)));
            }
            let recent = Paragraph::new(recent_lines)
                .wrap(Wrap { trim: true })
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(Span::styled(
                            format!("Debug: Recent /web-api/ ({}s)", app.debug_window_secs),
                            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
                        )),
                );
            frame.render_widget(recent, sub[1]);
        }
        View::Search => {
            let mut lines: Vec<Line> = Vec::new();
            lines.push(Line::from(Span::styled("Search", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))));
            lines.push(Line::from(Span::raw("Type toon name, Tab to switch, Left/Right to change gateway, Enter to search.")));
            lines.push(Line::from(Span::raw("")));
            let gw_label = crate::api::gateway_label(app.search_gateway);
            let name_style = if app.search_focus_gateway { Style::default() } else { Style::default().add_modifier(Modifier::BOLD) };
            let gw_style = if app.search_focus_gateway { Style::default().add_modifier(Modifier::BOLD) } else { Style::default() };
            lines.push(Line::from(vec![
                Span::styled("Name: ", Style::default()),
                Span::styled(app.search_name.clone(), name_style),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Gateway: ", Style::default()),
                Span::styled(format!("{} ({})", gw_label, app.search_gateway), gw_style),
            ]));
            lines.push(Line::from(Span::raw("")));
            if let Some(r) = app.search_rating { lines.push(Line::from(Span::raw(format!("Rating: {}", r)))); }
            if !app.search_other_toons.is_empty() {
                lines.push(Line::from(Span::styled("Other toons:", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))));
                for item in app.search_other_toons.iter().take(20) {
                    lines.push(Line::from(Span::raw(item.clone())));
                }
            }
            if !app.search_matches.is_empty() {
                lines.push(Line::from(Span::raw("")));
                lines.push(Line::from(Span::styled("Recent matches:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))));
                for m in app.search_matches.iter().take(20) {
                    lines.push(Line::from(Span::raw(m.clone())));
                }
            }
            if let Some(err) = &app.search_error {
                lines.push(Line::from(Span::styled(format!("Error: {}", err), Style::default().fg(Color::Red))));
            }
            let panel = Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(Span::styled(
                            "Search",
                            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                        )),
                );
            frame.render_widget(panel, layout[1]);
        }
    }

    let footer = Paragraph::new(Line::from(vec![
        Span::styled("bwtools ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("v{}", env!("CARGO_PKG_VERSION")), Style::default().fg(Color::DarkGray)),
    ]))
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, layout[2]);
}
