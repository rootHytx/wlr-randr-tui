use ncurses::*;

use crate::app::{App, Section, UiMode};
use crate::apply::do_apply;
use crate::consts::*;
use crate::monitor::{pos_cycle_list, resolve_abs, PosMode};
use crate::parse::parse_monitors;
use crate::proj::{apply_proj, Proj};

pub fn handle(app: &mut App, ch: i32) -> bool {
    if app.ui == UiMode::TextEdit { return handle_text(app, ch); }
    if matches!(app.ui, UiMode::ModeSelect { .. }) { return handle_mode_select(app, ch); }
    handle_normal(app, ch)
}

fn handle_normal(app: &mut App, ch: i32) -> bool {
    match ch {
        113 => return false, // q

        // Tab cycles sections: Settings -> Layout -> Tabs -> Settings
        9 => {
            app.section = match app.section {
                Section::Tabs     => Section::Settings,
                Section::Settings => Section::Layout,
                Section::Layout   => Section::Tabs,
            };
            app.status.clear();
        }

        KEY_UP => {
            app.status.clear();
            match app.section {
                Section::Tabs     => {}
                Section::Settings => { app.sel_field = app.sel_field.saturating_sub(1); }
                Section::Layout   => { if app.layout_sel > 0 { app.layout_sel -= 1; } }
            }
        }
        KEY_DOWN => {
            app.status.clear();
            match app.section {
                Section::Tabs     => { app.section = Section::Settings; }
                Section::Settings => { if app.sel_field + 1 < N_FIELDS { app.sel_field += 1; } }
                Section::Layout   => { if app.layout_sel + 1 < app.monitors.len() { app.layout_sel += 1; } }
            }
        }
        KEY_LEFT => {
            app.status.clear();
            match app.section {
                Section::Tabs     => { if app.tab > 0 { app.tab -= 1; } }
                Section::Settings => { cycle_setting(app, -1); }
                Section::Layout   => { cycle_pos(app, -1); }
            }
        }
        KEY_RIGHT => {
            app.status.clear();
            match app.section {
                Section::Tabs     => { if app.tab + 1 < app.monitors.len() { app.tab += 1; } }
                Section::Settings => { cycle_setting(app, 1); }
                Section::Layout   => { cycle_pos(app, 1); }
            }
        }

        10 | KEY_ENTER => {
            app.status.clear();
            match app.section {
                Section::Tabs => { app.section = Section::Settings; }
                Section::Settings => match app.sel_field {
                    F_MODE => {
                        let h = app.cur_mon().new_mode;
                        app.ui = UiMode::ModeSelect { scroll: h.saturating_sub(5), hover: h };
                    }
                    F_SCALE => {
                        app.buf = format!("{:.4}", app.cur_mon().new_scale);
                        app.ui  = UiMode::TextEdit;
                    }
                    F_ADAPTIVE => { let v = app.cur_mon().new_adaptive; app.cur_mon_mut().new_adaptive = !v; }
                    F_ENABLED  => { let v = app.cur_mon().new_enabled;  app.cur_mon_mut().new_enabled  = !v; }
                    _ => {}
                },
                Section::Layout => {
                    let mon = &app.monitors[app.layout_sel];
                    app.buf = match &mon.new_pos {
                        PosMode::Absolute(x, y) => format!("{},{}", x, y),
                        _ => {
                            let (x, y) = resolve_abs(&mon.new_pos, mon, &app.monitors, 0);
                            format!("{},{}", x, y)
                        }
                    };
                    app.ui = UiMode::TextEdit;
                }
            }
        }

        27 => { app.ui = UiMode::Normal; app.status.clear(); }

        112 | 46 => { // p or . : projection forward
            let p = app.proj.step(1);
            app.proj = p;
            apply_proj(&mut app.monitors, p);
            app.set_status(format!("Projection: {}", p.label()), false);
        }
        44 => { // , : projection backward
            let p = app.proj.step(-1);
            app.proj = p;
            apply_proj(&mut app.monitors, p);
            app.set_status(format!("Projection: {}", p.label()), false);
        }

        97 => { // a: apply
            match do_apply(&app.monitors) {
                Ok(msg) => {
                    let fresh = parse_monitors();
                    app.tab        = app.tab.min(fresh.len().saturating_sub(1));
                    app.layout_sel = app.layout_sel.min(fresh.len().saturating_sub(1));
                    app.monitors   = fresh;
                    app.proj       = Proj::Custom;
                    app.set_status(msg, false);
                }
                Err(e) => app.set_status(e, true),
            }
        }

        114 => { // r: reset current tab's monitor
            let name = app.cur_mon().name.clone();
            app.cur_mon_mut().reset();
            app.set_status(format!("Reset {}", name), false);
        }

        _ => {}
    }
    true
}

