use ncurses::*;

use crate::app::{App, Section, UiMode};
use crate::consts::*;
use crate::monitor::{logical_size, resolve_abs, PosMode};
use crate::proj::Proj;

pub fn pad(s: &str, w: usize) -> String {
    let s = if s.len() > w { &s[..w] } else { s };
    format!("{:<w$}", s, w = w)
}

pub fn draw(app: &App) {
    let mut rows = 0i32;
    let mut cols = 0i32;
    getmaxyx(stdscr(), &mut rows, &mut cols);
    clear();
    let w = cols as usize;

    attron(COLOR_PAIR(CP_HEADER) | A_BOLD());
    mvprintw(0, 0, &pad(" wlr-randr TUI", w));
    attroff(COLOR_PAIR(CP_HEADER) | A_BOLD());

    draw_tabs(app, 1, cols);
    mvprintw(2, 0, &"-".repeat(w));

    // Mode-select overlay replaces everything below the tab bar
    if let UiMode::ModeSelect { scroll, hover } = app.ui {
        draw_mode_overlay(app, scroll, hover, rows, cols);
        draw_statusbar(app, rows, cols);
        refresh();
        return;
    }

    let mon = app.cur_mon();
    let info = format!(
        "  {}  {}  {}",
        mon.name,
        if mon.make.is_empty() {
            String::new()
        } else {
            format!("{} {}", mon.make, mon.model)
        },
        if mon.is_laptop() {
            "[laptop]"
        } else {
            "[external]"
        }
    );
    mvprintw(3, 0, &pad(&info, w));
    mvprintw(4, 0, &"-".repeat(w));

    let settings_top = 5i32;
    for f in 0..N_FIELDS {
        let y = settings_top + f as i32;
        if y >= rows - 1 {
            break;
        }
        let sel = app.section == Section::Settings && f == app.sel_field;
        let val = if sel && app.ui == UiMode::TextEdit {
            format!("[{}]_", app.buf)
        } else {
            mon.field_value(f)
        };
        let line = format!(
            " {} {:<14} {}",
            if sel { ">" } else { " " },
            format!("{}:", FIELD_NAMES[f]),
            val
        );
        if sel {
            attron(COLOR_PAIR(CP_SELECTED) | A_BOLD());
        } else if mon.field_dirty(f) {
            attron(COLOR_PAIR(CP_MODIFIED));
        }
        mvprintw(y, 0, &pad(&line, w));
        if sel {
            attroff(COLOR_PAIR(CP_SELECTED) | A_BOLD());
        } else if mon.field_dirty(f) {
            attroff(COLOR_PAIR(CP_MODIFIED));
        }
    }

    let layout_div = settings_top + N_FIELDS as i32;
    if layout_div < rows - 1 {
        mvprintw(layout_div, 0, &"-".repeat(w));
    }

    let layout_top = layout_div + 1;
    if layout_top < rows - 1 {
        draw_layout_section(app, layout_top, rows, cols);
    }

    draw_statusbar(app, rows, cols);
    refresh();
}

fn draw_tabs(app: &App, y: i32, cols: i32) {
    mvprintw(y, 0, &" ".repeat(cols as usize));
    let mut x = 1i32;
    for (i, mon) in app.monitors.iter().enumerate() {
        let mark = if mon.dirty() { "*" } else { " " };
        let label = format!("[{}{}]", mon.name, mark);
        if x + label.len() as i32 + 1 >= cols {
            break;
        }
        let is_active = i == app.tab;
        let is_focused = is_active && app.section == Section::Tabs;
        if is_focused {
            attron(COLOR_PAIR(CP_SELECTED) | A_BOLD());
        } else if is_active {
            attron(A_BOLD());
        }
        mvprintw(y, x, &label);
        if is_focused {
            attroff(COLOR_PAIR(CP_SELECTED) | A_BOLD());
        } else if is_active {
            attroff(A_BOLD());
        }
        x += label.len() as i32 + 1;
    }
    let hint = "Tab:focus";
    if hint.len() as i32 + 2 < cols {
        attron(COLOR_PAIR(CP_HEADER));
        mvprintw(y, cols - hint.len() as i32 - 1, hint);
        attroff(COLOR_PAIR(CP_HEADER));
    }
}

