//! Persistent per-monitor profiles.
//!
//! A profile is keyed by hardware identity (EDID make/model/serial, or the
//! physical size when the serial is missing or a placeholder), not by the
//! connector name. Connector names change between boots and docking stations,
//! hardware does not. Profiles are written after every successful apply and
//! re-applied at startup.

use std::fs;
use std::path::{Path, PathBuf};

use crate::consts::TRANSFORMS;
use crate::monitor::{resolve_abs, Monitor, PosMode};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Saved {
    pub key: String,
    pub name: String,
    pub make: String,
    pub model: String,
    pub serial: String,
    pub physical: (u32, u32),
    pub enabled: bool,
    /// (width, height, refresh in Hz)
    pub mode: Option<(u32, u32, f64)>,
    pub scale: f64,
    pub transform: String,
    pub adaptive: bool,
    pub pos: (i32, i32),
}

impl Saved {
    fn defaults(key: String) -> Self {
        Saved {
            key,
            enabled: true,
            scale: 1.0,
            transform: "normal".to_string(),
            ..Saved::default()
        }
    }
}

/// `$WLR_RANDR_TUI_CONFIG`, else `$XDG_CONFIG_HOME/wlr-randr-tui/config`,
/// else `$HOME/.config/wlr-randr-tui/config`.
pub fn config_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("WLR_RANDR_TUI_CONFIG") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(|h| PathBuf::from(h).join(".config"))
        })?;
    Some(base.join("wlr-randr-tui").join("config"))
}

fn norm(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Serial numbers of the form `0`, `00000000`, `0x00000000`, ... are
/// placeholders reported by plenty of panels and MST hubs. They must not be
/// part of the identity, or every such monitor would share a profile.
fn serial_is_usable(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .any(|c| c != '0' && c != 'x' && c != 'X' && c != '-' && c != ' ')
}

/// Stable 64-bit FNV-1a. `DefaultHasher` is explicitly not stable across Rust
/// releases, and this value is written to disk.
fn fnv1a64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Hardware identity of an output. Deliberately independent of `Monitor::name`.
pub fn key(m: &Monitor) -> String {
    let make = norm(&m.make);
    let model = norm(&m.model);
    let serial = norm(&m.serial);
    let (pw, ph) = m.physical_size;

    let mut parts: Vec<String> = Vec::new();
    if !make.is_empty() {
        parts.push(make);
    }
    if !model.is_empty() {
        parts.push(model);
    }
    if serial_is_usable(&serial) {
        parts.push(serial);
    } else if pw > 0 && ph > 0 {
        parts.push(format!("{}x{}", pw, ph));
    }
    if parts.is_empty() {
        // No EDID data at all (headless/virtual output): the connector name is
        // the only stable thing left.
        return format!("name:{}", norm(&m.name));
    }
    format!("{:016x}", fnv1a64(&parts.join("|")))
}

fn mode_eq(a: f64, b: f64) -> bool {
    (a * 1000.0).round() == (b * 1000.0).round()
}

/// Apply saved profiles to the matching monitors, writing into the pending
/// (`new_*`) fields only. Returns how many monitors matched a profile.
/// Values that are no longer valid (removed mode, unknown transform) are
/// skipped instead of being applied blindly.
pub fn restore(monitors: &mut [Monitor], saved: &[Saved]) -> usize {
    let mut restored = 0;
    for m in monitors.iter_mut() {
        let k = key(m);
        let Some(s) = saved.iter().find(|s| s.key == k) else {
            continue;
        };
        if let Some((w, h, r)) = s.mode {
            if let Some(i) = m
                .modes
                .iter()
                .position(|md| md.width == w && md.height == h && mode_eq(md.refresh, r))
            {
                m.new_mode = i;
            }
        }
        if s.scale >= 0.25 && s.scale <= 4.0 {
            m.new_scale = s.scale;
        }
        if TRANSFORMS.contains(&s.transform.as_str()) {
            m.new_transform = s.transform.clone();
        }
        m.new_adaptive = s.adaptive;
        m.new_enabled = s.enabled;
        m.new_pos = PosMode::Absolute(s.pos.0, s.pos.1);
        restored += 1;
    }
    restored
}

/// Persist the current state of every connected monitor. Entries for monitors
/// that are not connected right now are kept, so a docked-at-work profile is
/// not destroyed by saving at home.
pub fn save(path: &Path, monitors: &[Monitor]) -> Result<(), String> {
    let mut entries = read_existing(path)?;
    for m in monitors {
        let k = key(m);
        let (px, py) = resolve_abs(&m.new_pos, m, monitors, 0);
        let md = &m.modes[m.new_mode];
        let entry = Saved {
            key: k.clone(),
            name: m.name.clone(),
            make: m.make.clone(),
            model: m.model.clone(),
            serial: m.serial.clone(),
            physical: m.physical_size,
            enabled: m.new_enabled,
            mode: Some((md.width, md.height, md.refresh)),
            scale: m.new_scale,
            transform: m.new_transform.clone(),
            adaptive: m.new_adaptive,
            pos: (px, py),
        };
        match entries.iter_mut().find(|e| e.key == k) {
            Some(e) => *e = entry,
            None => entries.push(entry),
        }
    }
    write_entries(path, &entries)
}

/// Remove the saved profile for one hardware identity.
pub fn forget(path: &Path, key: &str) -> Result<bool, String> {
    let mut entries = read_existing(path)?;
    let before = entries.len();
    entries.retain(|e| e.key != key);
    if entries.len() == before {
        return Ok(false);
    }
    write_entries(path, &entries)?;
    Ok(true)
}

/// Loaded profiles. A missing or unreadable file simply means "nothing saved";
/// startup must never fail because of it.
pub fn load(path: &Path) -> Vec<Saved> {
    read_existing(path).unwrap_or_default()
}

fn read_existing(path: &Path) -> Result<Vec<Saved>, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(parse(&text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {}", path.display(), e)),
    }
}

fn write_entries(path: &Path, entries: &[Saved]) -> Result<(), String> {
    let mut body =
        String::from("# wlr-randr-tui layout profiles. Written on apply; safe to edit.\n");
    for e in entries {
        body.push('\n');
        body.push_str(&entry_to_string(e));
    }
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
        }
    }
    // Write-then-rename so a crash mid-write cannot truncate a good config.
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, body).map_err(|e| format!("{}: {}", tmp.display(), e))?;
    fs::rename(&tmp, path).map_err(|e| format!("{}: {}", path.display(), e))
}

