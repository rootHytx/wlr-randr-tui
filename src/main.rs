use ncurses::*;
use std::process::Command;

const CP_NORMAL:    i16 = 1;
const CP_HEADER:    i16 = 2;
const CP_SELECTED:  i16 = 3;
const CP_MODIFIED:  i16 = 4;
const CP_STATUSBAR: i16 = 5;
const CP_ERROR:     i16 = 6;
const CP_OVERLAY:   i16 = 7;

const TRANSFORMS: &[&str] = &[
    "normal", "90", "180", "270",
    "flipped", "flipped-90", "flipped-180", "flipped-270",
];

const N_FIELDS: usize = 5;
const F_MODE:      usize = 0;
const F_SCALE:     usize = 1;
const F_TRANSFORM: usize = 2;
const F_ADAPTIVE:  usize = 3;
const F_ENABLED:   usize = 4;
const FIELD_NAMES: &[&str] = &["Mode", "Scale", "Transform", "Adaptive Sync", "Enabled"];

// ── alignment & position ──────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum Align { Start, Center }

impl Align {
    fn label(&self) -> &'static str {
        match self { Align::Start => "", Align::Center => " (center)" }
    }
}

#[derive(Clone, PartialEq)]
enum PosMode {
    RightOf(String, Align),
    LeftOf(String, Align),
    Above(String, Align),
    Below(String, Align),
    Absolute(i32, i32),
}

impl PosMode {
    fn display(&self) -> String {
        match self {
            PosMode::RightOf(n, a) => format!("Right of {}{}", n, a.label()),
            PosMode::LeftOf(n, a)  => format!("Left of {}{}", n, a.label()),
            PosMode::Above(n, a)   => format!("Above {}{}", n, a.label()),
            PosMode::Below(n, a)   => format!("Below {}{}", n, a.label()),
            PosMode::Absolute(x, y) => format!("x={} y={}", x, y),
        }
    }
    fn is_absolute_eq(&self, pos: (i32, i32)) -> bool {
        matches!(self, PosMode::Absolute(x, y) if (*x, *y) == pos)
    }
}

fn logical_size(m: &Monitor) -> (i32, i32) {
    let md = &m.modes[m.new_mode];
    (
        (md.width  as f64 / m.new_scale).round() as i32,
        (md.height as f64 / m.new_scale).round() as i32,
    )
}

fn resolve_abs(pos: &PosMode, this: &Monitor, all: &[Monitor], depth: u8) -> (i32, i32) {
    if depth > 8 { return this.position; }
    match pos {
        PosMode::Absolute(x, y) => (*x, *y),
        PosMode::RightOf(ref_name, _) |
        PosMode::LeftOf(ref_name, _)  |
        PosMode::Above(ref_name, _)   |
        PosMode::Below(ref_name, _)   => {
            let rm = match all.iter().find(|m| &m.name == ref_name) {
                Some(m) => m, None => return this.position,
            };
            let rp = resolve_abs(&rm.new_pos, rm, all, depth + 1);
            let (rw, rh) = logical_size(rm);
            let (sw, sh) = logical_size(this);
            match pos {
                PosMode::RightOf(_, a) => (
                    rp.0 + rw,
                    rp.1 + if *a == Align::Center { (rh - sh) / 2 } else { 0 },
                ),
                PosMode::LeftOf(_, a) => (
                    rp.0 - sw,
                    rp.1 + if *a == Align::Center { (rh - sh) / 2 } else { 0 },
                ),
                PosMode::Above(_, a) => (
                    rp.0 + if *a == Align::Center { (rw - sw) / 2 } else { 0 },
                    rp.1 - sh,
                ),
                PosMode::Below(_, a) => (
                    rp.0 + if *a == Align::Center { (rw - sw) / 2 } else { 0 },
                    rp.1 + rh,
                ),
                _ => unreachable!(),
            }
        }
    }
}

