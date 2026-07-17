use crate::consts::*;

#[derive(Clone, PartialEq)]
pub enum Align {
    Start,
    Center,
}

impl Align {
    pub fn label(&self) -> &'static str {
        match self {
            Align::Start => "",
            Align::Center => " (center)",
        }
    }
}

/// Relative positions carry an alignment on the perpendicular axis:
///   RightOf / LeftOf → align vertically (top vs. centered)
///   Above   / Below  → align horizontally (left vs. centered)
#[derive(Clone, PartialEq)]
pub enum PosMode {
    RightOf(String, Align),
    LeftOf(String, Align),
    Above(String, Align),
    Below(String, Align),
    Absolute(i32, i32),
}

impl PosMode {
    pub fn display(&self) -> String {
        match self {
            PosMode::RightOf(n, a) => format!("Right of {}{}", n, a.label()),
            PosMode::LeftOf(n, a) => format!("Left of {}{}", n, a.label()),
            PosMode::Above(n, a) => format!("Above {}{}", n, a.label()),
            PosMode::Below(n, a) => format!("Below {}{}", n, a.label()),
            PosMode::Absolute(x, y) => format!("x={} y={}", x, y),
        }
    }

    pub fn is_absolute_eq(&self, pos: (i32, i32)) -> bool {
        matches!(self, PosMode::Absolute(x, y) if (*x, *y) == pos)
    }
}

/// Logical size = mode pixels ÷ scale factor (compositor layout coordinates).
pub fn logical_size(m: &Monitor) -> (i32, i32) {
    let md = &m.modes[m.new_mode];
    (
        (md.width as f64 / m.new_scale).round() as i32,
        (md.height as f64 / m.new_scale).round() as i32,
    )
}

/// Recursively resolve a PosMode to absolute (x, y) layout coordinates.
/// Falls back to the monitor's reported position on circular reference or missing ref.
pub fn resolve_abs(pos: &PosMode, this: &Monitor, all: &[Monitor], depth: u8) -> (i32, i32) {
    if depth > 8 {
        return this.position;
    }
    match pos {
        PosMode::Absolute(x, y) => (*x, *y),
        PosMode::RightOf(ref_name, _)
        | PosMode::LeftOf(ref_name, _)
        | PosMode::Above(ref_name, _)
        | PosMode::Below(ref_name, _) => {
            let rm = match all.iter().find(|m| &m.name == ref_name) {
                Some(m) => m,
                None => return this.position,
            };
            let rp = resolve_abs(&rm.new_pos, rm, all, depth + 1);
            let (rw, rh) = logical_size(rm);
            let (sw, sh) = logical_size(this);
            match pos {
                PosMode::RightOf(_, a) => (
                    rp.0 + rw,
                    rp.1 + if *a == Align::Center {
                        (rh - sh) / 2
                    } else {
                        0
                    },
                ),
                PosMode::LeftOf(_, a) => (
                    rp.0 - sw,
                    rp.1 + if *a == Align::Center {
                        (rh - sh) / 2
                    } else {
                        0
                    },
                ),
                PosMode::Above(_, a) => (
                    rp.0 + if *a == Align::Center {
                        (rw - sw) / 2
                    } else {
                        0
                    },
                    rp.1 - sh,
                ),
                PosMode::Below(_, a) => (
                    rp.0 + if *a == Align::Center {
                        (rw - sw) / 2
                    } else {
                        0
                    },
                    rp.1 + rh,
                ),
                _ => unreachable!(),
            }
        }
    }
}

/// Build the ordered list of position options for cycling through in the UI.
pub fn pos_cycle_list(cur: &PosMode, others: &[String]) -> Vec<PosMode> {
    let mut opts: Vec<PosMode> = Vec::new();
    for o in others {
        opts.push(PosMode::Below(o.clone(), Align::Start));
        opts.push(PosMode::Below(o.clone(), Align::Center));
        opts.push(PosMode::RightOf(o.clone(), Align::Start));
        opts.push(PosMode::RightOf(o.clone(), Align::Center));
        opts.push(PosMode::Above(o.clone(), Align::Start));
        opts.push(PosMode::Above(o.clone(), Align::Center));
        opts.push(PosMode::LeftOf(o.clone(), Align::Start));
        opts.push(PosMode::LeftOf(o.clone(), Align::Center));
    }
    let abs = match cur {
        PosMode::Absolute(x, y) => PosMode::Absolute(*x, *y),
        _ => PosMode::Absolute(0, 0),
    };
    if !opts.contains(&abs) {
        opts.push(abs);
    }
    opts
}

#[derive(Clone)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub refresh: f64,
    pub preferred: bool,
    pub current: bool,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{} @ {:.3} Hz", self.width, self.height, self.refresh)
    }
}

#[derive(Clone)]
pub struct Monitor {
    pub name: String,
    pub make: String,
    pub model: String,
    pub enabled: bool,
    pub modes: Vec<Mode>,
    pub cur_mode: usize,
    pub position: (i32, i32),
    pub transform: String,
    pub scale: f64,
    pub adaptive_sync: bool,
    // Pending (unsaved) state
    pub new_mode: usize,
    pub new_pos: PosMode,
    pub new_transform: String,
    pub new_scale: f64,
    pub new_adaptive: bool,
    pub new_enabled: bool,
}

impl Monitor {
    pub fn is_laptop(&self) -> bool {
        self.name.starts_with("eDP")
    }

    pub fn field_dirty(&self, f: usize) -> bool {
        match f {
            F_MODE => self.new_mode != self.cur_mode,
            F_SCALE => (self.new_scale - self.scale).abs() > 0.0001,
            F_TRANSFORM => self.new_transform != self.transform,
            F_ADAPTIVE => self.new_adaptive != self.adaptive_sync,
            F_ENABLED => self.new_enabled != self.enabled,
            _ => false,
        }
    }

    pub fn pos_dirty(&self) -> bool {
        !self.new_pos.is_absolute_eq(self.position)
    }

    pub fn dirty(&self) -> bool {
        self.pos_dirty() || (0..N_FIELDS).any(|f| self.field_dirty(f))
    }

    pub fn reset(&mut self) {
        self.new_mode = self.cur_mode;
        self.new_pos = PosMode::Absolute(self.position.0, self.position.1);
        self.new_transform = self.transform.clone();
        self.new_scale = self.scale;
        self.new_adaptive = self.adaptive_sync;
        self.new_enabled = self.enabled;
    }

    pub fn field_value(&self, f: usize) -> String {
        let mark = |b: bool| if b { " [*]" } else { "" };
        match f {
            F_MODE => {
                let md = &self.modes[self.new_mode];
                format!(
                    "{}{}{}{}",
                    md,
                    if md.preferred { " (preferred)" } else { "" },
                    if md.current { " (current)" } else { "" },
                    mark(self.field_dirty(f))
                )
            }
            F_SCALE => format!("{:.4}{}", self.new_scale, mark(self.field_dirty(f))),
            F_TRANSFORM => format!("{}{}", self.new_transform, mark(self.field_dirty(f))),
            F_ADAPTIVE => format!(
                "{}{}",
                if self.new_adaptive {
                    "enabled"
                } else {
                    "disabled"
                },
                mark(self.field_dirty(f))
            ),
            F_ENABLED => format!(
                "{}{}",
                if self.new_enabled { "yes" } else { "no" },
                mark(self.field_dirty(f))
            ),
            _ => String::new(),
        }
    }
}
