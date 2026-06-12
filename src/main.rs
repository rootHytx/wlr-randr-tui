use ncurses::*;
use std::process::Command;

const CP_NORMAL: i16 = 1;
const CP_HEADER: i16 = 2;
const CP_SELECTED: i16 = 3;
const CP_MODIFIED: i16 = 4;
const CP_STATUSBAR: i16 = 5;
const CP_ERROR: i16 = 6;
const CP_OVERLAY: i16 = 7;

const TRANSFORMS: &[&str] = &[
    "normal", "90", "180", "270",
    "flipped", "flipped-90", "flipped-180", "flipped-270",
];

const N_FIELDS: usize = 6;
const F_MODE: usize = 0;
const F_POS: usize = 1;
const F_SCALE: usize = 2;
const F_TRANSFORM: usize = 3;
const F_ADAPTIVE: usize = 4;
const F_ENABLED: usize = 5;
const FIELD_NAMES: &[&str] = &["Mode", "Position", "Scale", "Transform", "Adaptive Sync", "Enabled"];

// ── alignment & position ──────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum Align { Start, Center }

impl Align {
    fn label(&self) -> &'static str {
        match self { Align::Start => "", Align::Center => " (center)" }
    }
}

/// Relative positions carry an alignment on the *perpendicular* axis:
///   RightOf / LeftOf  → align vertically (top vs. centered)
///   Above   / Below   → align horizontally (left vs. centered)
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
            PosMode::LeftOf(n, a)  => format!("Left of {}{}",  n, a.label()),
            PosMode::Above(n, a)   => format!("Above {}{}",    n, a.label()),
            PosMode::Below(n, a)   => format!("Below {}{}",    n, a.label()),
            PosMode::Absolute(x, y) => format!("x={} y={}", x, y),
        }
    }
    fn is_absolute_eq(&self, pos: (i32, i32)) -> bool {
        matches!(self, PosMode::Absolute(x, y) if (*x, *y) == pos)
    }
}

/// Logical size = mode pixels ÷ scale factor
fn logical_size(m: &Monitor) -> (i32, i32) {
    let md = &m.modes[m.new_mode];
    (
        (md.width  as f64 / m.new_scale).round() as i32,
        (md.height as f64 / m.new_scale).round() as i32,
    )
}

/// Recursively resolve a PosMode to an absolute (x, y).
/// Falls back to the monitor's current reported position on cycle or missing ref.
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
        // Group by direction; center immediately follows plain so one press toggles it
        opts.push(PosMode::Below(o.clone(),   Align::Start));
        opts.push(PosMode::Below(o.clone(),   Align::Center));
        opts.push(PosMode::RightOf(o.clone(), Align::Start));
        opts.push(PosMode::RightOf(o.clone(), Align::Center));
        opts.push(PosMode::Above(o.clone(),   Align::Start));
        opts.push(PosMode::Above(o.clone(),   Align::Center));
        opts.push(PosMode::LeftOf(o.clone(),  Align::Start));
        opts.push(PosMode::LeftOf(o.clone(),  Align::Center));
    }
    // Keep the current absolute so it round-trips cleanly
    let abs = match cur { PosMode::Absolute(x, y) => PosMode::Absolute(*x, *y), _ => PosMode::Absolute(0, 0) };
    if !opts.contains(&abs) { opts.push(abs); }
    opts
}

// ── projection preset ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Proj { Extend, ExtOnly, LaptopOnly, Custom }