fn pos_cycle_list(cur: &PosMode, others: &[String]) -> Vec<PosMode> {
    let mut opts: Vec<PosMode> = Vec::new();
    for o in others {
        opts.push(PosMode::Below(o.clone(),   Align::Start));
        opts.push(PosMode::Below(o.clone(),   Align::Center));
        opts.push(PosMode::RightOf(o.clone(), Align::Start));
        opts.push(PosMode::RightOf(o.clone(), Align::Center));
        opts.push(PosMode::Above(o.clone(),   Align::Start));
        opts.push(PosMode::Above(o.clone(),   Align::Center));
        opts.push(PosMode::LeftOf(o.clone(),  Align::Start));
        opts.push(PosMode::LeftOf(o.clone(),  Align::Center));
    }
    let abs = match cur { PosMode::Absolute(x, y) => PosMode::Absolute(*x, *y), _ => PosMode::Absolute(0, 0) };
    if !opts.contains(&abs) { opts.push(abs); }
    opts
}

// ── projection presets ────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Proj { Extend, ExtOnly, LaptopOnly, Custom }

impl Proj {
    const ALL: &'static [Proj] = &[Proj::Extend, Proj::ExtOnly, Proj::LaptopOnly, Proj::Custom];
    fn label(self) -> &'static str {
        match self {
            Proj::Extend     => "Extend",
            Proj::ExtOnly    => "Ext Only",
            Proj::LaptopOnly => "Laptop Only",
            Proj::Custom     => "Custom",
        }
    }
    fn step(self, d: i32) -> Self {
        let i = Self::ALL.iter().position(|&p| p == self).unwrap_or(0);
        Self::ALL[((i as i32 + d).rem_euclid(Self::ALL.len() as i32)) as usize]
    }
}

fn apply_proj(mons: &mut [Monitor], proj: Proj) {
    let first_ext = mons.iter().find(|m| !m.is_laptop()).map(|m| m.name.clone());
    match proj {
        Proj::Extend => {
            for m in mons.iter_mut() { m.new_enabled = true; }
            for m in mons.iter_mut() {
                if !m.is_laptop() { m.new_pos = PosMode::Absolute(0, 0); }
            }
            if let Some(ext) = &first_ext {
                for m in mons.iter_mut() {
                    if m.is_laptop() { m.new_pos = PosMode::Below(ext.clone(), Align::Center); }
                }
            }
        }
        Proj::ExtOnly    => { for m in mons.iter_mut() { m.new_enabled = !m.is_laptop(); } }
        Proj::LaptopOnly => { for m in mons.iter_mut() { m.new_enabled =  m.is_laptop(); } }
        Proj::Custom     => {}
    }
}

// ── monitor data ──────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Mode {
    width: u32, height: u32, refresh: f64,
    preferred: bool, current: bool,
}
impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{} @ {:.3} Hz", self.width, self.height, self.refresh)
    }
}

#[derive(Clone)]
struct Monitor {
    name: String, make: String, model: String,
    enabled: bool, modes: Vec<Mode>, cur_mode: usize,
    position: (i32, i32), transform: String, scale: f64, adaptive_sync: bool,
    new_mode: usize, new_pos: PosMode,
    new_transform: String, new_scale: f64, new_adaptive: bool, new_enabled: bool,
}

impl Monitor {
    fn is_laptop(&self) -> bool { self.name.starts_with("eDP") }

    fn field_dirty(&self, f: usize) -> bool {
        match f {
            F_MODE      => self.new_mode != self.cur_mode,
            F_SCALE     => (self.new_scale - self.scale).abs() > 0.0001,
            F_TRANSFORM => self.new_transform != self.transform,
            F_ADAPTIVE  => self.new_adaptive != self.adaptive_sync,
            F_ENABLED   => self.new_enabled != self.enabled,
            _           => false,
        }
    }
    fn pos_dirty(&self) -> bool { !self.new_pos.is_absolute_eq(self.position) }
    fn dirty(&self) -> bool { self.pos_dirty() || (0..N_FIELDS).any(|f| self.field_dirty(f)) }

    fn reset(&mut self) {
        self.new_mode      = self.cur_mode;
        self.new_pos       = PosMode::Absolute(self.position.0, self.position.1);
        self.new_transform = self.transform.clone();
        self.new_scale     = self.scale;
        self.new_adaptive  = self.adaptive_sync;
        self.new_enabled   = self.enabled;
    }

