//! Compositor metadata is converted to logical geometry at this boundary.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::process::Command;
use std::time::Duration;

use crate::geometry::Geometry;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MonitorInfo {
    pub name: String,
    pub scale: f64,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl MonitorInfo {
    pub fn geometry(&self) -> Result<Geometry> {
        Geometry::new(self.x, self.y, self.width, self.height)
    }
}

pub(crate) fn get_monitor_info_for_geometry(
    geometry: &Geometry,
    debug: bool,
) -> Result<MonitorInfo> {
    let (mut command, parse): (_, fn(&Value) -> Option<MonitorInfo>) =
        if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
            let mut command = Command::new("hyprctl");
            command.args(["monitors", "-j"]);
            (command, monitor_info_from_hypr)
        } else if std::env::var_os("SWAYSOCK").is_some() {
            let mut command = Command::new("swaymsg");
            command.args(["-t", "get_outputs", "-r"]);
            (command, monitor_info_from_sway)
        } else {
            anyhow::bail!("Cannot identify compositor; no output will be guessed for recording");
        };
    command.stdin(std::process::Stdio::null());
    let output = crate::utils::output_with_timeout(command, Duration::from_secs(3))?;
    ensure!(
        output.status.success(),
        "Monitor query failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let values: Vec<Value> =
        serde_json::from_slice(&output.stdout).context("Invalid monitor metadata")?;
    let monitor = find_monitor_for_geometry(&values, geometry, parse)
        .context("Selected region must fit entirely within one enabled output; cross-output recording is not supported")?;
    if debug {
        eprintln!("Selected output: {monitor:?}");
    }
    Ok(monitor)
}

fn find_monitor_for_geometry(
    outputs: &[Value],
    geometry: &Geometry,
    parse: fn(&Value) -> Option<MonitorInfo>,
) -> Option<MonitorInfo> {
    outputs.iter().filter_map(parse).find(|monitor| {
        monitor
            .geometry()
            .is_ok_and(|bounds| bounds.contains(geometry))
    })
}

pub(crate) fn monitor_info_from_hypr(value: &Value) -> Option<MonitorInfo> {
    if value["disabled"].as_bool() == Some(true) {
        return None;
    }
    let scale = value["scale"].as_f64()?;
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let mut width = value["width"].as_f64()?;
    let mut height = value["height"].as_f64()?;
    let transform = value["transform"].as_u64().unwrap_or(0);
    if transform > 7 {
        return None;
    }
    if transform % 2 == 1 {
        std::mem::swap(&mut width, &mut height);
    }
    // hyprctl reports mode pixels, not m_size. Transform first, then scale.
    let width = (width / scale).round();
    let height = (height / scale).round();
    if !(1.0..=i32::MAX as f64).contains(&width) || !(1.0..=i32::MAX as f64).contains(&height) {
        return None;
    }
    let monitor = MonitorInfo {
        name: value["name"].as_str()?.to_owned(),
        scale,
        x: i32::try_from(value["x"].as_i64()?).ok()?,
        y: i32::try_from(value["y"].as_i64()?).ok()?,
        width: width as i32,
        height: height as i32,
    };
    monitor.geometry().ok()?;
    Some(monitor)
}

fn monitor_info_from_sway(value: &Value) -> Option<MonitorInfo> {
    if value["active"].as_bool() == Some(false) {
        return None;
    }
    let rect = &value["rect"];
    let scale = value["scale"].as_f64()?;
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let monitor = MonitorInfo {
        name: value["name"].as_str()?.to_owned(),
        scale,
        x: i32::try_from(rect["x"].as_i64()?).ok()?,
        y: i32::try_from(rect["y"].as_i64()?).ok()?,
        width: i32::try_from(rect["width"].as_i64()?).ok()?,
        height: i32::try_from(rect["height"].as_i64()?).ok()?,
    };
    monitor.geometry().ok()?;
    Some(monitor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn monitors() -> Vec<Value> {
        vec![
            json!({"name":"eDP-1","x":0,"y":0,"width":2560,"height":1600,"scale":2.0}),
            json!({"name":"HDMI-A-1","x":1280,"y":0,"width":1920,"height":1080,"scale":1.0}),
        ]
    }

    #[test]
    fn scaled_primary_does_not_shadow_adjacent_output() {
        let selected = Geometry::new(1400, 100, 300, 200).unwrap();
        let monitor =
            find_monitor_for_geometry(&monitors(), &selected, monitor_info_from_hypr).unwrap();
        assert_eq!(monitor.name, "HDMI-A-1");
        assert_eq!(monitor_info_from_hypr(&monitors()[0]).unwrap().width, 1280);
    }

    #[test]
    fn rotation_and_fractional_scale_are_applied_once() {
        let output = json!({"name":"DP-1","x":-1440,"y":-200,"width":3840,"height":2160,"scale":1.5,"transform":1});
        let monitor = monitor_info_from_hypr(&output).unwrap();
        assert_eq!(
            monitor.geometry().unwrap(),
            Geometry::new(-1440, -200, 1440, 2560).unwrap()
        );
    }

    #[test]
    fn cross_output_and_invalid_scale_are_rejected() {
        assert!(
            find_monitor_for_geometry(
                &monitors(),
                &Geometry::new(1200, 0, 200, 100).unwrap(),
                monitor_info_from_hypr
            )
            .is_none()
        );
        let mut monitor = monitors().remove(0);
        monitor["scale"] = json!(0);
        assert!(monitor_info_from_hypr(&monitor).is_none());
    }

    #[test]
    fn sway_rect_is_already_logical() {
        let output =
            json!({"name":"DP-1","scale":2.0,"rect":{"x":0,"y":0,"width":1280,"height":800}});
        assert_eq!(monitor_info_from_sway(&output).unwrap().width, 1280);
    }
}
