use crate::app::App;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

#[allow(clippy::collapsible_if)]
pub fn handle_key_event(app: &mut App, key: KeyEvent) {
    if key.kind != KeyEventKind::Press { return; }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('d') => {
                app.view = match app.view {
                    crate::app::View::Debug => crate::app::View::Main,
                    _ => crate::app::View::Debug,
                };
                if matches!(app.view, crate::app::View::Debug) {
                    app.debug_scroll = 0;
                }
            }
            KeyCode::Char('s') => {
                app.view = crate::app::View::Search;
                app.search_focus_gateway = false;
                app.search_cursor = app.search_name.chars().count();
            }
            KeyCode::Char('m') => { app.view = crate::app::View::Main; }
            KeyCode::Char('q') => { app.should_quit = true; }
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
                if let Some(rect) = app.status_opponent_rect {
                    if x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height {
                        if let (Some(name), Some(gw)) = (&app.profile_name, app.gateway) {
                            app.view = crate::app::View::Search;
                            app.search_name = name.clone();
                            app.search_gateway = gw;
                            app.search_focus_gateway = false;
                            app.search_cursor = app.search_name.chars().count();
                            app.search_matches_scroll = 0;
                            app.search_in_progress = true;
                        }
                    }
                }
                if let Some(rect) = app.main_opponent_toons_rect {
                    if x >= rect.x && y >= rect.y && y < rect.y + rect.height {
                        let idx = (y - rect.y) as usize;
                        if idx == 0 {
                            if let (Some(name), Some(gw)) = (&app.profile_name, app.gateway) {
                                let rating = app
                                    .opponent_toons_data
                                    .iter()
                                    .find(|(t, _, _)| t.eq_ignore_ascii_case(name))
                                    .map(|(_, _, r)| *r);
                                let race_opt = app.opponent_race.clone();
                                let mut parts: Vec<String> = vec![
                                    name.clone(),
                                    crate::api::gateway_label(gw).to_string(),
                                ];
                                if let Some(race) = race_opt { parts.push(race); }
                                if let Some(r) = rating { parts.push(r.to_string()); }
                                let head_text = parts.join(" • ");
                                let head_width = head_text.chars().count() as u16;
                                if x < rect.x + head_width {
                                    app.view = crate::app::View::Search;
                                    app.search_name = name.clone();
                                    app.search_gateway = gw;
                                    app.search_focus_gateway = false;
                                    app.search_cursor = app.search_name.chars().count();
                                    app.search_matches_scroll = 0;
                                    app.search_in_progress = true;
                                }
                            }
                        } else {
                            let others: Vec<(String, u16, u32)> = app
                                .opponent_toons_data
                                .iter()
                                .filter(|(t, _, _)| app.profile_name.as_ref().map(|n| !t.eq_ignore_ascii_case(n)).unwrap_or(true))
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
                                    app.view = crate::app::View::Search;
                                    app.search_name = toon;
                                    app.search_gateway = gw;
                                    app.search_focus_gateway = false;
                                    app.search_cursor = app.search_name.chars().count();
                                    app.search_matches_scroll = 0;
                                    app.search_in_progress = true;
                                }
                            }
                        }
                    }
                }
            }
            crate::app::View::Search => {
                if let Some(rect) = app.search_other_toons_rect {
                    if x >= rect.x && y >= rect.y && y < rect.y + rect.height {
                        let idx = (y - rect.y) as usize;
                        if idx < app.search_other_toons_data.len() {
                            let text_width = app.search_other_toons.get(idx).map(|s| s.chars().count() as u16).unwrap_or(0);
                            if x < rect.x + text_width {
                                let (toon, gw, _r) = app.search_other_toons_data[idx].clone();
                                app.search_name = toon;
                                app.search_gateway = gw;
                                app.search_focus_gateway = false;
                                app.search_cursor = app.search_name.chars().count();
                                app.search_matches_scroll = 0;
                                app.search_in_progress = true;
                            }
                        }
                    }
                }
            }
            crate::app::View::Debug => {}
        }
    }
}
