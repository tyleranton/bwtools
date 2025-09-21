use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::App;
use crate::ui::profile_stats::profile_stat_lines;

pub fn render_search(frame: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(0)])
        .split(area);

    let gw_label = crate::api::gateway_label(app.search.gateway);
    let name_style = if app.search.focus_gateway {
        Style::default()
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };
    let gw_style = if app.search.focus_gateway {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let name_prefix = if app.search.focus_gateway {
        "  "
    } else {
        "→ "
    };
    let gw_prefix = if app.search.focus_gateway {
        "→ "
    } else {
        "  "
    };
    let input_block = Block::default().borders(Borders::ALL).title(Span::styled(
        "Search Input",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ));
    let input_inner = input_block.inner(rows[0]);
    let input = Paragraph::new(vec![
        Line::from(Span::raw(
            "Type name • Tab focus • ←/→ gateway • Enter search • Ctrl+M Main",
        )),
        Line::from(Span::raw("")),
        Line::from(vec![
            Span::raw(name_prefix),
            Span::styled("Name: ", Style::default()),
            Span::styled(app.search.name.clone(), name_style),
        ]),
        Line::from(vec![
            Span::raw(gw_prefix),
            Span::styled("Gateway: ", Style::default()),
            Span::styled(format!("{} ({})", gw_label, app.search.gateway), gw_style),
        ]),
    ])
    .alignment(Alignment::Left)
    .block(input_block);
    frame.render_widget(input, rows[0]);
    if !app.search.focus_gateway {
        let prefix_cols = 2u16;
        let label_cols = "Name: ".len() as u16;
        let name_cols = app.search.cursor.min(app.search.name.chars().count()) as u16;
        let mut x = input_inner
            .x
            .saturating_add(prefix_cols)
            .saturating_add(label_cols)
            .saturating_add(name_cols);
        let max_x = input_inner
            .x
            .saturating_add(input_inner.width.saturating_sub(1));
        if x > max_x {
            x = max_x;
        }
        let y = input_inner.y.saturating_add(2);
        frame.set_cursor_position((x, y));
    }

    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(0)])
        .split(rows[1]);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(body[0]);

    let mut prof_lines: Vec<Line> = Vec::new();
    let is_self = app
        .self_profile_name
        .as_ref()
        .map(|n| n.eq_ignore_ascii_case(&app.search.name))
        .unwrap_or(false)
        && app.self_gateway == Some(app.search.gateway);
    if is_self {
        prof_lines.push(Line::from(Span::styled(
            "Self profile — see Main panel.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        prof_lines = profile_stat_lines(
            app.search.rating,
            app.search.main_race.as_deref(),
            &app.search.matchups,
        );
        if let Some(err) = &app.search.error {
            prof_lines.push(Line::from(Span::styled(
                format!("Error: {}", err),
                Style::default().fg(Color::Red),
            )));
        }
    }
    let profile_panel = Paragraph::new(prof_lines).alignment(Alignment::Left).block(
        Block::default().borders(Borders::ALL).title(Span::styled(
            "Profile",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
    );
    frame.render_widget(profile_panel, top[0]);

    let mut others: Vec<Line> = Vec::new();
    if app.search.other_toons.is_empty() {
        others.push(Line::from(Span::styled(
            "No other toons.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for item in app.search.other_toons.iter().take(6) {
            others.push(Line::from(Span::raw(item.clone())));
        }
    }
    let others_block = Block::default().borders(Borders::ALL).title(Span::styled(
        "Other Toons",
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    ));
    let others_inner = others_block.inner(top[1]);
    let others_panel = Paragraph::new(others)
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Left)
        .block(others_block);
    app.search.other_toons_rect = Some(others_inner);
    frame.render_widget(others_panel, top[1]);

    let mut matches: Vec<Line> = Vec::new();
    if app.search.matches.is_empty() {
        matches.push(Line::from(Span::styled(
            "No recent matches.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for m in app.search.matches.iter().take(30) {
            matches.push(Line::from(Span::raw(m.clone())));
        }
    }
    let matches_panel = Paragraph::new(matches)
        .wrap(Wrap { trim: true })
        .scroll((app.search.matches_scroll, 0))
        .alignment(Alignment::Left)
        .block(
            Block::default().borders(Borders::ALL).title(Span::styled(
                "Recent Matches",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
        );
    frame.render_widget(matches_panel, body[1]);
}