    fn field_value(&self, f: usize) -> String {
        let m = |b: bool| if b { " [*]" } else { "" };
        match f {
            F_MODE => {
                let md = &self.modes[self.new_mode];
                format!("{}{}{}{}", md,
                    if md.preferred { " (preferred)" } else { "" },
                    if md.current   { " (current)"   } else { "" },
                    m(self.field_dirty(f)))
            }
            F_SCALE     => format!("{:.4}{}", self.new_scale, m(self.field_dirty(f))),
            F_TRANSFORM => format!("{}{}", self.new_transform, m(self.field_dirty(f))),
            F_ADAPTIVE  => format!("{}{}", if self.new_adaptive { "enabled" } else { "disabled" }, m(self.field_dirty(f))),
            F_ENABLED   => format!("{}{}", if self.new_enabled  { "yes"     } else { "no"        }, m(self.field_dirty(f))),
            _ => String::new(),
        }
    }
}

// ── parsing ───────────────────────────────────────────────────────────────────

fn parse_monitors() -> Vec<Monitor> {
    let out = Command::new("wlr-randr").output().unwrap_or_else(|_| {
        eprintln!("error: wlr-randr not found"); std::process::exit(1);
    });
    parse_output(&String::from_utf8_lossy(&out.stdout))
}

fn parse_output(text: &str) -> Vec<Monitor> {
    let mut result = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if line.starts_with(' ') || line.is_empty() { continue; }
        let name = match line.split_whitespace().next() { Some(n) => n.to_string(), None => continue };
        let (mut make, mut model) = (String::new(), String::new());
        let mut enabled = true;
        let mut modes: Vec<Mode> = Vec::new();
        let mut pos = (0i32, 0i32);
        let mut transform = "normal".to_string();
        let mut scale = 1.0f64;
        let mut adaptive = false;
        let mut in_modes = false;
        loop {
            match lines.peek() { Some(l) if l.starts_with("  ") || l.starts_with('\t') => {} _ => break }
            let t = lines.next().unwrap(); let t = t.trim();
            if      t.starts_with("Make: ")          { make      = t["Make: ".len()..].to_string();                         in_modes = false; }
            else if t.starts_with("Model: ")         { model     = t["Model: ".len()..].to_string();                        in_modes = false; }
            else if t.starts_with("Enabled: ")       { enabled   = t.ends_with("yes");                                      in_modes = false; }
            else if t == "Modes:"                    { in_modes  = true; }
            else if t.starts_with("Position: ")      {
                in_modes = false;
                let s = &t["Position: ".len()..]; let mut p = s.splitn(2, ',');
                pos = (p.next().unwrap_or("0").trim().parse().unwrap_or(0),
                       p.next().unwrap_or("0").trim().parse().unwrap_or(0));
            }
            else if t.starts_with("Transform: ")     { transform = t["Transform: ".len()..].to_string();                    in_modes = false; }
            else if t.starts_with("Scale: ")         { scale     = t["Scale: ".len()..].trim().parse().unwrap_or(1.0);      in_modes = false; }
            else if t.starts_with("Adaptive Sync: ") { adaptive  = t.ends_with("enabled");                                  in_modes = false; }
            else if in_modes && t.contains("px,")    { if let Some(m) = parse_mode(t) { modes.push(m); } }
        }
        if modes.is_empty() { continue; }
        let cur = modes.iter().position(|m| m.current).unwrap_or(0);
        result.push(Monitor {
            name, make, model, enabled, modes, cur_mode: cur, position: pos,
            transform: transform.clone(), scale, adaptive_sync: adaptive,
            new_mode: cur, new_pos: PosMode::Absolute(pos.0, pos.1),
            new_transform: transform, new_scale: scale, new_adaptive: adaptive, new_enabled: enabled,
        });
    }
    result
}

fn parse_mode(t: &str) -> Option<Mode> {
    let w: Vec<&str> = t.split_whitespace().collect();
    if w.len() < 4 { return None; }
    let d: Vec<&str> = w[0].split('x').collect();
    if d.len() != 2 { return None; }
    Some(Mode { width: d[0].parse().ok()?, height: d[1].parse().ok()?,
                refresh: w[2].parse().ok()?, preferred: t.contains("preferred"), current: t.contains("current") })
}

