use crate::app::{App, View};
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

#[allow(clippy::collapsible_if)]
pub fn handle_key_event(app: &mut App, key: KeyEvent) {
    if key.kind != KeyEventKind::Press {
        return;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('d') => {
                app.view = match app.view {
                    crate::app::View::Debug => crate::app::View::Main,
                    _ => crate::app::View::Debug,
                };
                if app.view == View::Debug {
                    app.debug.scroll = 0;
                }
            }
            KeyCode::Char('s') => {
                app.view = crate::app::View::Search;
                app.search.focus_gateway = false;
                app.search.cursor = app.search.name.chars().count();
            }
            KeyCode::Char('m') => {
                app.view = crate::app::View::Main;
            }
            KeyCode::Char('r') => {
                app.view = crate::app::View::Replays;
                if app.replay.toon_input.is_empty() {
                    if let Some(name) = &app.self_profile.name {
                        app.replay.toon_input = name.clone();
                        app.replay.toon_cursor = app.replay.toon_input.chars().count();
                    }
                }
                if let Some(gw) = app.self_profile.gateway {
                    app.replay.input_gateway = gw;
                }
                app.replay.focus = crate::app::ReplayFocus::Toon;
                app.replay.last_error = None;
            }
            KeyCode::Char('p') => {
                app.view = crate::app::View::Players;
                app.players.scroll = 0;
                app.players.search_cursor = app.players.search_query.chars().count();
            }
            KeyCode::Char('q') => {
                app.should_quit = true;
            }
            _ => {}
        }
    } else {
        match key.code {
            KeyCode::Esc => app.should_quit = true,
            other => app.on_key(other),
        }
    }
}

#[allow(clippy::collapsible_if)]
pub fn handle_mouse_event(app: &mut App, me: MouseEvent) {
    if let MouseEventKind::Down(MouseButton::Left) = me.kind {
        let x = me.column;
        let y = me.row;
        match app.view {
            crate::app::View::Main => {
                if let Some(rect) = app.layout.status_opponent_rect {
                    if x >= rect.x
                        && x < rect.x + rect.width
                        && y >= rect.y
                        && y < rect.y + rect.height
                    {
                        if let (Some(name), Some(gw)) = (&app.opponent.name, app.opponent.gateway) {
                            app.begin_search(name.clone(), gw);
                        }
                    }
                }
                if let Some(rect) = app.layout.main_opponent_toons_rect {
                    if x >= rect.x && y >= rect.y && y < rect.y + rect.height {
                        let idx = (y - rect.y) as usize;
                        if idx == 0 {
                            if let (Some(name), Some(gw)) =
                                (&app.opponent.name, app.opponent.gateway)
                            {
                                let rating = app
                                    .opponent
                                    .toons_data
                                    .iter()
                                    .find(|(t, _, _)| t.eq_ignore_ascii_case(name))
                                    .map(|(_, _, r)| *r);
                                let race_opt = app.opponent.race.clone();
                                let mut parts: Vec<String> =
                                    vec![name.clone(), crate::api::gateway_label(gw).to_string()];
                                if let Some(race) = race_opt {
                                    parts.push(race);
                                }
                                if let Some(r) = rating {
                                    parts.push(r.to_string());
                                }
                                let head_text = parts.join(" • ");
                                let head_width = head_text.chars().count() as u16;
                                if x < rect.x + head_width {
                                    app.begin_search(name.clone(), gw);
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
                                    let (toon, gw, _r) = others[sel].clone();
                                    app.begin_search(toon, gw);
                                }
                            }
                        }
                    }
                }
            }
            crate::app::View::Search => {
                if let Some(rect) = app.search.other_toons_rect {
                    if x >= rect.x && y >= rect.y && y < rect.y + rect.height {
                        let idx = (y - rect.y) as usize;
                        if idx < app.search.other_toons_data.len() {
                            let text_width = app
                                .search
                                .other_toons
                                .get(idx)
                                .map(|s| s.chars().count() as u16)
                                .unwrap_or(0);
                            if x < rect.x + text_width {
                                let (toon, gw, _r) = app.search.other_toons_data[idx].clone();
                                app.begin_search(toon, gw);
                            }
                        }
                    }
                }
            }
            crate::app::View::Debug => {}
            crate::app::View::Replays => {}
            crate::app::View::Players => {}
        }
    }
}
