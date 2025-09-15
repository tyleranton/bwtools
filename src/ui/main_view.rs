use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::App;

pub fn render_main(frame: &mut ratatui::Frame, area: ratatui::layout::Rect, app: &mut App) {
    let sub = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(7), Constraint::Min(0)])
        .split(area);

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
        let race_opt = app.opponent_race.as_deref();
        let mut head = crate::ui::display::opponent_header(
            name,
            crate::api::gateway_label(gw),
            race_opt,
            rating,
        );
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
            list_lines.push(Line::from(Span::raw(
                crate::ui::display::toon_line(toon, crate::api::gateway_label(*gw2), *r),
            )));
        }
    } else if app.opponent_toons_data.is_empty() {
        list_lines.push(Line::from(Span::styled(
            "No opponent info yet.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (toon, gw2, r) in app.opponent_toons_data.iter() {
            list_lines.push(Line::from(Span::raw(
                crate::ui::display::toon_line(toon, crate::api::gateway_label(*gw2), *r),
            )));
        }
    }

    let list_panel = Paragraph::new(list_lines)
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Left)
        .block(list_block);
    app.main_opponent_toons_rect = Some(list_inner);
    frame.render_widget(list_panel, sub[2]);
}