fn cycle_setting(app: &mut App, dir: i32) {
    let m = &mut app.monitors[app.tab];
    match app.sel_field {
        F_MODE => {
            let n = m.modes.len() as i32;
            m.new_mode = ((m.new_mode as i32 + dir).rem_euclid(n)) as usize;
        }
        F_SCALE => {
            m.new_scale = (m.new_scale + dir as f64 * 0.25).clamp(0.25, 4.0);
            m.new_scale = (m.new_scale * 4.0).round() / 4.0;
        }
        F_TRANSFORM => {
            let i = TRANSFORMS.iter().position(|&t| t == m.new_transform).unwrap_or(0);
            m.new_transform = TRANSFORMS[((i as i32 + dir).rem_euclid(TRANSFORMS.len() as i32)) as usize].to_string();
        }
        F_ADAPTIVE => m.new_adaptive = !m.new_adaptive,
        F_ENABLED  => m.new_enabled  = !m.new_enabled,
        _ => {}
    }
}

fn cycle_pos(app: &mut App, dir: i32) {
    let idx      = app.layout_sel;
    let mon_name = app.monitors[idx].name.clone();
    let others: Vec<String> = app.monitors.iter()
        .filter(|m| m.name != mon_name)
        .map(|m| m.name.clone())
        .collect();
    let m    = &mut app.monitors[idx];
    let opts = pos_cycle_list(&m.new_pos, &others);
    let cur  = opts.iter().position(|o| *o == m.new_pos).unwrap_or(0);
    let next = ((cur as i32 + dir).rem_euclid(opts.len() as i32)) as usize;
    m.new_pos = opts[next].clone();
}

fn handle_mode_select(app: &mut App, ch: i32) -> bool {
    let n = app.monitors[app.tab].modes.len();
    let mut rows = 0i32; let mut _c = 0i32;
    getmaxyx(stdscr(), &mut rows, &mut _c);
    let visible = (rows - 3 - 2).max(1) as usize;
    let (mut scroll, mut hover) = match app.ui {
        UiMode::ModeSelect { scroll, hover } => (scroll, hover),
        _ => return true,
    };
    match ch {
        KEY_UP   => { if hover > 0     { hover -= 1; if hover < scroll { scroll = hover; } } }
        KEY_DOWN => { if hover + 1 < n { hover += 1; if hover >= scroll + visible { scroll = hover + 1 - visible; } } }
        10 | KEY_ENTER => { app.monitors[app.tab].new_mode = hover; app.ui = UiMode::Normal; return true; }
        27             => { app.ui = UiMode::Normal; return true; }
        _ => {}
    }
    app.ui = UiMode::ModeSelect { scroll, hover };
    true
}

fn handle_text(app: &mut App, ch: i32) -> bool {
    match ch {
        27 => { app.buf.clear(); app.ui = UiMode::Normal; }
        10 | KEY_ENTER => {
            let buf = app.buf.clone();
            app.buf.clear();
            app.ui = UiMode::Normal;
            match app.section {
                Section::Settings if app.sel_field == F_SCALE => {
                    match buf.trim().parse::<f64>() {
                        Ok(v) if v > 0.0 && v <= 4.0 => app.cur_mon_mut().new_scale = v,
                        Ok(_)  => app.set_status("Scale: 0.25 to 4.0", true),
                        Err(_) => app.set_status("Invalid number", true),
                    }
                }
                Section::Layout => {
                    let p: Vec<&str> = buf.splitn(2, ',').collect();
                    if p.len() == 2 {
                        match (p[0].trim().parse::<i32>(), p[1].trim().parse::<i32>()) {
                            (Ok(x), Ok(y)) => app.monitors[app.layout_sel].new_pos = PosMode::Absolute(x, y),
                            _              => app.set_status("Invalid -- use: x,y", true),
                        }
                    } else {
                        app.set_status("Format: x,y  e.g. 3440,0", true);
                    }
                }
                _ => {}
            }
        }
        KEY_BACKSPACE | 127 | 8 => { app.buf.pop(); }
        c if c >= 32 && c < 127  => app.buf.push(c as u8 as char),
        _ => {}
    }
    true
}
