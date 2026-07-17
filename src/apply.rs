use std::process::Command;

use crate::monitor::{resolve_abs, Monitor};

pub fn build_args(m: &Monitor, all: &[Monitor]) -> Vec<String> {
    let mut a = vec!["--output".to_string(), m.name.clone()];
    if !m.new_enabled {
        a.push("--off".to_string());
        return a;
    }
    if !m.enabled {
        a.push("--on".to_string());
    }
    if m.new_mode != m.cur_mode {
        let md = &m.modes[m.new_mode];
        a.extend([
            "--mode".to_string(),
            format!("{}x{}@{:.3}Hz", md.width, md.height, md.refresh),
        ]);
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
        a.extend([
            "--adaptive-sync".to_string(),
            if m.new_adaptive {
                "enabled"
            } else {
                "disabled"
            }
            .to_string(),
        ]);
    }
    a
}

pub fn do_apply(monitors: &[Monitor]) -> Result<String, String> {
    if !monitors.iter().any(|m| m.dirty()) {
        return Ok("No changes to apply".to_string());
    }
    // Send the complete layout every time — never a partial update.
    // This ensures the compositor gets a consistent picture of all outputs.
    let args: Vec<String> = monitors
        .iter()
        .flat_map(|m| build_args(m, monitors))
        .collect();
    let cmd = format!("wlr-randr {}", args.join(" "));
    let out = Command::new("wlr-randr")
        .args(&args)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(cmd)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}
