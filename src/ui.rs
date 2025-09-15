use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::{App, View};

pub fn render(frame: &mut ratatui::Frame, app: &mut App) {
    let size = frame.area();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
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

    let status_block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            "Status",
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ));
    let status = Paragraph::new(status_lines)
        .alignment(Alignment::Left)
        .block(status_block);
    app.status_opponent_rect = None;
    frame.render_widget(status, layout[0]);


    match app.view {
        View::Main => {
            let main_area = layout[1];
            let sub = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Length(7), Constraint::Min(0)])
                .split(main_area);

            let header = Paragraph::new(vec![
                Line::from(Span::raw("Ctrl+S Search  •  Ctrl+D Debug  •  Ctrl+Q/Esc Quit")),
            ])
                .alignment(Alignment::Left)
            .block(Block::default().borders(Borders::ALL).title(Span::styled(
                "Main",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            )));
            frame.render_widget(header, sub[0]);

            let stats_block = Block::default().borders(Borders::ALL).title(Span::styled(
                "Profile Stats",
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ));
            let stats_area = sub[1];
            let stats_inner = stats_block.inner(stats_area);
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(65), Constraint::Min(10)])
                .split(stats_inner);

            let mut stats_lines: Vec<Line> = Vec::new();
            if let Some(ref r) = app.self_main_race {
                stats_lines.push(Line::from(vec![
                    Span::styled("Race: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::raw(r.clone()),
                ]));
            } else {
                stats_lines.push(Line::from(vec![
                    Span::styled("Race: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::raw("N/A"),
                ]));
            }
            if app.self_matchups.is_empty() {
                stats_lines.push(Line::from(Span::styled("No matchup stats.", Style::default().fg(Color::DarkGray))));
            } else {
                for m in app.self_matchups.iter() {
                    if let Some((label, rest)) = m.split_once(':') {
                        stats_lines.push(Line::from(vec![
                            Span::styled(label.trim(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                            Span::raw(":"),
                            Span::raw(rest.to_string()),
                        ]));
                    } else {
                        stats_lines.push(Line::from(Span::raw(m.clone())));
                    }
                }
            }
            frame.render_widget(stats_block, stats_area);
            frame.render_widget(Paragraph::new(stats_lines).alignment(Alignment::Left), cols[0]);

            frame.render_widget(
                Paragraph::new(Line::from(Span::raw(""))).alignment(Alignment::Left),
                cols[1],
            );
            let list_block = Block::default().borders(Borders::ALL).title(Span::styled(
                "Opponent Info",
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            ));
            let list_inner = list_block.inner(sub[2]);
            let mut list_lines: Vec<Line> = Vec::new();
            if let (Some(name), Some(gw)) = (&app.profile_name, app.gateway) {
                let rating = app
                    .opponent_toons_data
                    .iter()
                    .find(|(t, _, _)| t.eq_ignore_ascii_case(name))
                    .map(|(_, _, r)| *r);
                let race_opt = app.opponent_race.clone();
                let mut parts: Vec<String> = vec![
                    format!("{}", name),
                    crate::api::gateway_label(gw).to_string(),
                ];
                if let Some(race) = race_opt { parts.push(race); }
                if let Some(r) = rating { parts.push(r.to_string()); }
                let mut head = parts.join(" • ");
                // Append history if present
                if let Some(rec) = app.opponent_history.get(&name.to_ascii_lowercase()) {
                    if rec.wins + rec.losses > 0 {
                        head.push_str(&format!(" • W-L {}-{}", rec.wins, rec.losses));
                    }
                    if let (Some(cur), Some(prev)) = (rec.current_rating, rec.previous_rating) {
                        let diff: i32 = (cur as i32) - (prev as i32);
                        let sign = if diff >= 0 { "+" } else { "" };
                        head.push_str(&format!(" • ΔRating {}{}", sign, diff));
                    }
                }
                list_lines.push(Line::from(Span::styled(head, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))));
                for (toon, gw2, r) in app
                    .opponent_toons_data
                    .iter()
                    .filter(|(t, _, _)| !t.eq_ignore_ascii_case(name))
                {
                    list_lines.push(Line::from(Span::raw(format!(
                        "{} • {} • {}",
                        toon,
                        crate::api::gateway_label(*gw2),
                        r
                    ))));
                }
            } else if app.opponent_toons_data.is_empty() {
                list_lines.push(Line::from(Span::styled(
                    "No opponent info yet.",
                    Style::default().fg(Color::DarkGray),
                )));
            } else {
                for (toon, gw2, r) in app.opponent_toons_data.iter() {
                    list_lines.push(Line::from(Span::raw(format!(
                        "{} • {} • {}",
                        toon,
                        crate::api::gateway_label(*gw2),
                        r
                    ))));
                }
            }

            let list_panel = Paragraph::new(list_lines)
                .wrap(Wrap { trim: true })
                .alignment(Alignment::Left)
                .block(list_block);
            app.main_opponent_toons_rect = Some(list_inner);
            frame.render_widget(list_panel, sub[2]);
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
            let area = layout[1];
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(7), Constraint::Min(0)])
                .split(area);

            let gw_label = crate::api::gateway_label(app.search_gateway);
            let name_style = if app.search_focus_gateway { Style::default() } else { Style::default().add_modifier(Modifier::BOLD) };
            let gw_style = if app.search_focus_gateway { Style::default().add_modifier(Modifier::BOLD) } else { Style::default() };
            let name_prefix = if app.search_focus_gateway { "  " } else { "→ " };
            let gw_prefix = if app.search_focus_gateway { "→ " } else { "  " };
            let input_block = Block::default().borders(Borders::ALL).title(Span::styled(
                "Search Input",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ));
            let input_inner = input_block.inner(rows[0]);
            let input = Paragraph::new(vec![
                Line::from(Span::raw("Type name • Tab focus • ←/→ gateway • Enter search • Ctrl+M Main")),
                Line::from(Span::raw("")),
                Line::from(vec![Span::raw(name_prefix), Span::styled("Name: ", Style::default()), Span::styled(app.search_name.clone(), name_style)]),
                Line::from(vec![Span::raw(gw_prefix), Span::styled("Gateway: ", Style::default()), Span::styled(format!("{} ({})", gw_label, app.search_gateway), gw_style)]),
            ])
            .alignment(Alignment::Left)
            .block(input_block);
            frame.render_widget(input, rows[0]);
            if !app.search_focus_gateway {
                // Place terminal cursor at current edit position (hardware blink if supported)
                let prefix_cols = 2u16; // "→ " or two spaces
                let label_cols = "Name: ".len() as u16;
                let name_cols = app.search_cursor.min(app.search_name.chars().count()) as u16;
                let mut x = input_inner.x.saturating_add(prefix_cols).saturating_add(label_cols).saturating_add(name_cols);
                let max_x = input_inner.x.saturating_add(input_inner.width.saturating_sub(1));
                if x > max_x { x = max_x; }
                let y = input_inner.y.saturating_add(2);
                frame.set_cursor_position((x, y));
            }

            let body = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(7), Constraint::Length(6), Constraint::Min(0)])
                .split(rows[1]);

            let mut prof_lines: Vec<Line> = Vec::new();
            let is_self = app
                .self_profile_name
                .as_ref()
                .map(|n| n.eq_ignore_ascii_case(&app.search_name))
                .unwrap_or(false)
                && app.self_gateway == Some(app.search_gateway);
            if is_self {
                prof_lines.push(Line::from(Span::styled(
                    "Self profile — see Main panel.",
                    Style::default().fg(Color::DarkGray),
                )));
            } else {
                let rate_text = app
                    .search_rating
                    .map(|r| format!("Rating: {}", r))
                    .unwrap_or_else(|| "Rating: N/A".to_string());
                prof_lines.push(Line::from(Span::raw(rate_text)));
                if let Some(ref r) = app.search_main_race {
                    prof_lines.push(Line::from(vec![
                        Span::styled("Race: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                        Span::raw(r.clone()),
                    ]));
                }
                if !app.search_matchups.is_empty() {
                    for m in app.search_matchups.iter() {
                        if let Some((label, rest)) = m.split_once(':') {
                            prof_lines.push(Line::from(vec![
                                Span::styled(label.trim(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                                Span::raw(":"),
                                Span::raw(rest.to_string()),
                            ]));
                        } else {
                            prof_lines.push(Line::from(Span::raw(m.clone())));
                        }
                    }
                }
                if let Some(err) = &app.search_error {
                    prof_lines.push(Line::from(Span::styled(
                        format!("Error: {}", err),
                        Style::default().fg(Color::Red),
                    )));
                }
            }
            let profile_panel = Paragraph::new(prof_lines)
                .alignment(Alignment::Left)
                .block(Block::default().borders(Borders::ALL).title(Span::styled(
                    "Profile",
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                )));
            frame.render_widget(profile_panel, body[0]);

            let mut others: Vec<Line> = Vec::new();
            if app.search_other_toons.is_empty() {
                others.push(Line::from(Span::styled("No other toons.", Style::default().fg(Color::DarkGray))));
            } else {
                for item in app.search_other_toons.iter().take(6) { others.push(Line::from(Span::raw(item.clone()))); }
            }
            let others_block = Block::default().borders(Borders::ALL).title(Span::styled(
                "Other Toons",
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            ));
            let others_inner = others_block.inner(body[1]);
            let others_panel = Paragraph::new(others)
                .wrap(Wrap { trim: true })
                .alignment(Alignment::Left)
                .block(others_block);
            app.search_other_toons_rect = Some(others_inner);
            frame.render_widget(others_panel, body[1]);

            let mut matches: Vec<Line> = Vec::new();
            if app.search_matches.is_empty() {
                matches.push(Line::from(Span::styled("No recent matches.", Style::default().fg(Color::DarkGray))));
            } else {
                for m in app.search_matches.iter().take(30) { matches.push(Line::from(Span::raw(m.clone()))); }
            }
            let matches_panel = Paragraph::new(matches)
                .wrap(Wrap { trim: true })
                .scroll((app.search_matches_scroll, 0))
                .alignment(Alignment::Left)
                .block(Block::default().borders(Borders::ALL).title(Span::styled(
                    "Recent Matches",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                )));
            frame.render_widget(matches_panel, body[2]);
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