fn entry_to_string(s: &Saved) -> String {
    let label = [s.make.as_str(), s.model.as_str(), s.serial.as_str()]
        .iter()
        .filter(|p| !p.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    let mode = match s.mode {
        Some((w, h, r)) => format!("{}x{}@{:.6}", w, h, r),
        None => String::new(),
    };
    format!(
        "# {} {}\n[monitor]\nkey={}\nname={}\nmake={}\nmodel={}\nserial={}\nphysical={}x{}\nenabled={}\nmode={}\nscale={:.6}\ntransform={}\nadaptive={}\npos={},{}\n",
        s.name,
        if label.is_empty() { "(unknown)" } else { &label },
        s.key,
        s.name,
        s.make,
        s.model,
        s.serial,
        s.physical.0,
        s.physical.1,
        if s.enabled { "yes" } else { "no" },
        mode,
        s.scale,
        s.transform,
        if s.adaptive { "enabled" } else { "disabled" },
        s.pos.0,
        s.pos.1,
    )
}

fn parse_pair<T: std::str::FromStr>(v: &str, sep: char) -> Option<(T, T)> {
    let (a, b) = v.split_once(sep)?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

fn parse_mode(v: &str) -> Option<(u32, u32, f64)> {
    let (wh, r) = v.split_once('@')?;
    let (w, h) = wh.split_once('x')?;
    Some((
        w.trim().parse().ok()?,
        h.trim().parse().ok()?,
        r.trim().parse().ok()?,
    ))
}

fn parse(text: &str) -> Vec<Saved> {
    let mut out = Vec::new();
    let mut cur: Option<Saved> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[monitor]" {
            if let Some(s) = cur.take() {
                if !s.key.is_empty() {
                    out.push(s);
                }
            }
            cur = Some(Saved::defaults(String::new()));
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let Some(s) = cur.as_mut() else {
            continue;
        };
        let v = v.trim();
        match k.trim() {
            "key" => s.key = v.to_string(),
            "name" => s.name = v.to_string(),
            "make" => s.make = v.to_string(),
            "model" => s.model = v.to_string(),
            "serial" => s.serial = v.to_string(),
            "physical" => s.physical = parse_pair::<u32>(v, 'x').unwrap_or((0, 0)),
            "enabled" => s.enabled = v != "no",
            "mode" => s.mode = parse_mode(v),
            "scale" => {
                if let Ok(x) = v.parse::<f64>() {
                    if x > 0.0 {
                        s.scale = x;
                    }
                }
            }
            "transform" => s.transform = v.to_string(),
            "adaptive" => s.adaptive = v == "enabled" || v == "yes",
            "pos" => s.pos = parse_pair::<i32>(v, ',').unwrap_or((0, 0)),
            _ => {}
        }
    }
    if let Some(s) = cur {
        if !s.key.is_empty() {
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_output;

    const SAMPLE: &str = r#"DP-2 "PNP(AMZ) - C24M2020D - DP-2"
  Make: PNP(AMZ)
  Model: C24M2020D
  Serial: H548000289
  Physical size: 530x290 mm
  Enabled: yes
  Modes:
    1920x1080 px, 60.000000 Hz (preferred, current)
    1920x1080 px, 100.000000 Hz
  Position: 1920,0
  Transform: normal
  Scale: 1.000000
  Adaptive Sync: disabled

eDP-1 "Sharp Corporation LQ140M1JW46"
  Make: Sharp Corporation
  Model: LQ140M1JW46
  Serial: 0x00000000
  Physical size: 310x170 mm
  Enabled: yes
  Modes:
    2880x1800 px, 120.000000 Hz (preferred, current)
  Position: 0,0
  Transform: normal
  Scale: 1.750000
  Adaptive Sync: disabled
"#;

    fn sample() -> Vec<Monitor> {
        parse_output(SAMPLE)
    }

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "wlr-randr-tui-test-{}-{}.conf",
            std::process::id(),
            tag
        ))
    }

    #[test]
    fn parses_identity_fields() {
        let mons = sample();
        assert_eq!(mons.len(), 2);
        assert_eq!(mons[0].serial, "H548000289");
        assert_eq!(mons[0].physical_size, (530, 290));
        assert_eq!(mons[1].physical_size, (310, 170));
    }

    #[test]
    fn key_ignores_connector_name() {
        let mut a = sample().remove(0);
        let mut b = a.clone();
        b.name = "DP-7".to_string();
        assert_eq!(key(&a), key(&b));
        a.serial = "DIFFERENT".to_string();
        assert_ne!(key(&a), key(&b));
    }

    #[test]
    fn placeholder_serial_falls_back_to_size() {
        let mut m = sample().remove(1);
        let before = key(&m);
        m.serial = "0x00000000".to_string();
        assert_eq!(before, key(&m));
        m.name = "eDP-9".to_string();
        assert_eq!(before, key(&m));
    }

    #[test]
    fn roundtrip_and_restore() {
        let path = temp_path("roundtrip");
        let _ = fs::remove_file(&path);

        let mut mons = sample();
        mons[0].new_scale = 1.5;
        mons[0].new_adaptive = true;
        mons[0].new_transform = "90".to_string();
        mons[0].new_pos = PosMode::Absolute(3440, 0);
        save(&path, &mons).unwrap();

        let loaded = load(&path);
        assert_eq!(loaded.len(), 2);
        let dp = loaded.iter().find(|s| s.name == "DP-2").unwrap();
        assert_eq!(dp.scale, 1.5);
        assert!(dp.adaptive);
        assert_eq!(dp.transform, "90");
        assert_eq!(dp.pos, (3440, 0));
        assert_eq!(dp.mode, Some((1920, 1080, 60.0)));

        let mut fresh = sample();
        assert_eq!(restore(&mut fresh, &loaded), 2);
        assert_eq!(fresh[0].new_scale, 1.5);
        assert!(fresh[0].new_adaptive);
        assert_eq!(fresh[0].new_transform, "90");
        assert_eq!(fresh[0].new_pos, PosMode::Absolute(3440, 0));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn save_keeps_absent_monitors() {
        let path = temp_path("merge");
        let _ = fs::remove_file(&path);

        let mons = sample();
        save(&path, &mons).unwrap();
        save(&path, &mons[..1]).unwrap();

        let loaded = load(&path);
        assert_eq!(loaded.len(), 2);
        assert!(loaded.iter().any(|s| s.name == "eDP-1"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn forget_removes_one_entry() {
        let path = temp_path("forget");
        let _ = fs::remove_file(&path);

        let mons = sample();
        save(&path, &mons).unwrap();
        assert!(forget(&path, &key(&mons[0])).unwrap());
        assert!(!forget(&path, &key(&mons[0])).unwrap());
        let loaded = load(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "eDP-1");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn garbage_config_does_not_panic() {
        let text = "[monitor]\nkey=\nmode=nonsense\nscale=NaN\npos=x\n[monitor]\nkey=abc\n";
        let loaded = parse(text);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].key, "abc");
        assert_eq!(loaded[0].scale, 1.0);
    }

    #[test]
    fn unavailable_mode_is_skipped() {
        let mut mons = sample();
        let mut saved = Saved::defaults(key(&mons[0]));
        saved.mode = Some((3840, 2160, 60.0));
        assert_eq!(restore(&mut mons[..1], &[saved]), 1);
        assert_eq!(mons[0].new_mode, mons[0].cur_mode);
    }
}