impl Proj {
    const ALL: &'static [Proj] = &[Proj::Extend, Proj::ExtOnly, Proj::LaptopOnly, Proj::Custom];
    fn label(self) -> &'static str {
        match self {
            Proj::Extend     => "Extend",
            Proj::ExtOnly    => "External Only",
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
    let first_lap = mons.iter().find(|m|  m.is_laptop()).map(|m| m.name.clone());
    match proj {
        Proj::Extend => {
            for m in mons.iter_mut() { m.new_enabled = true; }
            // External at origin; laptop centered below external
            for m in mons.iter_mut() {
                if !m.is_laptop() { m.new_pos = PosMode::Absolute(0, 0); }
            }
            if let Some(ext) = &first_ext {
                for m in mons.iter_mut() {
                    if m.is_laptop() { m.new_pos = PosMode::Below(ext.clone(), Align::Center); }
                }
            }
        }
        Proj::ExtOnly => {
            for m in mons.iter_mut() { m.new_enabled = !m.is_laptop(); }
        }
        Proj::LaptopOnly => {
            for m in mons.iter_mut() { m.new_enabled = m.is_laptop(); }
        }
        Proj::Custom => {}
    }
    let _ = first_lap;
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
    // editable
    new_mode: usize, new_pos: PosMode,
    new_transform: String, new_scale: f64, new_adaptive: bool, new_enabled: bool,
}

impl Monitor {
    fn is_laptop(&self) -> bool { self.name.starts_with("eDP") }

    fn field_dirty(&self, f: usize) -> bool {
        match f {
            F_MODE      => self.new_mode != self.cur_mode,
            F_POS       => !self.new_pos.is_absolute_eq(self.position),
            F_SCALE     => (self.new_scale - self.scale).abs() > 0.0001,
            F_TRANSFORM => self.new_transform != self.transform,
            F_ADAPTIVE  => self.new_adaptive != self.adaptive_sync,
            F_ENABLED   => self.new_enabled != self.enabled,
            _           => false,
        }
    }
    fn dirty(&self) -> bool { (0..N_FIELDS).any(|f| self.field_dirty(f)) }

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
            if      t.starts_with("Make: ")         { make      = t["Make: ".len()..].to_string();           in_modes = false; }
            else if t.starts_with("Model: ")        { model     = t["Model: ".len()..].to_string();          in_modes = false; }
            else if t.starts_with("Enabled: ")      { enabled   = t.ends_with("yes");                        in_modes = false; }
            else if t == "Modes:"                   { in_modes  = true; }
            else if t.starts_with("Position: ")     {
                in_modes = false;
                let s = &t["Position: ".len()..]; let mut p = s.splitn(2, ',');
                pos = (p.next().unwrap_or("0").trim().parse().unwrap_or(0),
                       p.next().unwrap_or("0").trim().parse().unwrap_or(0));
            }
            else if t.starts_with("Transform: ")    { transform = t["Transform: ".len()..].to_string();      in_modes = false; }
            else if t.starts_with("Scale: ")        { scale     = t["Scale: ".len()..].trim().parse().unwrap_or(1.0); in_modes = false; }
            else if t.starts_with("Adaptive Sync: "){ adaptive  = t.ends_with("enabled");                    in_modes = false; }
            else if in_modes && t.contains("px,")   { if let Some(m) = parse_mode(t) { modes.push(m); } }
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
    // Always resolve to absolute — handles both plain relative and centered alignment
    let abs = resolve_abs(&m.new_pos, m, all, 0);
    if abs != m.position {
        a.extend(["--pos".to_string(), format!("{},{}", abs.0, abs.1)]);
    }
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
    let args: Vec<String> = monitors.iter()
        .filter(|m| m.dirty())
        .flat_map(|m| build_args(m, monitors))
        .collect();
    if args.is_empty() { return Ok("No changes to apply".to_string()); }
    let cmd = format!("wlr-randr {}", args.join(" "));
    let out = Command::new("wlr-randr").args(&args).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(cmd) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

// ── app state ─────────────────────────────────────────────────────────────────

#[derive(PartialEq, Clone, Copy)]
enum Panel { Left, Right }

#[derive(PartialEq, Clone)]
enum UiMode { Normal, ModeSelect { scroll: usize, hover: usize }, TextEdit }

struct App {
    monitors: Vec<Monitor>,
    proj: Proj,
    sel_mon: usize,
    panel: Panel,
    sel_field: usize,
    ui: UiMode,
    buf: String,
    status: String,
    status_err: bool,
}

impl App {
    fn new(monitors: Vec<Monitor>) -> Self {
        App { monitors, proj: Proj::Custom, sel_mon: 0, panel: Panel::Left,
              sel_field: 0, ui: UiMode::Normal, buf: String::new(),
              status: String::new(), status_err: false }
    }
    fn set_status(&mut self, s: impl Into<String>, err: bool) { self.status = s.into(); self.status_err = err; }
    fn mon(&self)         -> &Monitor     { &self.monitors[self.sel_mon] }
    fn mon_mut(&mut self) -> &mut Monitor { &mut self.monitors[self.sel_mon] }
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