// ── applying ──────────────────────────────────────────────────────────────────

fn build_args(m: &Monitor, all: &[Monitor]) -> Vec<String> {
    let mut a = vec!["--output".to_string(), m.name.clone()];
    if !m.new_enabled { a.push("--off".to_string()); return a; }
    if !m.enabled { a.push("--on".to_string()); }
    if m.new_mode != m.cur_mode {
        let md = &m.modes[m.new_mode];
        a.extend(["--mode".to_string(), format!("{}x{}@{:.3}Hz", md.width, md.height, md.refresh)]);
    }
    // Always emit --pos — partial omission lets the compositor decide, which causes wrong layouts.
    let abs = resolve_abs(&m.new_pos, m, all, 0);
    a.extend(["--pos".to_string(), format!("{},{}", abs.0, abs.1)]);
    if (m.new_scale - m.scale).abs() > 0.0001 {
        a.extend(["--scale".to_string(), format!("{:.6}", m.new_scale)]);
    }
    if m.new_transform != m.transform {
        a.extend(["--transform".to_string(), m.new_transform.clone()]);
    }
    if m.new_adaptive != m.adaptive_sync {
        a.extend(["--adaptive-sync".to_string(),
            if m.new_adaptive { "enabled" } else { "disabled" }.to_string()]);
    }
    a
}

fn do_apply(monitors: &[Monitor]) -> Result<String, String> {
    if !monitors.iter().any(|m| m.dirty()) {
        return Ok("No changes to apply".to_string());
    }
    // Send the complete layout every time — never a partial update.
    // This ensures the compositor gets a consistent picture of all outputs.
    let args: Vec<String> = monitors.iter()
        .flat_map(|m| build_args(m, monitors))
        .collect();
    let cmd = format!("wlr-randr {}", args.join(" "));
    let out = Command::new("wlr-randr").args(&args).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(cmd) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

// ── app state ─────────────────────────────────────────────────────────────────

#[derive(PartialEq, Clone, Copy)]
enum Section { Tabs, Settings, Layout }

#[derive(PartialEq, Clone)]
enum UiMode { Normal, ModeSelect { scroll: usize, hover: usize }, TextEdit }

struct App {
    monitors: Vec<Monitor>,
    proj: Proj,
    tab: usize,
    section: Section,
    sel_field: usize,
    layout_sel: usize,
    ui: UiMode,
    buf: String,
    status: String,
    status_err: bool,
}

impl App {
    fn new(monitors: Vec<Monitor>) -> Self {
        App { monitors, proj: Proj::Custom, tab: 0, section: Section::Settings,
              sel_field: 0, layout_sel: 0, ui: UiMode::Normal,
              buf: String::new(), status: String::new(), status_err: false }
    }
    fn set_status(&mut self, s: impl Into<String>, err: bool) { self.status = s.into(); self.status_err = err; }
    fn cur_mon(&self)         -> &Monitor     { &self.monitors[self.tab] }
    fn cur_mon_mut(&mut self) -> &mut Monitor { &mut self.monitors[self.tab] }
}

// ── drawing ───────────────────────────────────────────────────────────────────

fn pad(s: &str, w: usize) -> String {
    let s = if s.len() > w { &s[..w] } else { s };
    format!("{:<w$}", s, w = w)
}

fn draw(app: &App) {
    let mut rows = 0i32; let mut cols = 0i32;
    getmaxyx(stdscr(), &mut rows, &mut cols);
    clear();
    let w = cols as usize;

    // Row 0: title bar
    attron(COLOR_PAIR(CP_HEADER) | A_BOLD());
    mvprintw(0, 0, &pad(" wlr-randr TUI", w));
    attroff(COLOR_PAIR(CP_HEADER) | A_BOLD());

    // Row 1: monitor tabs
    draw_tabs(app, 1, cols);

    // Row 2: divider
    mvprintw(2, 0, &"-".repeat(w));

    // Mode-select overlay covers rows 3+
    if let UiMode::ModeSelect { scroll, hover } = app.ui {
        draw_mode_overlay(app, scroll, hover, rows, cols);
        draw_statusbar(app, rows, cols);
        refresh();
        return;
    }

    // Row 3: monitor info
    let mon = app.cur_mon();
    let info = format!("  {}  {}  {}", mon.name,
        if mon.make.is_empty() { String::new() } else { format!("{} {}", mon.make, mon.model) },
        if mon.is_laptop() { "[laptop]" } else { "[external]" });
    mvprintw(3, 0, &pad(&info, w));

    // Row 4: divider
    mvprintw(4, 0, &"-".repeat(w));

    // Rows 5..(5+N_FIELDS): settings fields
    let settings_top = 5i32;
    for f in 0..N_FIELDS {
        let y = settings_top + f as i32;
        if y >= rows - 1 { break; }
        let sel = app.section == Section::Settings && f == app.sel_field;
        let val = if sel && app.ui == UiMode::TextEdit {
            format!("[{}]_", app.buf)
        } else {
            mon.field_value(f)
        };
        let line = format!(" {} {:<14} {}",
            if sel { ">" } else { " " },
            format!("{}:", FIELD_NAMES[f]),
            val);
        if sel { attron(COLOR_PAIR(CP_SELECTED) | A_BOLD()); }
        else if mon.field_dirty(f) { attron(COLOR_PAIR(CP_MODIFIED)); }
        mvprintw(y, 0, &pad(&line, w));
        if sel { attroff(COLOR_PAIR(CP_SELECTED) | A_BOLD()); }
        else if mon.field_dirty(f) { attroff(COLOR_PAIR(CP_MODIFIED)); }
    }

    // Divider before layout section
    let layout_div = settings_top + N_FIELDS as i32;
    if layout_div < rows - 1 {
        mvprintw(layout_div, 0, &"-".repeat(w));
    }

    // Layout section
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
        if x + label.len() as i32 + 1 >= cols { break; }
        let is_active  = i == app.tab;
        let is_focused = is_active && app.section == Section::Tabs;
        if is_focused        { attron(COLOR_PAIR(CP_SELECTED) | A_BOLD()); }
        else if is_active    { attron(A_BOLD()); }
        mvprintw(y, x, &label);
        if is_focused        { attroff(COLOR_PAIR(CP_SELECTED) | A_BOLD()); }
        else if is_active    { attroff(A_BOLD()); }
        x += label.len() as i32 + 1;
    }
    let hint = "Tab:focus";
    if (hint.len() as i32 + 2) < cols {
        attron(COLOR_PAIR(CP_HEADER));
        mvprintw(y, cols - hint.len() as i32 - 1, hint);
        attroff(COLOR_PAIR(CP_HEADER));
    }
}