fn draw_layout_section(app: &App, top: i32, rows: i32, cols: i32) {
    let w = cols as usize;

    // Header: projection preset row
    mvprintw(top, 0, &" ".repeat(w));
    let hdr = "Layout  Proj:";
    mvprintw(top, 1, hdr);
    let mut x = 1 + hdr.len() as i32 + 1;
    for p in Proj::ALL {
        let s = format!("[{}]", p.label());
        if *p == app.proj {
            attron(COLOR_PAIR(CP_SELECTED) | A_BOLD());
        }
        mvprintw(top, x, &s);
        if *p == app.proj {
            attroff(COLOR_PAIR(CP_SELECTED) | A_BOLD());
        }
        x += s.len() as i32 + 1;
    }
    let hint = "p/,/.:cycle";
    if x + hint.len() as i32 + 1 < cols {
        attron(COLOR_PAIR(CP_HEADER));
        mvprintw(top, x + 1, hint);
        attroff(COLOR_PAIR(CP_HEADER));
    }

    // Single position line for the currently focused monitor
    let pos_y = top + 1;
    if pos_y < rows - 1 && !app.monitors.is_empty() {
        let mon = &app.monitors[app.layout_sel];
        let in_layout = app.section == Section::Layout;
        let pos_str = if in_layout && app.ui == UiMode::TextEdit {
            format!("[{}]_", app.buf)
        } else {
            match &mon.new_pos {
                PosMode::Absolute(px, py) => format!("x={} y={}", px, py),
                rel => {
                    let (ax, ay) = resolve_abs(&mon.new_pos, mon, &app.monitors, 0);
                    format!("{}  -> ({}, {})", rel.display(), ax, ay)
                }
            }
        };
        let mark = if mon.pos_dirty() { " [*]" } else { "" };
        let nav = if app.monitors.len() > 1 {
            format!(" ({}/{})", app.layout_sel + 1, app.monitors.len())
        } else {
            String::new()
        };
        let line = format!(
            " {} {:<12} {}{}",
            if in_layout { ">" } else { " " },
            format!("{}{}:", mon.name, nav),
            pos_str,
            mark
        );
        if in_layout {
            attron(COLOR_PAIR(CP_SELECTED) | A_BOLD());
        } else if mon.pos_dirty() {
            attron(COLOR_PAIR(CP_MODIFIED));
        }
        mvprintw(pos_y, 0, &pad(&line, w));
        if in_layout {
            attroff(COLOR_PAIR(CP_SELECTED) | A_BOLD());
        } else if mon.pos_dirty() {
            attroff(COLOR_PAIR(CP_MODIFIED));
        }
    }

    let diag_top = pos_y + 1;
    if diag_top < rows - 1 {
        draw_layout_diagram(app, diag_top, rows, cols);
    }
}

fn draw_layout_diagram(app: &App, start_y: i32, rows: i32, cols: i32) {
    let avail_h = (rows - 1 - start_y).max(0) as usize;
    if avail_h < 3 || cols < 10 {
        return;
    }

    let sel_name = if app.section == Section::Layout {
        app.monitors[app.layout_sel].name.clone()
    } else {
        app.cur_mon().name.clone()
    };

    let layout: Vec<(String, i32, i32, i32, i32, bool)> = app
        .monitors
        .iter()
        .map(|m| {
            let (ax, ay) = resolve_abs(&m.new_pos, m, &app.monitors, 0);
            let (lw, lh) = logical_size(m);
            (m.name.clone(), ax, ay, lw, lh, m.name == sel_name)
        })
        .collect();

    let min_x = layout.iter().map(|e| e.1).min().unwrap_or(0);
    let min_y = layout.iter().map(|e| e.2).min().unwrap_or(0);
    let max_x = layout.iter().map(|e| e.1 + e.3).max().unwrap_or(1);
    let max_y = layout.iter().map(|e| e.2 + e.4).max().unwrap_or(1);
    let total_w = (max_x - min_x).max(1) as f64;
    let total_h = (max_y - min_y).max(1) as f64;

    let avail_w = (cols as f64 - 2.0).max(4.0);
    let avail_hf = (avail_h as f64).max(1.0);
    let scale = (avail_w / total_w)
        .min((avail_hf / total_h) * 2.0)
        .min(avail_hf / total_h);

    for (name, mx, my, mw, mh, is_sel) in &layout {
        let bx = ((*mx - min_x) as f64 * scale).round() as i32;
        let by = ((*my - min_y) as f64 * scale).round() as i32;
        let bw = ((*mw as f64) * scale).round() as i32;
        let bh = ((*mh as f64) * scale * 0.5).round() as i32;
        let bw = bw.max(6);
        let bh = bh.max(3);

        let x0 = 1 + bx;
        let y0 = start_y + by;
        let x1 = x0 + bw - 1;
        let y1 = y0 + bh - 1;

        if y0 >= rows - 1 || x0 >= cols {
            continue;
        }
        if *is_sel {
            attron(COLOR_PAIR(CP_SELECTED));
        }

        if y0 < rows - 1 {
            mvprintw(y0, x0, "+");
            for dx in 1..bw - 1 {
                let px = x0 + dx;
                if px < x1 && px < cols {
                    mvprintw(y0, px, "-");
                }
            }
            if x1 < cols && x1 > x0 {
                mvprintw(y0, x1, "+");
            }
        }
        for dy in 1..bh - 1 {
            let y = y0 + dy;
            if y >= rows - 1 {
                break;
            }
            mvprintw(y, x0, "|");
            if x1 < cols {
                mvprintw(y, x1, "|");
            }
            if dy == bh / 2 {
                let space = (bw - 2).max(0) as usize;
                let label = &name[..name.len().min(space)];
                let pad_l = (space - label.len()) / 2;
                let center = format!("{:>width$}", label, width = label.len() + pad_l);
                mvprintw(y, x0 + 1, &center[..center.len().min(space)]);
            }
        }
        if y1 < rows - 1 && y1 > y0 {
            mvprintw(y1, x0, "+");
            for dx in 1..bw - 1 {
                let px = x0 + dx;
                if px < x1 && px < cols {
                    mvprintw(y1, px, "-");
                }
            }
            if x1 < cols && x1 > x0 {
                mvprintw(y1, x1, "+");
            }
        }

        if *is_sel {
            attroff(COLOR_PAIR(CP_SELECTED));
        }
    }
}

