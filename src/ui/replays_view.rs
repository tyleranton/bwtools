use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::{App, ReplayFocus};

pub fn render_replays(frame: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(0)])
        .split(area);

    let focus_indicator = |focus: ReplayFocus, target: ReplayFocus| {
        if focus == target { "→ " } else { "  " }
    };

    let gw_label = crate::api::gateway_label(app.replay_input_gateway);
    let mut info_lines = vec![Line::from(Span::raw(
        "Ctrl+M Main  •  Ctrl+S Search  •  Enter Start Download",
    ))];
    info_lines.push(Line::from(Span::raw("")));

    info_lines.push(Line::from(vec![
        Span::raw(focus_indicator(app.replay_focus, ReplayFocus::Toon)),
        Span::styled("Profile: ", Style::default()),
        Span::styled(
            app.replay_toon_input.clone(),
            if matches!(app.replay_focus, ReplayFocus::Toon) {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            },
        ),
    ]));

    info_lines.push(Line::from(vec![
        Span::raw(focus_indicator(app.replay_focus, ReplayFocus::Gateway)),
        Span::styled("Gateway: ", Style::default()),
        Span::styled(
            format!("{} ({})", gw_label, app.replay_input_gateway),
            if matches!(app.replay_focus, ReplayFocus::Gateway) {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            },
        ),
    ]));

    info_lines.push(Line::from(vec![
        Span::raw(focus_indicator(app.replay_focus, ReplayFocus::Matchup)),
        Span::styled("Matchup: ", Style::default()),
        Span::styled(
            app.replay_matchup_input.clone(),
            if matches!(app.replay_focus, ReplayFocus::Matchup) {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            },
        ),
    ]));

    info_lines.push(Line::from(vec![
        Span::raw(focus_indicator(app.replay_focus, ReplayFocus::Count)),
        Span::styled("Count: ", Style::default()),
        Span::styled(
            app.replay_input_count.to_string(),
            if matches!(app.replay_focus, ReplayFocus::Count) {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            },
        ),
    ]));

    if let Some(err) = &app.replay_last_error {
        info_lines.push(Line::from(Span::styled(
            format!("Error: {}", err),
            Style::default().fg(Color::Red),
        )));
    } else if app.replay_in_progress {
        info_lines.push(Line::from(Span::styled(
            "Downloading replays...",
            Style::default().fg(Color::Yellow),
        )));
    }

    let input_block_base = Block::default().borders(Borders::ALL).title(Span::styled(
        "Replay Download",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));
    let input_inner = input_block_base.inner(rows[0]);

    let input_block = Paragraph::new(info_lines)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true })
        .block(input_block_base);

    frame.render_widget(input_block, rows[0]);

    let mut summary_lines: Vec<Line> = Vec::new();
    if let Some(summary) = &app.replay_last_summary {
        summary_lines.push(Line::from(Span::styled(
            "Last download",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )));
        summary_lines.push(Line::from(Span::raw(format!(
            "Requested: {}  Saved: {}  Skipped: {}  Filtered: {}",
            summary.requested, summary.saved, summary.skipped_existing, summary.filtered_short
        ))));
        if !summary.errors.is_empty() {
            summary_lines.push(Line::from(Span::styled(
                "Errors:",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )));
            for err in summary.errors.iter().take(5) {
                summary_lines.push(Line::from(Span::styled(
                    format!("- {}", err),
                    Style::default().fg(Color::Red),
                )));
            }
        }
        if !summary.saved_paths.is_empty() {
            summary_lines.push(Line::from(Span::styled(
                "Saved:",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )));
            for path in summary.saved_paths.iter().take(5) {
                summary_lines.push(Line::from(Span::raw(format!("- {}", path.display()))));
            }
            if summary.saved_paths.len() > 5 {
                summary_lines.push(Line::from(Span::styled(
                    format!("… {} more", summary.saved_paths.len() - 5),
                    Style::default().fg(Color::Gray),
                )));
            }
        }
    } else if app.replay_in_progress {
        summary_lines.push(Line::from(Span::styled(
            "Download in progress…",
            Style::default().fg(Color::Yellow),
        )));
    } else {
        summary_lines.push(Line::from(Span::styled(
            "No downloads yet. Press Enter to begin.",
            Style::default().fg(Color::DarkGray),
        )));
    }

    if let Some(req) = &app.replay_last_request {
        summary_lines.push(Line::from(Span::raw("")));
        summary_lines.push(Line::from(Span::styled(
            format!(
                "Last request: {} @ {} ({})",
                req.toon,
                crate::api::gateway_label(req.gateway),
                req.matchup.clone().unwrap_or_else(|| "All".to_string())
            ),
            Style::default().fg(Color::Gray),
        )));
    }

    let summary_block_base = Block::default().borders(Borders::ALL).title(Span::styled(
        "Status",
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    ));

    let summary_block = Paragraph::new(summary_lines)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true })
        .block(summary_block_base);

    frame.render_widget(summary_block, rows[1]);

    if matches!(app.replay_focus, ReplayFocus::Toon) {
        let cursor_x = input_inner.x + 2 + "Profile: ".len() as u16 + app.replay_toon_cursor as u16;
        let cursor_y = input_inner.y + 2;
        frame.set_cursor_position((cursor_x, cursor_y));
    } else if matches!(app.replay_focus, ReplayFocus::Matchup) {
        let cursor_x =
            input_inner.x + 2 + "Matchup: ".len() as u16 + app.replay_matchup_cursor as u16;
        let cursor_y = input_inner.y + 4;
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}