fn draw_layout_section(app: &App, top: i32, rows: i32, cols: i32) {
    let w = cols as usize;

    // Header: "Layout  Proj: [X] [Y] ..."
    mvprintw(top, 0, &" ".repeat(w));
    let hdr = "Layout  Proj:";
    mvprintw(top, 1, hdr);
    let mut x = 1 + hdr.len() as i32 + 1;
    for p in Proj::ALL {
        let s = format!("[{}]", p.label());
        if *p == app.proj { attron(COLOR_PAIR(CP_SELECTED) | A_BOLD()); }
        mvprintw(top, x, &s);
        if *p == app.proj { attroff(COLOR_PAIR(CP_SELECTED) | A_BOLD()); }
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
        } else { String::new() };
        let line = format!(" {} {:<12} {}{}",
            if in_layout { ">" } else { " " },
            format!("{}{}:", mon.name, nav),
            pos_str, mark);
        if in_layout { attron(COLOR_PAIR(CP_SELECTED) | A_BOLD()); }
        else if mon.pos_dirty() { attron(COLOR_PAIR(CP_MODIFIED)); }
        mvprintw(pos_y, 0, &pad(&line, w));
        if in_layout { attroff(COLOR_PAIR(CP_SELECTED) | A_BOLD()); }
        else if mon.pos_dirty() { attroff(COLOR_PAIR(CP_MODIFIED)); }
    }

    // Diagram takes all remaining space
    let diag_top = pos_y + 1;
    if diag_top < rows - 1 {
        draw_layout_diagram(app, diag_top, rows, cols);
    }
}