fn draw_mode_overlay(app: &App, scroll: usize, hover: usize, rows: i32, cols: i32) {
    let mon = app.cur_mon();
    let w = cols as usize;
    let list_start = 3i32;
    let visible = (rows - list_start - 2).max(1) as usize;

    attron(COLOR_PAIR(CP_OVERLAY) | A_BOLD());
    mvprintw(
        list_start,
        0,
        &pad(
            &format!(" Mode for {} ({} available)", mon.name, mon.modes.len()),
            w,
        ),
    );
    attroff(COLOR_PAIR(CP_OVERLAY) | A_BOLD());

    for i in 0..visible {
        let mi = scroll + i;
        let y = list_start + 1 + i as i32;
        if y >= rows - 1 {
            break;
        }
        if mi >= mon.modes.len() {
            mvprintw(y, 0, &pad("", w));
            continue;
        }
        let m = &mon.modes[mi];
        let flags = format!(
            "{}{}",
            if m.preferred { " (preferred)" } else { "" },
            if m.current { " (current)" } else { "" }
        );
        let line = format!(" {} {}{}", if mi == hover { ">" } else { " " }, m, flags);
        if mi == hover {
            attron(COLOR_PAIR(CP_SELECTED) | A_BOLD());
        } else if m.current {
            attron(COLOR_PAIR(CP_MODIFIED));
        }
        mvprintw(y, 0, &pad(&line, w));
        if mi == hover {
            attroff(COLOR_PAIR(CP_SELECTED) | A_BOLD());
        } else if m.current {
            attroff(COLOR_PAIR(CP_MODIFIED));
        }
    }

    if mon.modes.len() > visible {
        let info = format!(" {}/{} | Up/Dn to scroll ", hover + 1, mon.modes.len());
        attron(COLOR_PAIR(CP_HEADER));
        mvprintw(rows - 2, 0, &pad(&info, w));
        attroff(COLOR_PAIR(CP_HEADER));
    }
}

pub fn draw_statusbar(app: &App, rows: i32, cols: i32) {
    let help = if !app.status.is_empty() {
        format!(
            "{}  {}",
            if app.status_err { "[ERR]" } else { "[OK] " },
            app.status
        )
    } else {
        match (&app.ui, app.section) {
            (UiMode::TextEdit, _) => " Type value  Enter:confirm  Esc:cancel".to_string(),
            (UiMode::ModeSelect { .. }, _) => " Up/Dn:scroll  Enter:select  Esc:cancel".to_string(),
            (_, Section::Tabs) => {
                " L/R:switch tab  Down/Tab:settings  a:apply  r:reset  q:quit".to_string()
            }
            (_, Section::Settings) => {
                " Up/Dn:field  L/R:cycle  Enter:edit  Tab:layout  a:apply  r:reset  q:quit"
                    .to_string()
            }
            (_, Section::Layout) => {
                " Up/Dn:monitor  L/R:cycle pos  Enter:type x,y  Tab:tabs  p:proj  a:apply  q:quit"
                    .to_string()
            }
        }
    };
    let cp = if app.status_err {
        CP_ERROR
    } else {
        CP_STATUSBAR
    };
    attron(COLOR_PAIR(cp));
    mvprintw(rows - 1, 0, &pad(&help, cols as usize));
    attroff(COLOR_PAIR(cp));
}