    // Title
    attron(COLOR_PAIR(CP_HEADER) | A_BOLD());
    mvprintw(0, 0, &pad(" wlr-randr TUI", cols as usize));
    attroff(COLOR_PAIR(CP_HEADER) | A_BOLD());

    // Projection row
    mvprintw(1, 0, &pad("", cols as usize));
    mvprintw(1, 0, " Proj: ");
    let mut px = 7i32;
    for p in Proj::ALL {
        let label = format!("[{}]", p.label());
        if *p == app.proj { attron(COLOR_PAIR(CP_SELECTED) | A_BOLD()); }
        mvprintw(1, px, &label);
        if *p == app.proj { attroff(COLOR_PAIR(CP_SELECTED) | A_BOLD()); }
        px += label.len() as i32 + 1;
    }
    attron(COLOR_PAIR(CP_HEADER));
    mvprintw(1, px, "  p / , . : cycle");
    attroff(COLOR_PAIR(CP_HEADER));

    // Divider
    mvprintw(2, 0, &std::iter::repeat('-').take(cols as usize).collect::<String>());

    let lw = 22i32; let rx = lw + 1; let rw = (cols - rx).max(0) as usize;

    // Panel headers
    if app.panel == Panel::Left  { attron(COLOR_PAIR(CP_SELECTED) | A_BOLD()); } else { attron(COLOR_PAIR(CP_HEADER)); }
    mvprintw(3, 0, &pad(" Monitors", lw as usize));
    if app.panel == Panel::Left  { attroff(COLOR_PAIR(CP_SELECTED) | A_BOLD()); } else { attroff(COLOR_PAIR(CP_HEADER)); }

    let rhead = if app.monitors.is_empty() { " Settings".to_string() }
                else { format!(" {}", app.monitors[app.sel_mon].name) };
    if app.panel == Panel::Right { attron(COLOR_PAIR(CP_SELECTED) | A_BOLD()); } else { attron(COLOR_PAIR(CP_HEADER)); }
    mvprintw(3, rx, &pad(&rhead, rw));
    if app.panel == Panel::Right { attroff(COLOR_PAIR(CP_SELECTED) | A_BOLD()); } else { attroff(COLOR_PAIR(CP_HEADER)); }

    for y in 3..rows - 1 { mvprintw(y, lw, "|"); }

    // Monitor list
    for (i, mon) in app.monitors.iter().enumerate() {
        let y = 4 + i as i32;
        if y >= rows - 1 { break; }
        let sel = i == app.sel_mon;
        let icon = if mon.is_laptop() { "L" } else { "M" };
        let mark = if mon.dirty() { "*" } else { " " };
        let line = format!(" {} [{}] {}{}", if sel { ">" } else { " " }, icon, mon.name, mark);
        if sel && app.panel == Panel::Left { attron(COLOR_PAIR(CP_SELECTED) | A_BOLD()); }
        else if sel  { attron(A_BOLD()); }
        else if mon.dirty() { attron(COLOR_PAIR(CP_MODIFIED)); }
        mvprintw(y, 0, &pad(&line, lw as usize));
        if sel && app.panel == Panel::Left { attroff(COLOR_PAIR(CP_SELECTED) | A_BOLD()); }
        else if sel  { attroff(A_BOLD()); }
        else if mon.dirty() { attroff(COLOR_PAIR(CP_MODIFIED)); }
    }