fn draw_layout_diagram(app: &App, start_y: i32, rows: i32, cols: i32) {
    let avail_h = (rows - 1 - start_y).max(0) as usize;
    if avail_h < 3 || cols < 10 { return; }

    let sel_name = if app.section == Section::Layout {
        app.monitors[app.layout_sel].name.clone()
    } else {
        app.cur_mon().name.clone()
    };

    let layout: Vec<(String, i32, i32, i32, i32, bool)> = app.monitors.iter().map(|m| {
        let (ax, ay) = resolve_abs(&m.new_pos, m, &app.monitors, 0);
        let (lw, lh) = logical_size(m);
        (m.name.clone(), ax, ay, lw, lh, m.name == sel_name)
    }).collect();

    let min_x = layout.iter().map(|e| e.1).min().unwrap_or(0);
    let min_y = layout.iter().map(|e| e.2).min().unwrap_or(0);
    let max_x = layout.iter().map(|e| e.1 + e.3).max().unwrap_or(1);
    let max_y = layout.iter().map(|e| e.2 + e.4).max().unwrap_or(1);
    let total_w = (max_x - min_x).max(1) as f64;
    let total_h = (max_y - min_y).max(1) as f64;

    let avail_w = (cols as f64 - 2.0).max(4.0);
    let avail_hf = (avail_h as f64).max(1.0);
    let sx = avail_w / total_w;
    let sy = avail_hf / total_h;
    let scale = sx.min(sy * 2.0).min(sy);

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

        if y0 >= rows - 1 || x0 >= cols { continue; }
        if *is_sel { attron(COLOR_PAIR(CP_SELECTED)); }

        if y0 < rows - 1 {
            mvprintw(y0, x0, "+");
            for dx in 1..bw - 1 {
                let px = x0 + dx;
                if px < x1 && px < cols { mvprintw(y0, px, "-"); }
            }
            if x1 < cols && x1 > x0 { mvprintw(y0, x1, "+"); }
        }
        for dy in 1..bh - 1 {
            let y = y0 + dy;
            if y >= rows - 1 { break; }
            mvprintw(y, x0, "|");
            if x1 < cols { mvprintw(y, x1, "|"); }
            if dy == bh / 2 {
                let space = (bw - 2).max(0) as usize;
                let label = &name[..name.len().min(space)];
                let pad_l = (space - label.len()) / 2;
                let centered = format!("{:>width$}", label, width = label.len() + pad_l);
                mvprintw(y, x0 + 1, &centered[..centered.len().min(space)]);
            }
        }
        if y1 < rows - 1 && y1 > y0 {
            mvprintw(y1, x0, "+");
            for dx in 1..bw - 1 {
                let px = x0 + dx;
                if px < x1 && px < cols { mvprintw(y1, px, "-"); }
            }
            if x1 < cols && x1 > x0 { mvprintw(y1, x1, "+"); }
        }

        if *is_sel { attroff(COLOR_PAIR(CP_SELECTED)); }
    }
}

fn draw_mode_overlay(app: &App, scroll: usize, hover: usize, rows: i32, cols: i32) {
    let mon = app.cur_mon();
    let w = cols as usize;
    let list_start = 3i32;
    let visible = (rows - list_start - 2).max(1) as usize;

    attron(COLOR_PAIR(CP_OVERLAY) | A_BOLD());
    mvprintw(list_start, 0, &pad(&format!(" Mode for {} ({} available)", mon.name, mon.modes.len()), w));
    attroff(COLOR_PAIR(CP_OVERLAY) | A_BOLD());

    for i in 0..visible {
        let mi = scroll + i;
        let y  = list_start + 1 + i as i32;
        if y >= rows - 1 { break; }
        if mi >= mon.modes.len() { mvprintw(y, 0, &pad("", w)); continue; }
        let m = &mon.modes[mi];
        let flags = format!("{}{}",
            if m.preferred { " (preferred)" } else { "" },
            if m.current   { " (current)"   } else { "" });
        let line = format!(" {} {}{}", if mi == hover { ">" } else { " " }, m, flags);
        if mi == hover    { attron(COLOR_PAIR(CP_SELECTED) | A_BOLD()); }
        else if m.current { attron(COLOR_PAIR(CP_MODIFIED)); }
        mvprintw(y, 0, &pad(&line, w));
        if mi == hover    { attroff(COLOR_PAIR(CP_SELECTED) | A_BOLD()); }
        else if m.current { attroff(COLOR_PAIR(CP_MODIFIED)); }
    }

    if mon.modes.len() > visible {
        let info = format!(" {}/{} | Up/Dn to scroll ", hover + 1, mon.modes.len());
        attron(COLOR_PAIR(CP_HEADER));
        mvprintw(rows - 2, 0, &pad(&info, w));
        attroff(COLOR_PAIR(CP_HEADER));
    }
}

