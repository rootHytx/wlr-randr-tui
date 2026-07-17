use std::process::Command;

use crate::monitor::{Mode, Monitor, PosMode};

pub fn parse_monitors() -> Vec<Monitor> {
    let out = Command::new("wlr-randr").output().unwrap_or_else(|_| {
        eprintln!("error: wlr-randr not found");
        std::process::exit(1);
    });
    parse_output(&String::from_utf8_lossy(&out.stdout))
}

pub fn parse_output(text: &str) -> Vec<Monitor> {
    let mut result = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if line.starts_with(' ') || line.is_empty() {
            continue;
        }
        let name = match line.split_whitespace().next() {
            Some(n) => n.to_string(),
            None => continue,
        };
        let (mut make, mut model) = (String::new(), String::new());
        let mut enabled = true;
        let mut modes: Vec<Mode> = Vec::new();
        let mut pos = (0i32, 0i32);
        let mut transform = "normal".to_string();
        let mut scale = 1.0f64;
        let mut adaptive = false;
        let mut in_modes = false;

        loop {
            match lines.peek() {
                Some(l) if l.starts_with("  ") || l.starts_with('\t') => {}
                _ => break,
            }
            let t = lines.next().unwrap();
            let t = t.trim();
            if t.starts_with("Make: ") {
                make = t["Make: ".len()..].to_string();
                in_modes = false;
            } else if t.starts_with("Model: ") {
                model = t["Model: ".len()..].to_string();
                in_modes = false;
            } else if t.starts_with("Enabled: ") {
                enabled = t.ends_with("yes");
                in_modes = false;
            } else if t == "Modes:" {
                in_modes = true;
            } else if t.starts_with("Position: ") {
                in_modes = false;
                let s = &t["Position: ".len()..];
                let mut p = s.splitn(2, ',');
                pos = (
                    p.next().unwrap_or("0").trim().parse().unwrap_or(0),
                    p.next().unwrap_or("0").trim().parse().unwrap_or(0),
                );
            } else if t.starts_with("Transform: ") {
                transform = t["Transform: ".len()..].to_string();
                in_modes = false;
            } else if t.starts_with("Scale: ") {
                scale = t["Scale: ".len()..].trim().parse().unwrap_or(1.0);
                in_modes = false;
            } else if t.starts_with("Adaptive Sync: ") {
                adaptive = t.ends_with("enabled");
                in_modes = false;
            } else if in_modes && t.contains("px,") {
                if let Some(m) = parse_mode(t) {
                    modes.push(m);
                }
            }
        }

        if modes.is_empty() {
            continue;
        }
        let cur = modes.iter().position(|m| m.current).unwrap_or(0);
        result.push(Monitor {
            name,
            make,
            model,
            enabled,
            modes,
            cur_mode: cur,
            position: pos,
            transform: transform.clone(),
            scale,
            adaptive_sync: adaptive,
            new_mode: cur,
            new_pos: PosMode::Absolute(pos.0, pos.1),
            new_transform: transform,
            new_scale: scale,
            new_adaptive: adaptive,
            new_enabled: enabled,
        });
    }
    result
}

fn parse_mode(t: &str) -> Option<Mode> {
    let w: Vec<&str> = t.split_whitespace().collect();
    if w.len() < 4 {
        return None;
    }
    let d: Vec<&str> = w[0].split('x').collect();
    if d.len() != 2 {
        return None;
    }
    Some(Mode {
        width: d[0].parse().ok()?,
        height: d[1].parse().ok()?,
        refresh: w[2].parse().ok()?,
        preferred: t.contains("preferred"),
        current: t.contains("current"),
    })
}