    // Right panel
    if !app.monitors.is_empty() {
        if let UiMode::ModeSelect { scroll, hover } = app.ui {
            draw_mode_overlay(app, scroll, hover, rx, rows, rw);
        } else {
            draw_settings(app, rx, rw, rows);
        }
    }

    // Status / help
    let help = if !app.status.is_empty() {
        format!("{}  {}", if app.status_err { "[ERR]" } else { "[OK] " }, app.status)
    } else {
        match &app.ui {
            UiMode::TextEdit          => " Type value  Enter:confirm  Esc:cancel".to_string(),
            UiMode::ModeSelect { .. } => " Up/Dn:scroll  Enter:select  Esc:cancel".to_string(),
            UiMode::Normal            => " Tab:panel  Up/Dn:nav  L/R:cycle  Enter:edit  p:proj  a:apply  r:reset  q:quit".to_string(),
        }
    };
    let cp = if app.status_err { CP_ERROR } else { CP_STATUSBAR };
    attron(COLOR_PAIR(cp));
    mvprintw(rows - 1, 0, &pad(&help, cols as usize));
    attroff(COLOR_PAIR(cp));

    refresh();
}

fn draw_settings(app: &App, rx: i32, rw: usize, rows: i32) {
    let mon = app.mon();
    mvprintw(4, rx, &pad(&format!("  Make:  {}", mon.make),  rw));
    mvprintw(5, rx, &pad(&format!("  Model: {}", mon.model), rw));
    mvprintw(6, rx, &std::iter::repeat('-').take(rw).collect::<String>());

    for f in 0..N_FIELDS {
        let y = 7 + f as i32;
        if y >= rows - 1 { break; }
        let sel = app.panel == Panel::Right && f == app.sel_field;

        let val: String = if f == F_POS {
            // Show the relative label + the computed absolute coordinates
            let (abs_x, abs_y) = resolve_abs(&mon.new_pos, mon, &app.monitors, 0);
            let computed = match &mon.new_pos {
                PosMode::Absolute(x, y) => format!("x={} y={}", x, y),
                _ => format!("{}  -> ({}, {})", mon.new_pos.display(), abs_x, abs_y),
            };
            let mark = if mon.field_dirty(f) { " [*]" } else { "" };
            format!("{}{}", computed, mark)
        } else if sel && app.ui == UiMode::TextEdit {
            format!("[{}]_", app.buf)
        } else {
            mon.field_value(f)
        };

        let line = format!(" {} {:<14} {}", if sel { ">" } else { " " }, format!("{}:", FIELD_NAMES[f]), val);
        if sel { attron(COLOR_PAIR(CP_SELECTED) | A_BOLD()); }
        else if mon.field_dirty(f) { attron(COLOR_PAIR(CP_MODIFIED)); }
        mvprintw(y, rx, &pad(&line, rw));
        if sel { attroff(COLOR_PAIR(CP_SELECTED) | A_BOLD()); }
        else if mon.field_dirty(f) { attroff(COLOR_PAIR(CP_MODIFIED)); }
    }

    // Layout preview below the fields
    let preview_start = 7 + N_FIELDS as i32 + 1;
    if preview_start < rows - 2 {
        draw_layout_preview(app, rx, preview_start, rows, rw);
    }
}

fn draw_layout_preview(app: &App, rx: i32, start_y: i32, rows: i32, rw: usize) {
    let avail_h = (rows - 1 - start_y).max(0) as usize;
    if avail_h < 4 || rw < 10 { return; }

    attron(COLOR_PAIR(CP_HEADER));
    mvprintw(start_y, rx, &pad(" Layout:", rw));
    attroff(COLOR_PAIR(CP_HEADER));

    // Compute logical positions for every monitor
    let layout: Vec<(String, i32, i32, i32, i32, bool)> = app.monitors.iter().map(|m| {
        let (ax, ay) = resolve_abs(&m.new_pos, m, &app.monitors, 0);
        let (lw, lh) = logical_size(m);
        let sel = std::ptr::eq(m, app.mon());
        (m.name.clone(), ax, ay, lw, lh, sel)
    }).collect();

    let min_x = layout.iter().map(|e| e.1).min().unwrap_or(0);
    let min_y = layout.iter().map(|e| e.2).min().unwrap_or(0);
    let max_x = layout.iter().map(|e| e.1 + e.3).max().unwrap_or(1);
    let max_y = layout.iter().map(|e| e.2 + e.4).max().unwrap_or(1);
    let total_w = (max_x - min_x).max(1) as f64;
    let total_h = (max_y - min_y).max(1) as f64;

    // Scale to fit; terminal cells are ~2× wider than tall, so compensate
    let avail_w = (rw as f64 - 2.0).max(4.0);
    let avail_hf = (avail_h as f64 - 1.0).max(1.0); // -1 for label row
    let sx = avail_w   / total_w;
    let sy = avail_hf  / total_h;
    // Equalize visual aspect: one row ≈ 2 columns in terminal cells
    let scale = sx.min(sy * 2.0).min(sy);

    for (name, mx, my, mw, mh, is_sel) in &layout {
        let bx = ((*mx - min_x) as f64 * scale).round() as i32;
        let by = ((*my - min_y) as f64 * scale).round() as i32;
        let bw = ((*mw as f64) * scale).round() as i32;
        let bh = ((*mh as f64) * scale * 0.5).round() as i32; // halve height to compensate cell ratio
        let bw = bw.max(6);
        let bh = bh.max(3);

        let x0 = rx + 1 + bx;
        let y0 = start_y + 1 + by;
        let x1 = x0 + bw - 1;
        let y1 = y0 + bh - 1;

        if y0 >= rows - 1 || x0 >= rx + rw as i32 { continue; }

        if *is_sel { attron(COLOR_PAIR(CP_SELECTED)); }

        // Top edge
        if y0 < rows - 1 {
            mvprintw(y0, x0, "+");
            for dx in 1..bw - 1 { if x0 + dx < x1 { mvprintw(y0, x0 + dx, "-"); } }
            if x1 < rx + rw as i32 && x1 > x0 { mvprintw(y0, x1, "+"); }
        }
        // Sides + label
        for dy in 1..bh - 1 {
            let y = y0 + dy;
            if y >= rows - 1 { break; }
            mvprintw(y, x0, "|");
            if x1 < rx + rw as i32 { mvprintw(y, x1, "|"); }
            if dy == bh / 2 {
                let space = (bw - 2).max(0) as usize;
                let label = &name[..name.len().min(space)];
                let pad_l = (space - label.len()) / 2;
                let centered = format!("{:>width$}", label, width = label.len() + pad_l);
                mvprintw(y, x0 + 1, &centered[..centered.len().min(space)]);
            }
        }
        // Bottom edge
        if y1 < rows - 1 && y1 > y0 {
            mvprintw(y1, x0, "+");
            for dx in 1..bw - 1 { if x0 + dx < x1 { mvprintw(y1, x0 + dx, "-"); } }
            if x1 < rx + rw as i32 && x1 > x0 { mvprintw(y1, x1, "+"); }
        }

        if *is_sel { attroff(COLOR_PAIR(CP_SELECTED)); }
    }
}

fn draw_mode_overlay(app: &App, scroll: usize, hover: usize, rx: i32, rows: i32, rw: usize) {
    let mon = app.mon();
    let visible = (rows - 4 - 2).max(1) as usize;

    attron(COLOR_PAIR(CP_OVERLAY) | A_BOLD());
    mvprintw(3, rx, &pad(&format!(" Mode for {} ({} available)", mon.name, mon.modes.len()), rw));
    attroff(COLOR_PAIR(CP_OVERLAY) | A_BOLD());

    for i in 0..visible {
        let mi = scroll + i;
        let y  = 4 + i as i32;
        if y >= rows - 1 { break; }
        if mi >= mon.modes.len() { mvprintw(y, rx, &pad("", rw)); continue; }
        let m = &mon.modes[mi];
        let flags = format!("{}{}",
            if m.preferred { " (preferred)" } else { "" },
            if m.current   { " (current)"   } else { "" });
        let line = format!(" {} {}{}", if mi == hover { ">" } else { " " }, m, flags);
        if mi == hover    { attron(COLOR_PAIR(CP_SELECTED) | A_BOLD()); }
        else if m.current { attron(COLOR_PAIR(CP_MODIFIED)); }
        mvprintw(y, rx, &pad(&line, rw));
        if mi == hover    { attroff(COLOR_PAIR(CP_SELECTED) | A_BOLD()); }
        else if m.current { attroff(COLOR_PAIR(CP_MODIFIED)); }
    }

    if mon.modes.len() > visible {
        let info = format!(" {}/{} | Up/Dn to scroll ", hover + 1, mon.modes.len());
        attron(COLOR_PAIR(CP_HEADER));
        mvprintw(rows - 2, rx, &pad(&info, rw));
        attroff(COLOR_PAIR(CP_HEADER));
    }
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

        9 => { app.panel = if app.panel == Panel::Left { Panel::Right } else { Panel::Left }; app.status.clear(); }

        KEY_UP => {
            app.status.clear();
            if app.panel == Panel::Left { app.sel_mon = app.sel_mon.saturating_sub(1); }
            else { app.sel_field = app.sel_field.saturating_sub(1); }
        }
        KEY_DOWN => {
            app.status.clear();
            if app.panel == Panel::Left { if app.sel_mon + 1 < app.monitors.len() { app.sel_mon += 1; } }
            else if app.sel_field + 1 < N_FIELDS { app.sel_field += 1; }
        }
        KEY_LEFT  => { app.status.clear(); if app.panel == Panel::Right { cycle_field(app, -1); } }
        KEY_RIGHT => { app.status.clear(); if app.panel == Panel::Right { cycle_field(app,  1); } }

        10 | KEY_ENTER => {
            app.status.clear();
            if app.panel == Panel::Left { app.panel = Panel::Right; }
            else {
                match app.sel_field {
                    F_MODE => { let h = app.mon().new_mode; app.ui = UiMode::ModeSelect { scroll: h.saturating_sub(5), hover: h }; }
                    F_POS => {
                        app.buf = match &app.mon().new_pos {
                            PosMode::Absolute(x, y) => format!("{},{}", x, y),
                            _ => { let (x,y) = resolve_abs(&app.mon().new_pos, app.mon(), &app.monitors, 0); format!("{},{}", x, y) }
                        };
                        app.ui = UiMode::TextEdit;
                    }
                    F_SCALE    => { app.buf = format!("{:.4}", app.mon().new_scale); app.ui = UiMode::TextEdit; }
                    F_ADAPTIVE => { let v = app.mon().new_adaptive; app.mon_mut().new_adaptive = !v; }
                    F_ENABLED  => { let v = app.mon().new_enabled;  app.mon_mut().new_enabled  = !v; }
                    _ => {}
                }
            }
        }

        27 => { app.ui = UiMode::Normal; }

        // p / , / . → cycle projection
        112 | 46 => { let p = app.proj.step( 1); app.proj = p; apply_proj(&mut app.monitors, p); app.set_status(format!("Projection: {}", p.label()), false); }
        44        => { let p = app.proj.step(-1); app.proj = p; apply_proj(&mut app.monitors, p); app.set_status(format!("Projection: {}", p.label()), false); }

        97  => { // a: apply
            let res = do_apply(&app.monitors);
            match res {
                Ok(msg) => { let fresh = parse_monitors(); app.monitors = fresh; app.sel_mon = app.sel_mon.min(app.monitors.len().saturating_sub(1)); app.proj = Proj::Custom; app.set_status(msg, false); }
                Err(e)  => app.set_status(e, true),
            }
        }
        114 => { app.mon_mut().reset(); app.set_status("Changes reset", false); } // r

        _ => {}
    }
    true
}

fn cycle_field(app: &mut App, dir: i32) {
    let sel = app.sel_field;
    let idx = app.sel_mon;
    let mon_name = app.monitors[idx].name.clone();
    let others: Vec<String> = app.monitors.iter().filter(|m| m.name != mon_name).map(|m| m.name.clone()).collect();
    let m = &mut app.monitors[idx];
    match sel {
        F_MODE => {
            let n = m.modes.len() as i32;
            m.new_mode = ((m.new_mode as i32 + dir).rem_euclid(n)) as usize;
        }
        F_POS => {
            let opts = pos_cycle_list(&m.new_pos, &others);
            let cur  = opts.iter().position(|o| *o == m.new_pos).unwrap_or(0);
            let next = ((cur as i32 + dir).rem_euclid(opts.len() as i32)) as usize;
            m.new_pos = opts[next].clone();
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

fn handle_mode_select(app: &mut App, ch: i32) -> bool {
    let n = app.monitors[app.sel_mon].modes.len();
    let mut rows = 0i32; let mut _c = 0i32;
    getmaxyx(stdscr(), &mut rows, &mut _c);
    let visible = (rows - 4 - 2).max(1) as usize;
    let (mut scroll, mut hover) = match app.ui { UiMode::ModeSelect { scroll, hover } => (scroll, hover), _ => return true };
    match ch {
        KEY_UP   => { if hover > 0       { hover -= 1; if hover < scroll { scroll = hover; } } }
        KEY_DOWN => { if hover + 1 < n   { hover += 1; if hover >= scroll + visible { scroll = hover + 1 - visible; } } }
        10 | KEY_ENTER => { app.monitors[app.sel_mon].new_mode = hover; app.ui = UiMode::Normal; return true; }
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
            match app.sel_field {
                F_POS => {
                    let p: Vec<&str> = buf.splitn(2, ',').collect();
                    if p.len() == 2 {
                        match (p[0].trim().parse::<i32>(), p[1].trim().parse::<i32>()) {
                            (Ok(x), Ok(y)) => app.mon_mut().new_pos = PosMode::Absolute(x, y),
                            _ => app.set_status("Invalid — use: x,y", true),
                        }
                    } else { app.set_status("Format: x,y  e.g. 3440,0", true); }
                }
                F_SCALE => match buf.trim().parse::<f64>() {
                    Ok(v) if v > 0.0 && v <= 4.0 => app.mon_mut().new_scale = v,
                    Ok(_)  => app.set_status("Scale: 0.25 to 4.0", true),
                    Err(_) => app.set_status("Invalid number", true),
                },
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

    initscr(); start_color(); cbreak(); noecho();
    keypad(stdscr(), true);
    curs_set(CURSOR_VISIBILITY::CURSOR_INVISIBLE);

    init_pair(CP_NORMAL,    COLOR_WHITE,  COLOR_BLACK);
    init_pair(CP_HEADER,    COLOR_BLACK,  COLOR_CYAN);
    init_pair(CP_SELECTED,  COLOR_BLACK,  COLOR_GREEN);
    init_pair(CP_MODIFIED,  COLOR_YELLOW, COLOR_BLACK);
    init_pair(CP_STATUSBAR, COLOR_BLACK,  COLOR_WHITE);
    init_pair(CP_ERROR,     COLOR_WHITE,  COLOR_RED);
    init_pair(CP_OVERLAY,   COLOR_BLACK,  COLOR_MAGENTA);

    let mut app = App::new(monitors);
    loop { draw(&app); let ch = getch(); if !handle(&mut app, ch) { break; } }
    endwin();
}