fn draw_statusbar(app: &App, rows: i32, cols: i32) {
    let help = if !app.status.is_empty() {
        format!("{}  {}", if app.status_err { "[ERR]" } else { "[OK] " }, app.status)
    } else {
        match (&app.ui, app.section) {
            (UiMode::TextEdit, _)           => " Type value  Enter:confirm  Esc:cancel".to_string(),
            (UiMode::ModeSelect { .. }, _)  => " Up/Dn:scroll  Enter:select  Esc:cancel".to_string(),
            (_, Section::Tabs)              => " L/R:switch tab  Down/Tab:settings  a:apply  r:reset  q:quit".to_string(),
            (_, Section::Settings)          => " Up/Dn:field  L/R:cycle  Enter:edit  Tab:layout  a:apply  r:reset  q:quit".to_string(),
            (_, Section::Layout)            => " Up/Dn:monitor  L/R:cycle pos  Enter:type x,y  Tab:tabs  p:proj  a:apply  q:quit".to_string(),
        }
    };
    let cp = if app.status_err { CP_ERROR } else { CP_STATUSBAR };
    attron(COLOR_PAIR(cp));
    mvprintw(rows - 1, 0, &pad(&help, cols as usize));
    attroff(COLOR_PAIR(cp));
}

// ── input ─────────────────────────────────────────────────────────────────────

fn handle(app: &mut App, ch: i32) -> bool {
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
                        app.ui = UiMode::TextEdit;
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
            app.proj = p; apply_proj(&mut app.monitors, p);
            app.set_status(format!("Projection: {}", p.label()), false);
        }
        44 => { // , : projection backward
            let p = app.proj.step(-1);
            app.proj = p; apply_proj(&mut app.monitors, p);
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
    let idx = app.layout_sel;
    let mon_name = app.monitors[idx].name.clone();
    let others: Vec<String> = app.monitors.iter()
        .filter(|m| m.name != mon_name)
        .map(|m| m.name.clone())
        .collect();
    let m = &mut app.monitors[idx];
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
            let buf = app.buf.clone(); app.buf.clear(); app.ui = UiMode::Normal;
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
                            _ => app.set_status("Invalid -- use: x,y", true),
                        }
                    } else { app.set_status("Format: x,y  e.g. 3440,0", true); }
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

// ── main ──────────────────────────────────────────────────────────────────────

fn main() {
    let monitors = parse_monitors();
    if monitors.is_empty() { eprintln!("No monitors detected."); std::process::exit(1); }

    initscr(); start_color(); use_default_colors(); cbreak(); noecho();
    keypad(stdscr(), true);
    curs_set(CURSOR_VISIBILITY::CURSOR_INVISIBLE);

    init_pair(CP_NORMAL,    -1,           -1);
    init_pair(CP_HEADER,    COLOR_BLACK,  COLOR_CYAN);
    init_pair(CP_SELECTED,  COLOR_BLACK,  COLOR_GREEN);
    init_pair(CP_MODIFIED,  COLOR_YELLOW, -1);
    init_pair(CP_STATUSBAR, COLOR_BLACK,  COLOR_WHITE);
    init_pair(CP_ERROR,     COLOR_WHITE,  COLOR_RED);
    init_pair(CP_OVERLAY,   COLOR_BLACK,  COLOR_MAGENTA);

    let mut app = App::new(monitors);
    loop {
        draw(&app);
        let ch = getch();
        if !handle(&mut app, ch) { break; }
    }
    endwin();
}
