use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::App;
use crate::interaction::Intent;
use crate::ui::profile_stats::profile_stat_lines;

pub fn render_main(frame: &mut ratatui::Frame, area: ratatui::layout::Rect, app: &mut App) {
    let segments = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    let stats_block = Block::default().borders(Borders::ALL).title(Span::styled(
        "Profile Stats",
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    ));
    let stats_area = segments[0];
    let stats_inner = stats_block.inner(stats_area);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Min(10)])
        .split(stats_inner);

    let stats_lines = profile_stat_lines(
        app.self_profile.rating,
        app.self_profile.main_race.as_deref(),
        &app.self_profile.matchups,
        Some((
            app.self_profile.self_dodged,
            app.self_profile.opponent_dodged,
        )),
    );
    frame.render_widget(stats_block, stats_area);
    frame.render_widget(
        Paragraph::new(stats_lines).alignment(Alignment::Left),
        cols[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::raw(""))).alignment(Alignment::Left),
        cols[1],
    );

    let opponent_block = Block::default().borders(Borders::ALL).title(Span::styled(
        "Opponent Info",
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    ));
    let opponent_area = segments[1];
    let inner = opponent_block.inner(opponent_area);
    frame.render_widget(opponent_block, opponent_area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    let mut opponent_profile_lines: Vec<Line> = Vec::new();
    let mut other_toons_lines: Vec<Line> = Vec::new();

    if let (Some(name), Some(gw)) = (&app.opponent.name, app.opponent.gateway) {
        let rating = app
            .opponent
            .toons_data
            .iter()
            .find(|(t, _, _)| t.eq_ignore_ascii_case(name))
            .map(|(_, _, r)| *r);
        let race_opt = app.opponent.race.as_deref();
        let header = crate::ui::display::opponent_header(
            name,
            crate::api::gateway_label(gw),
            race_opt,
            rating,
        );
        opponent_profile_lines.push(Line::from(Span::styled(
            header,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));

        if let Some(rec) = app.opponent.history.get(&name.to_ascii_lowercase())
            && rec.wins + rec.losses > 0
        {
            opponent_profile_lines.push(Line::from(vec![
                Span::styled(
                    "Record: ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("{}-{}", rec.wins, rec.losses)),
            ]));
        }

        let mut matchup_lines = profile_stat_lines(
            rating,
            app.opponent.race.as_deref(),
            &app.opponent.matchups,
            None,
        );
        if !matchup_lines.is_empty() {
            matchup_lines.remove(0); // drop rating/race line (already in header)
        }
        if !matchup_lines.is_empty() {
            opponent_profile_lines.push(Line::raw(String::new()));
            opponent_profile_lines.push(Line::from(Span::styled(
                "Season winrates",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )));
            opponent_profile_lines.extend(matchup_lines);
        }

        other_toons_lines.push(Line::from(Span::styled(
            "Other toons",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));
        if app
            .opponent
            .toons_data
            .iter()
            .any(|(t, _, _)| !t.eq_ignore_ascii_case(name))
        {
            for (toon, gw2, r) in app
                .opponent
                .toons_data
                .iter()
                .filter(|(t, _, _)| !t.eq_ignore_ascii_case(name))
            {
                other_toons_lines.push(Line::from(Span::styled(
                    crate::ui::display::toon_line(toon, crate::api::gateway_label(*gw2), *r),
                    Style::default().fg(Color::Gray),
                )));
            }
        } else {
            other_toons_lines.push(Line::from(Span::styled(
                "No other toons.",
                Style::default().fg(Color::DarkGray),
            )));
        }
    } else if app.opponent.toons_data.is_empty() {
        opponent_profile_lines.push(Line::from(Span::styled(
            "No opponent info yet.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        other_toons_lines.push(Line::from(Span::styled(
            "Possible toons",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));
        for (toon, gw2, r) in app.opponent.toons_data.iter() {
            other_toons_lines.push(Line::from(Span::styled(
                crate::ui::display::toon_line(toon, crate::api::gateway_label(*gw2), *r),
                Style::default().fg(Color::Gray),
            )));
        }
    }

    let opponent_profile = Paragraph::new(opponent_profile_lines).wrap(Wrap { trim: true });
    frame.render_widget(opponent_profile, columns[0]);

    let other_toons = Paragraph::new(other_toons_lines).wrap(Wrap { trim: true });
    app.layout.main_opponent_toons_rect = Some(columns[1]);
    frame.render_widget(other_toons, columns[1]);

    let hotkey_line = Line::from(Span::styled(
        "Ctrl+S Search  •  Ctrl+D Debug  •  Ctrl+R Replays  •  Ctrl+Q/Esc Quit",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM),
    ));
    let hotkeys = Paragraph::new(hotkey_line)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(hotkeys, segments[2]);
}

pub fn intent_at(app: &App, x: u16, y: u16) -> Option<Intent> {
    if let Some(rect) = app.layout.status_opponent_rect
        && x >= rect.x
        && x < rect.x + rect.width
        && y >= rect.y
        && y < rect.y + rect.height
        && let (Some(name), Some(gw)) = (&app.opponent.name, app.opponent.gateway)
    {
        return Some(Intent::BeginSearch {
            name: name.clone(),
            gateway: gw,
        });
    }

    if let Some(rect) = app.layout.main_opponent_toons_rect
        && x >= rect.x
        && y >= rect.y
        && y < rect.y + rect.height
    {
        let idx = (y - rect.y) as usize;
        if idx == 0 {
            if let (Some(name), Some(gw)) = (&app.opponent.name, app.opponent.gateway) {
                let rating = app
                    .opponent
                    .toons_data
                    .iter()
                    .find(|(t, _, _)| t.eq_ignore_ascii_case(name))
                    .map(|(_, _, r)| *r);
                let race_opt = app.opponent.race.as_ref();
                let mut parts = vec![name.clone(), crate::api::gateway_label(gw).to_string()];
                if let Some(race) = race_opt {
                    parts.push(race.clone());
                }
                if let Some(r) = rating {
                    parts.push(r.to_string());
                }
                let head_text = parts.join(" • ");
                let head_width = head_text.chars().count() as u16;
                if x < rect.x + head_width {
                    return Some(Intent::BeginSearch {
                        name: name.clone(),
                        gateway: gw,
                    });
                }
            }
        } else {
            let others: Vec<(String, u16, u32)> = app
                .opponent
                .toons_data
                .iter()
                .filter(|(t, _, _)| {
                    app.opponent
                        .name
                        .as_ref()
                        .map(|n| !t.eq_ignore_ascii_case(n))
                        .unwrap_or(true)
                })
                .cloned()
                .collect();
            let sel = idx - 1;
            if sel < others.len() {
                let display_text = format!(
                    "{} • {} • {}",
                    others[sel].0,
                    crate::api::gateway_label(others[sel].1),
                    others[sel].2
                );
                let text_width = display_text.chars().count() as u16;
                if x < rect.x + text_width {
                    let (toon, gw, _) = others[sel].clone();
                    return Some(Intent::BeginSearch {
                        name: toon,
                        gateway: gw,
                    });
                }
            }
        }
    }

    None
}
